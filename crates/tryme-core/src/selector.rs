//! Interactive selector — port of `TrySelector` (`try.rb:10-953`).
//!
//! Renders to stderr; the selection result is turned into a shell script by
//! the dispatcher. All buffers are char-indexed (Ruby string semantics).

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "1:1 port of Ruby integer/float arithmetic; widths, cursor positions, and \
            list lengths are bounded by terminal size and directory counts"
)]

use crate::env::Env;
use crate::fuzzy;
use crate::naming::squeeze_ws_to_hyphen;
use crate::scan::{self, TryDir};
use crate::tui::{ansi, metrics, palette, terminal, text, Screen};
use crate::wrappers::expand_path;
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

/// What the user chose (`try.rb` result hashes).
pub enum Selection {
    /// cd into an existing try (`:cd`).
    Cd {
        /// Target path (realpath for symlinked entries).
        path: PathBuf,
    },
    /// Create + cd (`:mkdir`).
    Mkdir {
        /// New directory path.
        path: PathBuf,
    },
    /// Rename (`:rename`).
    Rename {
        /// Tries root.
        base_path: PathBuf,
        /// Old basename.
        old: String,
        /// New basename.
        new: String,
    },
    /// Graduate (`:ascend`).
    Ascend {
        /// Source path.
        source: PathBuf,
        /// Destination path.
        dest: PathBuf,
        /// Source basename (symlink name).
        basename: String,
        /// Tries root.
        base_path: PathBuf,
    },
    /// Batch delete (`:delete`) — realpath'd base + validated basenames.
    Delete {
        /// Basenames to delete (validated inside `base_path`).
        basenames: Vec<String>,
        /// Realpath of the tries root.
        base_path: PathBuf,
    },
}

struct ResultEntry {
    idx: usize,
    score: f64,
    positions: Vec<usize>,
}

/// Test-key / tty input source with the auto-ESC exhaustion rule.
struct Keys {
    test_keys: VecDeque<String>,
    test_had_keys: bool,
}

/// The selector state machine.
#[allow(
    clippy::struct_excessive_bools,
    reason = "1:1 record of TrySelector's independent state flags (try.rb:19-41)"
)]
pub struct Selector<'e> {
    env: &'e Env,
    base_path: PathBuf,
    input: Vec<char>,
    input_cursor: usize,
    cursor_pos: usize,
    scroll_offset: usize,
    selected: Option<Selection>,
    all_tries: Option<Vec<TryDir>>,
    fuzzy_entries: Option<Vec<fuzzy::Entry<usize>>>,
    last_query: Option<String>,
    cached: Option<Vec<ResultEntry>>,
    delete_status: Option<String>,
    delete_mode: bool,
    marked: Vec<PathBuf>,
    test_render_once: bool,
    test_no_cls: bool,
    keys: Keys,
    test_confirm: Option<String>,
    needs_redraw: std::sync::Arc<std::sync::atomic::AtomicBool>,
    winch_id: Option<signal_hook::SigId>,
    saved_termios: Option<rustix::termios::Termios>,
    alt_screen_active: bool,
}

/// Ruby's `ensure restore_terminal` (`try.rb:68-70`) + `STDERR.raw do..end`:
/// restoration must survive panics, or a mid-frame failure leaves the user's
/// terminal raw and stuck in the alt screen.
impl Drop for Selector<'_> {
    fn drop(&mut self) {
        self.leave_raw();
        self.restore_terminal();
    }
}

const INPUT_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_. ";

fn is_input_char(c: char) -> bool {
    INPUT_CHARS.contains(c)
}

/// `/[a-zA-Z0-9\-_\.\s\/]/` — rename dialog input filter (`try.rb:582`).
fn is_rename_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/') || c.is_whitespace()
}

/// Rename's set plus `~` (`try.rb:704`).
fn is_ascend_char(c: char) -> bool {
    is_rename_char(c) || c == '~'
}

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

/// `word_boundary_backward` (`try.rb:482-487`).
fn word_boundary_backward(buffer: &[char], cursor: usize) -> usize {
    let mut pos = cursor as i64 - 1;
    while pos >= 0 {
        #[allow(clippy::cast_sign_loss, reason = "pos >= 0 in loop")]
        let c = buffer[pos as usize];
        if word_char(c) {
            break;
        }
        pos -= 1;
    }
    while pos >= 0 {
        #[allow(clippy::cast_sign_loss, reason = "pos >= 0 in loop")]
        let c = buffer[pos as usize];
        if !word_char(c) {
            break;
        }
        pos -= 1;
    }
    #[allow(clippy::cast_sign_loss, reason = "pos+1 >= 0 always")]
    {
        (pos + 1) as usize
    }
}

/// `format_relative_time` (`try.rb:489-508`).
fn format_relative_time(now: SystemTime, mtime: SystemTime) -> String {
    let seconds = scan::seconds_between(now, mtime);
    let minutes = seconds / 60.0;
    let hours = minutes / 60.0;
    let days = hours / 24.0;
    if seconds < 60.0 {
        "just now".to_string()
    } else if minutes < 60.0 {
        format!("{}m ago", minutes as i64)
    } else if hours < 24.0 {
        format!("{}h ago", hours as i64)
    } else if days < 7.0 {
        format!("{}d ago", days as i64)
    } else {
        format!("{}w ago", (days / 7.0) as i64)
    }
}

/// `truncate_with_ansi` (`try.rb:510-531`) — simple char loop that passes
/// escape sequences through (terminated by `m`).
fn truncate_with_ansi(text: &str, max_length: usize) -> String {
    let mut visible_count = 0;
    let mut result = String::new();
    let mut in_ansi = false;
    for ch in text.chars() {
        if ch == '\x1b' {
            in_ansi = true;
            result.push(ch);
        } else if in_ansi {
            result.push(ch);
            if ch == 'm' {
                in_ansi = false;
            }
        } else {
            if visible_count >= max_length {
                break;
            }
            result.push(ch);
            visible_count += 1;
        }
    }
    result
}

/// `highlight_with_positions` (`try.rb:460-478`): batch consecutive
/// highlighted chars, positions offset into the full basename.
fn highlight_with_positions(t: &str, positions: &[usize], offset: usize) -> String {
    let chars: Vec<char> = t.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if positions.contains(&(i + offset)) {
            let batch_start = i;
            i += 1;
            while i < chars.len() && positions.contains(&(i + offset)) {
                i += 1;
            }
            let batch: String = chars[batch_start..i].iter().collect();
            result.push_str(&text::highlight(&batch));
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

impl Keys {
    /// Test-key pop + the auto-ESC exhaustion rule (`try.rb:283-287`).
    fn next_test_key(&mut self) -> Option<String> {
        if let Some(k) = self.test_keys.pop_front() {
            return Some(k);
        }
        if self.test_had_keys {
            return Some("\x1b".to_string());
        }
        None
    }
}

impl<'e> Selector<'e> {
    /// Mirror of `TrySelector#initialize` (`try.rb:19-41`).
    #[must_use]
    pub fn new(
        search_term: &str,
        base_path: PathBuf,
        env: &'e Env,
        initial_input: Option<&str>,
        test_render_once: bool,
        test_keys: Option<Vec<String>>,
        test_confirm: Option<String>,
    ) -> Self {
        let search_term = squeeze_ws_to_hyphen(search_term);
        let buffer = initial_input.map_or(search_term.clone(), squeeze_ws_to_hyphen);
        let input: Vec<char> = buffer.chars().collect();
        let input_cursor = input.len();
        let test_had_keys = test_keys.as_ref().is_some_and(|k| !k.is_empty());
        let test_no_cls = test_render_once || test_had_keys;
        if !base_path.exists() {
            let _ = std::fs::create_dir_all(&base_path);
        }
        Self {
            env,
            base_path,
            input,
            input_cursor,
            cursor_pos: 0,
            scroll_offset: 0,
            selected: None,
            all_tries: None,
            fuzzy_entries: None,
            last_query: None,
            cached: None,
            delete_status: None,
            delete_mode: false,
            marked: Vec::new(),
            test_render_once,
            test_no_cls,
            keys: Keys {
                test_keys: test_keys.unwrap_or_default().into(),
                test_had_keys,
            },
            test_confirm,
            needs_redraw: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            winch_id: None,
            saved_termios: None,
            alt_screen_active: false,
        }
    }

    /// `run` (`try.rb:43-70`): setup, tty checks, main loop, restore.
    /// Restoration happens in `Drop` (mirroring Ruby's `ensure`), so a panic
    /// mid-frame still restores termios and leaves the alt screen — this is
    /// why the workspace keeps `panic = "unwind"`.
    #[must_use]
    pub fn run(mut self) -> Option<Selection> {
        self.setup_terminal();
        self.run_inner()
    }

    fn run_inner(&mut self) -> Option<Selection> {
        if self.test_render_once && self.keys.test_keys.is_empty() && !self.keys.test_had_keys {
            let tries = self.get_tries();
            self.render(&tries);
            return None;
        }

        let stdin_tty = rustix::termios::isatty(std::io::stdin());
        let stderr_tty = rustix::termios::isatty(std::io::stderr());
        if !stdin_tty || !stderr_tty {
            if self.keys.test_keys.is_empty() && !self.keys.test_had_keys {
                let _ = writeln!(
                    std::io::stderr(),
                    "Error: try requires an interactive terminal"
                );
                return None;
            }
            self.main_loop();
        } else {
            self.enter_raw();
            self.main_loop();
            self.leave_raw();
        }
        self.selected.take()
    }

    fn setup_terminal(&mut self) {
        if !self.test_no_cls {
            self.alt_screen_active = true;
            let _ = write!(
                std::io::stderr(),
                "{}{}{}",
                ansi::ALT_SCREEN_ON,
                ansi::set_title("try"),
                ansi::CURSOR_BLINK
            );
        }
        self.winch_id = signal_hook::flag::register(
            signal_hook::consts::SIGWINCH,
            std::sync::Arc::clone(&self.needs_redraw),
        )
        .ok();
    }

    fn restore_terminal(&mut self) {
        if self.alt_screen_active {
            self.alt_screen_active = false;
            let mut err = std::io::stderr();
            let _ = write!(
                err,
                "{}{}{}",
                ansi::RESET,
                ansi::CURSOR_DEFAULT,
                ansi::ALT_SCREEN_OFF
            );
            let _ = err.flush();
        }
        if let Some(id) = self.winch_id.take() {
            signal_hook::low_level::unregister(id);
        }
    }

    fn enter_raw(&mut self) {
        let err = std::io::stderr();
        if let Ok(t) = rustix::termios::tcgetattr(&err) {
            let mut raw = t.clone();
            raw.make_raw();
            if rustix::termios::tcsetattr(&err, rustix::termios::OptionalActions::Now, &raw).is_ok()
            {
                self.saved_termios = Some(t);
            }
        }
    }

    fn leave_raw(&mut self) {
        if let Some(t) = self.saved_termios.take() {
            let _ = rustix::termios::tcsetattr(
                std::io::stderr(),
                rustix::termios::OptionalActions::Now,
                &t,
            );
        }
    }

    fn input_string(&self) -> String {
        self.input.iter().collect()
    }

    fn load_all_tries(&mut self) {
        if self.all_tries.is_none() {
            let tries = scan::load_all_tries(&self.base_path, SystemTime::now());
            self.fuzzy_entries = Some(
                tries
                    .iter()
                    .enumerate()
                    .map(|(i, t)| fuzzy::Entry::new(i, &t.basename, t.base_score))
                    .collect(),
            );
            self.all_tries = Some(tries);
        }
    }

    /// `get_tries` (`try.rb:157-174`) incl. the result-limit quirk: rows come
    /// from the CONTROLLING console, ignoring `TRY_HEIGHT`.
    fn get_tries(&mut self) -> Vec<ResultEntry> {
        self.load_all_tries();
        let query = self.input_string();
        if self.last_query.as_deref() == Some(query.as_str()) {
            if let Some(cached) = &self.cached {
                return cached
                    .iter()
                    .map(|r| ResultEntry {
                        idx: r.idx,
                        score: r.score,
                        positions: r.positions.clone(),
                    })
                    .collect();
            }
        }
        self.last_query = Some(query.clone());
        let height = terminal::console_rows();
        let max_results = height.saturating_sub(6).max(3);
        let entries = self.fuzzy_entries.as_ref().expect("loaded above");
        let results: Vec<ResultEntry> = fuzzy::match_entries(entries, &query, Some(max_results))
            .into_iter()
            .map(|m| ResultEntry {
                idx: *m.data,
                score: m.score,
                positions: m.positions,
            })
            .collect();
        self.cached = Some(
            results
                .iter()
                .map(|r| ResultEntry {
                    idx: r.idx,
                    score: r.score,
                    positions: r.positions.clone(),
                })
                .collect(),
        );
        results
    }

    fn invalidate_caches(&mut self) {
        self.all_tries = None;
        self.fuzzy_entries = None;
        self.cached = None;
        self.last_query = None;
    }

    /// `main_loop` (`try.rb:176-280`).
    #[allow(clippy::too_many_lines, reason = "1:1 port of the upstream key loop")]
    fn main_loop(&mut self) {
        loop {
            let tries = self.get_tries();
            let show_create_new = !self.input.is_empty();
            let total_items = tries.len() + usize::from(show_create_new);

            self.cursor_pos = self.cursor_pos.min(total_items.saturating_sub(1));

            self.render(&tries);

            let Some(key) = self.read_key() else {
                continue; // resize: re-render
            };

            match key.as_str() {
                "\r" => {
                    if self.delete_mode && !self.marked.is_empty() {
                        self.confirm_batch_delete(&tries);
                        if self.selected.is_some() {
                            break;
                        }
                    } else if self.cursor_pos < tries.len() {
                        let t = &self.all()[tries[self.cursor_pos].idx];
                        self.selected = Some(Selection::Cd {
                            path: t.path.clone(),
                        });
                        break;
                    } else if show_create_new {
                        self.handle_create_new();
                        if self.selected.is_some() {
                            break;
                        }
                    }
                }
                "\x1b[A" | "\x10" => self.cursor_pos = self.cursor_pos.saturating_sub(1),
                "\x1b[B" | "\x0e" => {
                    self.cursor_pos = (self.cursor_pos + 1).min(total_items.saturating_sub(1));
                }
                "\x1b[C" | "\x1b[D" => {}
                "\x7f" | "\x08" => {
                    if self.input_cursor > 0 {
                        self.input.remove(self.input_cursor - 1);
                        self.input_cursor -= 1;
                    }
                    self.cursor_pos = 0;
                }
                "\x01" => self.input_cursor = 0,
                "\x05" => self.input_cursor = self.input.len(),
                "\x02" => self.input_cursor = self.input_cursor.saturating_sub(1),
                "\x06" => self.input_cursor = (self.input_cursor + 1).min(self.input.len()),
                "\x0b" => self.input.truncate(self.input_cursor),
                "\x17" => {
                    if self.input_cursor > 0 {
                        let new_pos = word_boundary_backward(&self.input, self.input_cursor);
                        self.input.drain(new_pos..self.input_cursor);
                        self.input_cursor = new_pos;
                    }
                }
                "\x04" => {
                    if self.cursor_pos < tries.len() {
                        let path = self.all()[tries[self.cursor_pos].idx].path.clone();
                        if let Some(i) = self.marked.iter().position(|p| p == &path) {
                            self.marked.remove(i);
                        } else {
                            self.marked.push(path);
                            self.delete_mode = true;
                        }
                        if self.marked.is_empty() {
                            self.delete_mode = false;
                        }
                    }
                }
                "\x14" => {
                    self.handle_create_new();
                    if self.selected.is_some() {
                        break;
                    }
                }
                "\x12" => {
                    if self.cursor_pos < tries.len() {
                        let idx = tries[self.cursor_pos].idx;
                        self.run_rename_dialog(idx);
                        if self.selected.is_some() {
                            break;
                        }
                    }
                }
                "\x07" => {
                    if self.cursor_pos < tries.len() {
                        let idx = tries[self.cursor_pos].idx;
                        self.run_ascend_dialog(idx);
                        if self.selected.is_some() {
                            break;
                        }
                    }
                }
                "\x03" | "\x1b" => {
                    if self.delete_mode {
                        self.marked.clear();
                        self.delete_mode = false;
                    } else {
                        self.selected = None;
                        break;
                    }
                }
                other => {
                    let mut chars = other.chars();
                    if let (Some(c), None) = (chars.next(), chars.next()) {
                        if is_input_char(c) {
                            self.input.insert(self.input_cursor, c);
                            self.input_cursor += 1;
                            self.cursor_pos = 0;
                        }
                    }
                }
            }
        }
    }

    fn all(&self) -> &[TryDir] {
        self.all_tries.as_deref().unwrap_or(&[])
    }

    /// `read_key` (`try.rb:282-299`): test keys, auto-ESC on exhaustion, then
    /// poll-with-timeout for resize responsiveness. `None` = redraw.
    fn read_key(&mut self) -> Option<String> {
        if let Some(k) = self.keys.next_test_key() {
            return Some(k);
        }
        loop {
            if self
                .needs_redraw
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                if !self.test_no_cls {
                    self.clear_screen();
                }
                return None;
            }
            if poll_stdin(100) {
                return read_keypress();
            }
        }
    }

    #[allow(clippy::unused_self, reason = "mirrors the Ruby instance method")]
    fn clear_screen(&self) {
        let _ = write!(std::io::stderr(), "\x1b[2J\x1b[H");
    }

    /// `render` (`try.rb:329-388`).
    fn render(&mut self, tries: &[ResultEntry]) {
        let mut screen = Screen::new(self.env);
        let width = screen.width();
        let height = screen.height();

        {
            let line = screen.header.add_line(None);
            line.left.write_emoji("🏠");
            line.left.write(&text::accent(" Try Directory Selection"));
        }
        screen.header.add_line(None).left.write_dim_fill("─");
        {
            let value = self.input_string();
            let rendered = screen.input(&value, self.input_cursor);
            let line = screen.header.add_line(None);
            let prefix = "Search: ";
            line.left.write_dim(prefix);
            line.left.write(&rendered);
            line.mark_has_input(metrics::visible_width(prefix));
        }
        screen.header.add_line(None).left.write_dim_fill("─");

        screen.footer.add_line(None).left.write_dim_fill("─");
        if let Some(status) = self.delete_status.take() {
            screen.footer.add_line(None).left.write_bold(&status);
        } else if self.delete_mode {
            let line = screen.footer.add_line(Some(palette::DANGER_BG));
            line.left.write_bold(" DELETE MODE ");
            line.left.write(&format!(
                " {} marked  |  Ctrl-D: Toggle  Enter: Confirm  Esc: Cancel",
                self.marked.len()
            ));
        } else {
            screen.footer.add_line(None).center.write_dim(
                "↑/↓: Navigate  Enter: Select  ^R: Rename  ^G: Graduate  ^D: Delete  Esc: Cancel",
            );
        }

        let header_lines = screen.header.lines.len();
        let footer_lines = screen.footer.lines.len();
        let max_visible =
            (height as i64 - header_lines as i64 - footer_lines as i64).max(3) as usize;
        let show_create_new = !self.input.is_empty();
        let total_items = tries.len() + usize::from(show_create_new);

        if self.cursor_pos < self.scroll_offset {
            self.scroll_offset = self.cursor_pos;
        } else if self.cursor_pos >= self.scroll_offset + max_visible {
            self.scroll_offset = self.cursor_pos - max_visible + 1;
        }

        let visible_end = (self.scroll_offset + max_visible).min(total_items);
        let now = SystemTime::now();

        for idx in self.scroll_offset..visible_end {
            if idx == tries.len() && !tries.is_empty() && idx >= self.scroll_offset {
                screen.body.add_line(None);
            }
            if idx < tries.len() {
                self.render_entry_line(
                    &mut screen,
                    &tries[idx],
                    idx == self.cursor_pos,
                    width,
                    now,
                );
            } else {
                self.render_create_line(&mut screen, idx == self.cursor_pos);
            }
        }

        screen.flush(&mut std::io::stderr());
    }

    /// `render_entry_line` (`try.rb:390-426`).
    fn render_entry_line(
        &self,
        screen: &mut Screen,
        entry: &ResultEntry,
        is_selected: bool,
        width: usize,
        now: SystemTime,
    ) {
        let t = &self.all()[entry.idx];
        let is_marked = self.marked.contains(&t.path);
        let background = if is_marked {
            Some(palette::DANGER_BG)
        } else if is_selected {
            Some(palette::SELECTED_BG)
        } else {
            None
        };

        let (plain_name, rendered_name) = formatted_entry_name(&t.basename, &entry.positions);
        let prefix_width = 5usize;
        let meta_text = format!("{}, {:.1}", format_relative_time(now, t.mtime), entry.score);

        let max_name_width = width as i64 - prefix_width as i64 - 1;
        let display_rendered =
            if max_name_width > 2 && plain_name.chars().count() as i64 > max_name_width {
                #[allow(clippy::cast_sign_loss, reason = "checked > 2 above")]
                let w = (max_name_width - 1) as usize;
                format!("{}…", truncate_with_ansi(&rendered_name, w))
            } else {
                rendered_name
            };

        let line = screen.body.add_line(background);
        if is_selected {
            line.left.write(&text::highlight("→ "));
        } else {
            line.left.write("  ");
        }
        let icon = if is_marked {
            "🗑️"
        } else if t.is_symlink {
            "🔗"
        } else {
            "📁"
        };
        line.left.write_emoji(icon);
        line.left.write(" ");
        line.left.write(&display_rendered);
        line.right.write_dim(&meta_text);
    }

    /// `render_create_line` (`try.rb:428-439`).
    fn render_create_line(&self, screen: &mut Screen, is_selected: bool) {
        let background = if is_selected {
            Some(palette::SELECTED_BG)
        } else {
            None
        };
        let line = screen.body.add_line(background);
        if is_selected {
            line.left.write(&text::highlight("→ "));
        } else {
            line.left.write("  ");
        }
        let date_prefix = today_string();
        let label = if self.input.is_empty() {
            format!("📂 Create new: {date_prefix}-")
        } else {
            format!("📂 Create new: {date_prefix}-{}", self.input_string())
        };
        line.left.write(&label);
    }

    /// `handle_create_new` (`try.rb:785-820`).
    fn handle_create_new(&mut self) {
        let date_prefix = today_string();
        if !self.input.is_empty() {
            let final_name =
                squeeze_ws_to_hyphen(&format!("{date_prefix}-{}", self.input_string()));
            self.selected = Some(Selection::Mkdir {
                path: self.base_path.join(final_name),
            });
            return;
        }
        // Prompt for a name in cooked mode
        if !self.test_no_cls {
            self.clear_screen();
        }
        let mut err = std::io::stderr();
        let _ = write!(err, "{}", ansi::SHOW);
        let _ = writeln!(err, "Enter new try name");
        let _ = writeln!(err);
        let _ = write!(err, "> {date_prefix}-");
        let _ = err.flush();

        let entry = self.read_line_cooked();

        if !self.test_no_cls {
            let _ = write!(std::io::stderr(), "{}", ansi::HIDE);
        }
        let entry = entry.trim_end_matches(['\r', '\n']).to_string();
        if entry.is_empty() {
            return;
        }
        let final_name = squeeze_ws_to_hyphen(&format!("{date_prefix}-{entry}"));
        self.selected = Some(Selection::Mkdir {
            path: self.base_path.join(final_name),
        });
    }

    fn read_line_cooked(&mut self) -> String {
        // STDERR.cooked { STDIN.iflush; STDIN.gets } (try.rb:805-808)
        let raw = self.saved_termios.clone();
        if let Some(orig) = &raw {
            let _ = rustix::termios::tcsetattr(
                std::io::stderr(),
                rustix::termios::OptionalActions::Now,
                orig,
            );
            let _ =
                rustix::termios::tcflush(std::io::stdin(), rustix::termios::QueueSelector::IFlush);
        }
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        if raw.is_some() {
            self.enter_raw();
        }
        line
    }

    /// `run_rename_dialog` (`try.rb:534-591`).
    fn run_rename_dialog(&mut self, idx: usize) {
        self.delete_mode = false;
        self.marked.clear();

        let current_name = self.all()[idx].basename.clone();
        let mut buffer: Vec<char> = current_name.chars().collect();
        let mut cursor = buffer.len();
        let mut error: Option<String> = None;

        loop {
            self.render_rename_dialog(&current_name, &buffer, cursor, error.as_deref());
            let Some(ch) = self.read_key() else { continue };
            match ch.as_str() {
                "\r" => match self.finalize_rename(&current_name, &buffer) {
                    Ok(()) => break,
                    Err(e) => error = Some(e),
                },
                "\x1b" | "\x03" => break,
                "\x7f" | "\x08" => {
                    if cursor > 0 {
                        buffer.remove(cursor - 1);
                        cursor -= 1;
                    }
                    error = None;
                }
                "\x01" => cursor = 0,
                "\x05" => cursor = buffer.len(),
                "\x02" => cursor = cursor.saturating_sub(1),
                "\x06" => cursor = (cursor + 1).min(buffer.len()),
                "\x0b" => {
                    buffer.truncate(cursor);
                    error = None;
                }
                "\x17" => {
                    if cursor > 0 {
                        let new_pos = word_boundary_backward(&buffer, cursor);
                        buffer.drain(new_pos..cursor);
                        cursor = new_pos;
                    }
                    error = None;
                }
                other => {
                    let mut cs = other.chars();
                    if let (Some(c), None) = (cs.next(), cs.next()) {
                        if is_rename_char(c) {
                            buffer.insert(cursor, c);
                            cursor += 1;
                            error = None;
                        }
                    }
                }
            }
        }
        self.needs_redraw
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// `render_rename_dialog` (`try.rb:593-629`).
    fn render_rename_dialog(
        &self,
        current_name: &str,
        buffer: &[char],
        cursor: usize,
        error: Option<&str>,
    ) {
        let mut screen = Screen::new(self.env);
        {
            let line = screen.header.add_line(None);
            line.center.write_emoji("✏️");
            line.center.write(&text::accent("  Rename directory"));
        }
        screen.header.add_line(None).left.write_dim_fill("─");
        {
            let line = screen.body.add_line(None);
            line.left.write_emoji("📁");
            line.left.write(&format!(" {current_name}"));
        }
        screen.body.add_line(None);
        screen.body.add_line(None);
        {
            let value: String = buffer.iter().collect();
            let rendered = screen.input(&value, cursor);
            let width = screen.width();
            let line = screen.body.add_line(None);
            let prefix = "New name: ";
            line.center.write_dim(prefix);
            line.center.write(&rendered);
            let input_width = buffer.len().max(cursor + 1);
            let prefix_width = metrics::visible_width(prefix);
            let max_content = width as i64 - 1;
            let center_start = (max_content - prefix_width as i64 - input_width as i64) / 2;
            #[allow(clippy::cast_sign_loss, reason = "clamped at 0")]
            line.mark_has_input(center_start.max(0) as usize + prefix_width);
        }
        if let Some(e) = error {
            screen.body.add_line(None);
            screen.body.add_line(None).center.write_bold(e);
        }
        screen.footer.add_line(None).left.write_dim_fill("─");
        screen
            .footer
            .add_line(None)
            .center
            .write_dim("Enter: Confirm  Esc: Cancel");
        screen.flush(&mut std::io::stderr());
    }

    /// `finalize_rename` (`try.rb:631-642`).
    fn finalize_rename(&mut self, old_name: &str, buffer: &[char]) -> Result<(), String> {
        let raw: String = buffer.iter().collect();
        let new_name = squeeze_ws_to_hyphen(raw.trim());
        if new_name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }
        if new_name.contains('/') {
            return Err("Name cannot contain /".to_string());
        }
        if new_name == old_name {
            return Ok(()); // no change, just exit
        }
        if self.base_path.join(&new_name).is_dir() {
            return Err(format!("Directory exists: {new_name}"));
        }
        self.selected = Some(Selection::Rename {
            base_path: self.base_path.clone(),
            old: old_name.to_string(),
            new: new_name,
        });
        Ok(())
    }

    /// `run_ascend_dialog` (`try.rb:645-713`).
    fn run_ascend_dialog(&mut self, idx: usize) {
        self.delete_mode = false;
        self.marked.clear();

        let current_name = self.all()[idx].basename.clone();
        let source = self.all()[idx].path.clone();

        let project_name = if scan::has_date_prefix(&current_name) {
            current_name[11..].to_string()
        } else {
            current_name.clone()
        };

        let projects_dir = self.env.try_projects.as_ref().map_or_else(
            || {
                self.base_path
                    .parent()
                    .map_or_else(|| self.base_path.clone(), std::path::Path::to_path_buf)
            },
            |p| {
                expand_path(
                    p,
                    &std::env::current_dir().unwrap_or_default(),
                    self.env.home.as_deref(),
                )
            },
        );

        let mut buffer: Vec<char> = projects_dir
            .join(&project_name)
            .to_string_lossy()
            .chars()
            .collect();
        let mut cursor = buffer.len();
        let mut error: Option<String> = None;

        loop {
            self.render_ascend_dialog(
                &current_name,
                &buffer,
                cursor,
                error.as_deref(),
                &projects_dir,
            );
            let Some(ch) = self.read_key() else { continue };
            match ch.as_str() {
                "\r" => match self.finalize_ascend(&source, &current_name, &buffer) {
                    Ok(()) => break,
                    Err(e) => error = Some(e),
                },
                "\x1b" | "\x03" => break,
                "\x7f" | "\x08" => {
                    if cursor > 0 {
                        buffer.remove(cursor - 1);
                        cursor -= 1;
                    }
                    error = None;
                }
                "\x01" => cursor = 0,
                "\x05" => cursor = buffer.len(),
                "\x02" => cursor = cursor.saturating_sub(1),
                "\x06" => cursor = (cursor + 1).min(buffer.len()),
                "\x0b" => {
                    buffer.truncate(cursor);
                    error = None;
                }
                "\x17" => {
                    if cursor > 0 {
                        let new_pos = word_boundary_backward(&buffer, cursor);
                        buffer.drain(new_pos..cursor);
                        cursor = new_pos;
                    }
                    error = None;
                }
                other => {
                    let mut cs = other.chars();
                    if let (Some(c), None) = (cs.next(), cs.next()) {
                        if is_ascend_char(c) {
                            buffer.insert(cursor, c);
                            cursor += 1;
                            error = None;
                        }
                    }
                }
            }
        }
        self.needs_redraw
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// `render_ascend_dialog` (`try.rb:715-758`).
    fn render_ascend_dialog(
        &self,
        current_name: &str,
        buffer: &[char],
        cursor: usize,
        error: Option<&str>,
        projects_dir: &std::path::Path,
    ) {
        let mut screen = Screen::new(self.env);
        {
            let line = screen.header.add_line(None);
            line.center.write_emoji("🚀");
            line.center
                .write(&text::accent("  Graduate try to project"));
        }
        screen.header.add_line(None).left.write_dim_fill("─");
        {
            let line = screen.body.add_line(None);
            line.left.write_emoji("📁");
            line.left.write(&format!(" {current_name}"));
        }
        screen.body.add_line(None);
        let env_hint = if self.env.try_projects.is_some() {
            "$TRY_PROJECTS"
        } else {
            "parent of $TRY_PATH"
        };
        screen.body.add_line(None).center.write_dim(&format!(
            "Destination ({env_hint}: {})",
            projects_dir.display()
        ));
        {
            let value: String = buffer.iter().collect();
            let rendered = screen.input(&value, cursor);
            let width = screen.width();
            let line = screen.body.add_line(None);
            let prefix = "Move to: ";
            line.center.write_dim(prefix);
            line.center.write(&rendered);
            let input_width = buffer.len().max(cursor + 1);
            let prefix_width = metrics::visible_width(prefix);
            let max_content = width as i64 - 1;
            let center_start = (max_content - prefix_width as i64 - input_width as i64) / 2;
            #[allow(clippy::cast_sign_loss, reason = "clamped at 0")]
            line.mark_has_input(center_start.max(0) as usize + prefix_width);
        }
        screen.body.add_line(None);
        screen
            .body
            .add_line(None)
            .center
            .write_dim("A symlink will be left in the tries directory");
        if let Some(e) = error {
            screen.body.add_line(None);
            screen.body.add_line(None).center.write_bold(e);
        }
        screen.footer.add_line(None).left.write_dim_fill("─");
        screen
            .footer
            .add_line(None)
            .center
            .write_dim("Enter: Confirm  Esc: Cancel");
        screen.flush(&mut std::io::stderr());
    }

    /// `finalize_ascend` (`try.rb:760-778`).
    fn finalize_ascend(
        &mut self,
        source: &std::path::Path,
        basename: &str,
        buffer: &[char],
    ) -> Result<(), String> {
        let raw: String = buffer.iter().collect();
        let trimmed = raw.trim();
        // Ruby expands BEFORE the empty check (try.rb:761-764), so
        // File.expand_path("") == cwd makes the empty branch dead code —
        // an emptied buffer reports "Destination already exists: <cwd>".
        // Port the reachable behavior, keep the (dead) guard for shape.
        let dest = expand_path(
            trimmed,
            &std::env::current_dir().unwrap_or_default(),
            self.env.home.as_deref(),
        );
        if dest.as_os_str().is_empty() {
            return Err("Destination cannot be empty".to_string());
        }
        if dest.exists() {
            return Err(format!("Destination already exists: {}", dest.display()));
        }
        let parent = dest
            .parent()
            .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
        if !parent.is_dir() {
            return Err(format!(
                "Parent directory does not exist: {}",
                parent.display()
            ));
        }
        self.selected = Some(Selection::Ascend {
            source: source.to_path_buf(),
            dest,
            basename: basename.to_string(),
            base_path: self.base_path.clone(),
        });
        Ok(())
    }

    /// `confirm_batch_delete` (`try.rb:822-881`).
    fn confirm_batch_delete(&mut self, tries: &[ResultEntry]) {
        let marked_items: Vec<(PathBuf, String)> = tries
            .iter()
            .map(|r| &self.all()[r.idx])
            .filter(|t| self.marked.contains(&t.path))
            .map(|t| (t.path.clone(), t.basename.clone()))
            .collect();
        if marked_items.is_empty() {
            return;
        }

        // Test mode: keys feed the confirmation buffer raw until Enter
        if !self.keys.test_keys.is_empty() {
            let mut confirmation = String::new();
            while let Some(ch) = self.keys.test_keys.pop_front() {
                if ch == "\r" || ch == "\n" {
                    break;
                }
                confirmation.push_str(&ch);
            }
            self.process_delete_confirmation(&marked_items, &confirmation);
            return;
        }
        if self.test_confirm.is_some() || !rustix::termios::isatty(std::io::stderr()) {
            let confirmation = self.test_confirm.clone().unwrap_or_else(|| {
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                line
            });
            let confirmation = confirmation.trim_end_matches(['\r', '\n']).to_string();
            self.process_delete_confirmation(&marked_items, &confirmation);
            return;
        }

        // Interactive dialog
        if !self.test_no_cls {
            self.clear_screen();
        }
        let mut buffer: Vec<char> = Vec::new();
        let mut cursor = 0usize;
        loop {
            self.render_delete_dialog(&marked_items, &buffer, cursor);
            let Some(ch) = self.read_key() else { continue };
            match ch.as_str() {
                "\r" => {
                    let confirmation: String = buffer.iter().collect();
                    self.process_delete_confirmation(&marked_items, &confirmation);
                    break;
                }
                "\x1b" | "\x03" => {
                    self.delete_status = Some("Delete cancelled".to_string());
                    self.marked.clear();
                    self.delete_mode = false;
                    break;
                }
                "\x7f" | "\x08" => {
                    if cursor > 0 {
                        buffer.remove(cursor - 1);
                        cursor -= 1;
                    }
                }
                other => {
                    let mut cs = other.chars();
                    if let (Some(c), None) = (cs.next(), cs.next()) {
                        if c as u32 >= 32 {
                            buffer.insert(cursor, c);
                            cursor += 1;
                        }
                    }
                }
            }
        }
        self.needs_redraw
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// `render_delete_dialog` (`try.rb:883-917`).
    fn render_delete_dialog(&self, marked: &[(PathBuf, String)], buffer: &[char], cursor: usize) {
        let mut screen = Screen::new(self.env);
        let count = marked.len();
        {
            let line = screen.header.add_line(None);
            line.center.write_emoji("🗑️");
            line.center.write(&text::accent(&format!(
                "  Delete {count} {}?",
                if count == 1 {
                    "directory"
                } else {
                    "directories"
                }
            )));
        }
        screen.header.add_line(None).left.write_dim_fill("─");
        for (_, basename) in marked {
            let line = screen.body.add_line(Some(palette::DANGER_BG));
            line.left.write_emoji("🗑️");
            line.left.write(&format!(" {basename}"));
        }
        screen.body.add_line(None);
        screen.body.add_line(None);
        {
            let value: String = buffer.iter().collect();
            let rendered = screen.input(&value, cursor);
            let width = screen.width();
            let line = screen.body.add_line(None);
            let prefix = "Type YES to confirm: ";
            line.center.write_dim(prefix);
            line.center.write(&rendered);
            let input_width = buffer.len().max(cursor + 1);
            let prefix_width = metrics::visible_width(prefix);
            let max_content = width as i64 - 1;
            let center_start = (max_content - prefix_width as i64 - input_width as i64) / 2;
            #[allow(clippy::cast_sign_loss, reason = "clamped at 0")]
            line.mark_has_input(center_start.max(0) as usize + prefix_width);
        }
        screen.footer.add_line(None).left.write_dim_fill("─");
        screen
            .footer
            .add_line(None)
            .center
            .write_dim("Enter: Confirm  Esc: Cancel");
        screen.flush(&mut std::io::stderr());
    }

    /// `process_delete_confirmation` (`try.rb:919-952`): literal `YES`,
    /// realpath containment safety, cache invalidation.
    fn process_delete_confirmation(&mut self, marked: &[(PathBuf, String)], confirmation: &str) {
        if confirmation == "YES" {
            match self.validate_deletions(marked) {
                Ok((base_real, validated)) => {
                    let names: Vec<String> = validated.iter().map(|(_, b)| b.clone()).collect();
                    self.delete_status = Some(format!("Deleted: {}", names.join(", ")));
                    self.selected = Some(Selection::Delete {
                        basenames: names,
                        base_path: base_real,
                    });
                    self.invalidate_caches();
                    self.marked.clear();
                    self.delete_mode = false;
                }
                Err(e) => {
                    self.delete_status = Some(format!("Error: {e}"));
                }
            }
        } else {
            self.delete_status = Some("Delete cancelled".to_string());
            self.marked.clear();
            self.delete_mode = false;
        }
    }

    fn validate_deletions(
        &self,
        marked: &[(PathBuf, String)],
    ) -> Result<(PathBuf, Vec<(PathBuf, String)>), String> {
        let base_real = self.base_path.canonicalize().map_err(|e| e.to_string())?;
        let mut validated = Vec::new();
        for (path, basename) in marked {
            let target_real = path.canonicalize().map_err(|e| e.to_string())?;
            let base_prefix = format!("{}/", base_real.display());
            if !target_real.display().to_string().starts_with(&base_prefix) {
                return Err(format!(
                    "Safety check failed: {} is not inside {}",
                    target_real.display(),
                    base_real.display()
                ));
            }
            validated.push((target_real, basename.clone()));
        }
        Ok((base_real, validated))
    }
}

/// `formatted_entry_name` (`try.rb:441-458`): dimmed date prefix, the
/// position-10 hyphen highlight, highlighted name part.
fn formatted_entry_name(basename: &str, positions: &[usize]) -> (String, String) {
    if scan::has_date_prefix(basename) && basename.chars().count() > 11 {
        let date_part = &basename[..10];
        let name_part = &basename[11..];
        let date_len = 11; // date + hyphen
        let mut rendered = text::dim(date_part);
        if positions.contains(&10) {
            rendered.push_str(&text::highlight("-"));
        } else {
            rendered.push_str(&text::dim("-"));
        }
        rendered.push_str(&highlight_with_positions(name_part, positions, date_len));
        (basename.to_string(), rendered)
    } else {
        (
            basename.to_string(),
            highlight_with_positions(basename, positions, 0),
        )
    }
}

/// Local date `YYYY-MM-DD` (`Time.now.strftime`).
fn today_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Poll stdin for readability with a millisecond timeout (Ruby
/// `IO.select([STDIN], nil, nil, 0.1)`).
#[cfg(unix)]
fn poll_stdin(timeout_ms: i32) -> bool {
    use rustix::event::{PollFd, PollFlags};
    let stdin = std::io::stdin();
    let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
    matches!(
      rustix::event::poll(
        &mut fds,
        Some(&rustix::event::Timespec {
          tv_sec: 0,
          tv_nsec: i64::from(timeout_ms) * 1_000_000,
        })
      ),
      Ok(n) if n > 0
    )
}

#[cfg(not(unix))]
fn poll_stdin(_timeout_ms: i32) -> bool {
    true
}

/// `read_keypress` (`try.rb:301-315`): one byte, then up to 3 + up to 2
/// nonblocking continuation bytes for escape sequences.
///
/// Reads fd 0 directly (`rustix::io::read`) — `std::io::stdin()` is
/// internally buffered, which would swallow escape-sequence continuation
/// bytes into userspace where `poll(2)` can't see them (DOWN would read as
/// a bare ESC and cancel the selector).
#[cfg(unix)]
fn read_keypress() -> Option<String> {
    let stdin = std::io::stdin();
    let mut b = [0u8; 1];
    let n = rustix::io::read(&stdin, &mut b).ok()?;
    if n == 0 {
        return None;
    }
    let mut bytes = vec![b[0]];
    if b[0] == 0x1b {
        for cap in [3usize, 2] {
            let mut extra = vec![0u8; cap];
            if poll_stdin(0) {
                if let Ok(n) = rustix::io::read(&stdin, &mut extra[..]) {
                    bytes.extend_from_slice(&extra[..n]);
                }
            }
        }
    } else {
        // Ruby STDIN.getc decodes a full UTF-8 char; continuation bytes of a
        // multibyte char arrive together, so drain them (poll-guarded)
        let want = match b[0] {
            0xC0..=0xDF => 1usize,
            0xE0..=0xEF => 2,
            0xF0..=0xF7 => 3,
            _ => 0,
        };
        let mut got = 0;
        while got < want && poll_stdin(0) {
            let mut c = [0u8; 1];
            match rustix::io::read(&stdin, &mut c) {
                Ok(1) => {
                    bytes.push(c[0]);
                    got += 1;
                }
                _ => break,
            }
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(not(unix))]
fn read_keypress() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_boundary_backward_skips_symbols_then_words() {
        let buf: Vec<char> = "foo-bar".chars().collect();
        assert_eq!(word_boundary_backward(&buf, 7), 4); // deletes "bar"
        assert_eq!(word_boundary_backward(&buf, 4), 0); // deletes "foo-"
    }

    #[test]
    fn relative_time_boundaries() {
        use std::time::Duration;
        let now = SystemTime::now();
        let t = |s: u64| now.checked_sub(Duration::from_secs(s)).unwrap();
        assert_eq!(format_relative_time(now, t(30)), "just now");
        assert_eq!(format_relative_time(now, t(90)), "1m ago");
        assert_eq!(format_relative_time(now, t(3 * 3600)), "3h ago");
        assert_eq!(format_relative_time(now, t(2 * 86400)), "2d ago");
        assert_eq!(format_relative_time(now, t(15 * 86400)), "2w ago");
    }

    #[test]
    fn formatted_name_dims_date_and_highlights_hyphen_at_10() {
        let _guard = crate::tui::TEST_COLOR_LOCK.lock().unwrap();
        crate::tui::set_colors_enabled(true);
        let (plain, rendered) = formatted_entry_name("2026-07-10-alpha", &[10, 11]);
        assert_eq!(plain, "2026-07-10-alpha");
        // hyphen at index 10 highlighted, 'a' at 11 highlighted
        assert!(rendered.contains("\x1b[1;33m-\x1b[39m\x1b[22m"));
        assert!(rendered.contains("\x1b[1;33ma\x1b[39m\x1b[22m"));
    }

    #[test]
    fn truncate_with_ansi_counts_visible_only() {
        let s = format!("{}abcdef", "\x1b[2m");
        assert_eq!(truncate_with_ansi(&s, 3), format!("{}abc", "\x1b[2m"));
    }
}
