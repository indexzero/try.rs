//! TUI toolkit — structural port of upstream `lib/tui.rb`.
//!
//! Renders to **stderr** (stdout is the script channel). Frames are built in
//! one buffer and written with a single `write` to avoid flicker
//! (`tui.rb:389,465`). Byte output is pinned by the conformance suite's ANSI
//! and layout tests plus committed golden frames.

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "1:1 port of Ruby integer/float arithmetic; widths, cursor positions, and \
            list lengths are bounded by terminal size and directory counts"
)]

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global color gate, mirroring Ruby's module-level `@colors_enabled`
/// (`tui.rb:25`): initialized from `NO_COLORS` (plural), then optionally
/// disabled by `--no-colors` / `NO_COLOR` in the dispatcher.
static COLORS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Is styling active?
pub fn colors_enabled() -> bool {
    COLORS_ENABLED.load(Ordering::Relaxed)
}

/// Set the color gate (dispatcher startup only).
pub fn set_colors_enabled(on: bool) {
    COLORS_ENABLED.store(on, Ordering::Relaxed);
}

/// Serializes tests that mutate the global color gate (parallel test
/// harness + global state = flakes otherwise).
#[cfg(test)]
pub(crate) static TEST_COLOR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// ANSI escape constants and helpers (`tui.rb:47-88`).
pub mod ansi {
    /// Clear to end of line.
    pub const CLEAR_EOL: &str = "\x1b[K";
    /// Clear screen.
    pub const CLEAR_SCREEN: &str = "\x1b[2J";
    /// Cursor home.
    pub const HOME: &str = "\x1b[H";
    /// Hide cursor.
    pub const HIDE: &str = "\x1b[?25l";
    /// Show cursor.
    pub const SHOW: &str = "\x1b[?25h";
    /// Blinking block cursor.
    pub const CURSOR_BLINK: &str = "\x1b[1 q";
    /// Reset cursor to terminal default.
    pub const CURSOR_DEFAULT: &str = "\x1b[0 q";
    /// Enter alternate screen buffer.
    pub const ALT_SCREEN_ON: &str = "\x1b[?1049h";
    /// Return to main screen buffer.
    pub const ALT_SCREEN_OFF: &str = "\x1b[?1049l";
    /// Full SGR reset.
    pub const RESET: &str = "\x1b[0m";
    /// Reset foreground only.
    pub const RESET_FG: &str = "\x1b[39m";
    /// Reset intensity (bold/dim off).
    pub const RESET_INTENSITY: &str = "\x1b[22m";
    /// Bold on.
    pub const BOLD: &str = "\x1b[1m";

    /// Set window title (`tui.rb:85-87`).
    #[must_use]
    pub fn set_title(t: &str) -> String {
        format!("\x1b]2;{t}\x07")
    }
}

/// Color palette (`tui.rb:90-102`), values expanded from the Ruby
/// `ANSI.sgr`/`fg`/`bg` helpers.
pub mod palette {
    /// Accent: bold 256-color 214.
    pub const ACCENT: &str = "\x1b[1;38;5;214m";
    /// Highlight: bold yellow.
    pub const HIGHLIGHT: &str = "\x1b[1;33m";
    /// Muted/dim foreground: 256-color 245.
    pub const MUTED: &str = "\x1b[38;5;245m";
    /// Input cursor on: reverse video.
    pub const INPUT_CURSOR_ON: &str = "\x1b[7m";
    /// Input cursor off.
    pub const INPUT_CURSOR_OFF: &str = "\x1b[27m";
    /// Selected-row background: 256-color 238.
    pub const SELECTED_BG: &str = "\x1b[48;5;238m";
    /// Danger background: 256-color 52.
    pub const DANGER_BG: &str = "\x1b[48;5;52m";
}

/// Width metrics and ANSI-aware truncation (`tui.rb:104-253`).
pub mod metrics {
    /// Upstream's deliberately naive width table (`tui.rb:133-142`): variation
    /// selectors 0, the emoji block 2, **everything else 1** (arrows, box
    /// drawing, CJK included). Do not "fix" with unicode-width — parity.
    #[must_use]
    pub fn char_width(code: u32) -> usize {
        if (0xFE00..=0xFE0F).contains(&code) {
            0
        } else if (0x1F300..=0x1FAFF).contains(&code) {
            2
        } else {
            1
        }
    }

    /// Strip `\e[…X` sequences (Ruby `ANSI_STRIP_RE = /\e\[[0-9;]*[A-Za-z]/`).
    fn strip_ansi(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for e in chars.by_ref() {
                    if e.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Visible width: ANSI stripped, then `char_width` per codepoint
    /// (`tui.rb:109-130`; the Ruby fast paths are equivalent for ASCII).
    #[must_use]
    pub fn visible_width(text: &str) -> usize {
        let stripped;
        let s = if text.contains('\x1b') {
            stripped = strip_ansi(text);
            &stripped
        } else {
            text
        };
        s.chars().map(|c| char_width(c as u32)).sum()
    }

    /// Tail truncation preserving ANSI sequences, appending `overflow`
    /// (`tui.rb:156-192`). Note upstream `rstrip`s before appending.
    #[must_use]
    pub fn truncate(text: &str, max_width: usize, overflow: &str) -> String {
        if visible_width(text) <= max_width {
            return text.to_string();
        }
        let overflow_width = visible_width(overflow);
        let target = max_width.saturating_sub(overflow_width);
        let mut truncated = String::new();
        let mut width = 0;
        let mut in_escape = false;
        let mut escape_buf = String::new();

        for ch in text.chars() {
            if in_escape {
                escape_buf.push(ch);
                if ch.is_ascii_alphabetic() {
                    truncated.push_str(&escape_buf);
                    escape_buf.clear();
                    in_escape = false;
                }
                continue;
            }
            if ch == '\x1b' {
                in_escape = true;
                escape_buf.clear();
                escape_buf.push(ch);
                continue;
            }
            let cw = char_width(ch as u32);
            if width + cw > target {
                break;
            }
            truncated.push(ch);
            width += cw;
        }
        // Ruby String#rstrip strips ASCII whitespace + NUL only
        let stripped = truncated.trim_end_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r', '\0']);
        format!("{stripped}{overflow}")
    }

    /// Head truncation keeping the trailing portion, preserving LEADING escape
    /// sequences (`tui.rb:196-252`) — used for right-aligned overflow.
    #[must_use]
    pub fn truncate_from_start(text: &str, max_width: usize) -> String {
        let vis_width = visible_width(text);
        if vis_width <= max_width {
            return text.to_string();
        }

        // Collect leading escapes
        let mut leading_escapes = String::new();
        let mut in_escape = false;
        let mut escape_buf = String::new();
        for ch in text.chars() {
            if in_escape {
                escape_buf.push(ch);
                if ch.is_ascii_alphabetic() {
                    leading_escapes.push_str(&escape_buf);
                    escape_buf.clear();
                    in_escape = false;
                }
            } else if ch == '\x1b' {
                in_escape = true;
                escape_buf.clear();
                escape_buf.push(ch);
            } else {
                break;
            }
        }

        // Skip visible chars until max_width remain
        let chars_to_skip = vis_width - max_width;
        let mut skipped = 0;
        let mut result = String::new();
        let mut in_escape = false;
        for ch in text.chars() {
            if in_escape {
                if skipped >= chars_to_skip {
                    result.push(ch);
                }
                if ch.is_ascii_alphabetic() {
                    in_escape = false;
                }
                continue;
            }
            if ch == '\x1b' {
                in_escape = true;
                if skipped >= chars_to_skip {
                    result.push(ch);
                }
                continue;
            }
            let cw = char_width(ch as u32);
            if skipped < chars_to_skip {
                skipped += cw;
            } else {
                result.push(ch);
            }
        }
        format!("{leading_escapes}{result}")
    }
}

/// Styled text helpers (`tui.rb:255-279`). All return the input unchanged
/// when colors are disabled.
pub mod text {
    use super::{ansi, colors_enabled, palette};

    fn wrap(text: &str, prefix: &str, suffix: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        if !colors_enabled() {
            return text.to_string();
        }
        format!("{prefix}{text}{suffix}")
    }

    /// Bold.
    #[must_use]
    pub fn bold(t: &str) -> String {
        wrap(t, ansi::BOLD, ansi::RESET_INTENSITY)
    }

    /// Dim (muted fg).
    #[must_use]
    pub fn dim(t: &str) -> String {
        wrap(t, palette::MUTED, ansi::RESET_FG)
    }

    /// Bold-yellow highlight.
    #[must_use]
    pub fn highlight(t: &str) -> String {
        wrap(
            t,
            palette::HIGHLIGHT,
            &format!("{}{}", ansi::RESET_FG, ansi::RESET_INTENSITY),
        )
    }

    /// Accent (bold orange).
    #[must_use]
    pub fn accent(t: &str) -> String {
        wrap(
            t,
            palette::ACCENT,
            &format!("{}{}", ansi::RESET_FG, ansi::RESET_INTENSITY),
        )
    }
}

/// Terminal size resolution (`tui.rb:308-348`): `TRY_HEIGHT`/`TRY_WIDTH`
/// env overrides (positive ints only), then winsize of stderr/stdout/stdin,
/// then the controlling console, else 24×80.
pub mod terminal {
    use crate::env::Env;

    fn env_positive(v: Option<&str>) -> Option<usize> {
        // Ruby String#to_i: leading integer prefix, else 0; only positive wins.
        let v = v?;
        let digits: String = v
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let n: usize = digits.parse().ok()?;
        (n > 0).then_some(n)
    }

    #[cfg(unix)]
    fn winsize_of(fd: std::os::fd::BorrowedFd<'_>) -> Option<(usize, usize)> {
        let ws = rustix::termios::tcgetwinsize(fd).ok()?;
        Some((ws.ws_row as usize, ws.ws_col as usize))
    }

    /// `(rows, cols)`.
    #[must_use]
    pub fn size(env: &Env) -> (usize, usize) {
        let mut rows = env_positive(env.try_height.as_deref());
        let mut cols = env_positive(env.try_width.as_deref());

        #[cfg(unix)]
        {
            use std::os::fd::AsFd;
            let err = std::io::stderr();
            let out = std::io::stdout();
            let inp = std::io::stdin();
            for fd in [err.as_fd(), out.as_fd(), inp.as_fd()] {
                if rows.is_some() && cols.is_some() {
                    break;
                }
                if let Some((r, c)) = winsize_of(fd) {
                    rows = rows.or(Some(r));
                    cols = cols.or(Some(c));
                }
            }
            if rows.is_none() || cols.is_none() {
                if let Ok(tty) = std::fs::File::open("/dev/tty") {
                    if let Some((r, c)) = winsize_of(tty.as_fd()) {
                        rows = rows.or(Some(r));
                        cols = cols.or(Some(c));
                    }
                }
            }
        }

        (rows.unwrap_or(24), cols.unwrap_or(80))
    }

    /// Rows of the CONTROLLING console only — ignores `TRY_HEIGHT` — used by
    /// the fuzzy result limit (`try.rb:167`: `IO.console&.winsize&.first || 24`).
    /// The quirk is upstream behavior; port it, don't fix it.
    #[must_use]
    pub fn console_rows() -> usize {
        #[cfg(unix)]
        {
            use std::os::fd::AsFd;
            if let Ok(tty) = std::fs::File::open("/dev/tty") {
                if let Some((r, _)) = winsize_of(tty.as_fd()) {
                    return r;
                }
            }
        }
        24
    }
}

/// One styled/fill/emoji chunk in a [`SegmentWriter`].
enum Segment {
    Str(String),
    Fill {
        pattern: String,
        style: Option<Style>,
    },
    Emoji(String),
}

#[derive(Clone, Copy)]
enum Style {
    Dim,
}

/// Left/center/right line writer (`tui.rb:638-801`), reduced to the
/// operations the selector actually uses.
#[derive(Default)]
pub struct SegmentWriter {
    segments: Vec<Segment>,
}

impl SegmentWriter {
    /// Append plain (or pre-styled) text.
    pub fn write(&mut self, t: &str) -> &mut Self {
        if !t.is_empty() {
            self.segments.push(Segment::Str(t.to_string()));
        }
        self
    }

    /// Append dim-styled text.
    pub fn write_dim(&mut self, t: &str) -> &mut Self {
        self.write(&text::dim(t))
    }

    /// Append bold-styled text.
    pub fn write_bold(&mut self, t: &str) -> &mut Self {
        self.write(&text::bold(t))
    }

    /// Append a dim fill that pads to `width - 1` (`fill("─")` +
    /// `write_dim` in the selector; `tui.rb:298-300,772-785`).
    pub fn write_dim_fill(&mut self, pattern: &str) -> &mut Self {
        self.segments.push(Segment::Fill {
            pattern: pattern.to_string(),
            style: Some(Style::Dim),
        });
        self
    }

    /// Append an emoji segment (`tui.rb:302-305`).
    pub fn write_emoji(&mut self, ch: &str) -> &mut Self {
        self.segments.push(Segment::Emoji(ch.to_string()));
        self
    }

    /// Render segments to a string (`tui.rb:727-741`); fills need `width`.
    fn render(&self, width: usize) -> String {
        let mut rendered = String::new();
        for seg in &self.segments {
            match seg {
                Segment::Str(s) | Segment::Emoji(s) => rendered.push_str(s),
                Segment::Fill { pattern, style } => {
                    let max_fill = width.saturating_sub(1);
                    let current = metrics::visible_width(&rendered);
                    if max_fill <= current {
                        continue;
                    }
                    let remaining = max_fill - current;
                    let pattern = if pattern.is_empty() { " " } else { pattern };
                    let pattern_width = metrics::visible_width(pattern).max(1);
                    let repeat = remaining.div_ceil(pattern_width);
                    let filler = pattern.repeat(repeat);
                    let filler = metrics::truncate(&filler, remaining, "");
                    match style {
                        Some(Style::Dim) => rendered.push_str(&text::dim(&filler)),
                        None => rendered.push_str(&filler),
                    }
                }
            }
        }
        rendered
    }
}

/// Single-input-per-screen text field (`tui.rb:803-834`): renders the
/// buffer with a reverse-video cursor cell.
pub struct InputField {
    text: Vec<char>,
    cursor: usize,
}

impl InputField {
    /// `value` + clamped `cursor` (char index).
    #[must_use]
    pub fn new(value: &str, cursor: usize) -> Self {
        let text: Vec<char> = value.chars().collect();
        let cursor = cursor.min(text.len());
        Self { text, cursor }
    }

    /// Cursor char index (for cursor-column math).
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Render with the inverted cursor cell (`tui.rb:813-827`). An empty
    /// buffer renders the placeholder (dim, empty here) — NO cursor cell
    /// (`tui.rb:814`); golden frames pin this.
    #[must_use]
    pub fn render(&self) -> String {
        if self.text.is_empty() {
            return String::new(); // Text.dim("") == ""
        }
        let before: String = self.text[..self.cursor].iter().collect();
        let cursor_char = self.text.get(self.cursor).copied().unwrap_or(' ');
        let after: String = if self.cursor < self.text.len() {
            self.text[self.cursor + 1..].iter().collect()
        } else {
            String::new()
        };
        let mut buf = String::new();
        buf.push_str(&before);
        if colors_enabled() {
            buf.push_str(palette::INPUT_CURSOR_ON);
        }
        buf.push(cursor_char);
        if colors_enabled() {
            buf.push_str(palette::INPUT_CURSOR_OFF);
        }
        buf.push_str(&after);
        buf
    }
}

/// One rendered line (`tui.rb:501-636`): left/center/right writers, optional
/// background, ANSI-aware truncation, rendered as `\r` + clear + content.
pub struct Line {
    /// Background SGR (palette constant), applied when colors are on.
    pub background: Option<&'static str>,
    /// Left writer (z-order middle).
    pub left: SegmentWriter,
    /// Center writer (renders on top).
    pub center: SegmentWriter,
    /// Right writer (lowest layer; overwritten on overlap).
    pub right: SegmentWriter,
    has_input: bool,
    input_prefix_width: usize,
}

impl Line {
    fn new(background: Option<&'static str>) -> Self {
        Self {
            background,
            left: SegmentWriter::default(),
            center: SegmentWriter::default(),
            right: SegmentWriter::default(),
            has_input: false,
            input_prefix_width: 0,
        }
    }

    /// Mark this line as holding the screen's input field, recording the
    /// prefix width for cursor positioning (`tui.rb:535-543`).
    pub fn mark_has_input(&mut self, prefix_width: usize) {
        self.has_input = true;
        self.input_prefix_width = prefix_width;
    }

    fn render_into(&self, buf: &mut String, width: usize, trailing_newline: bool) {
        buf.push('\r');
        buf.push_str(ansi::CLEAR_EOL);
        if let Some(bg) = self.background {
            if colors_enabled() {
                buf.push_str(bg);
            }
        }

        let max_content = width.saturating_sub(1);
        let content_width = width.max(1);

        let mut left_text = self.left.render(content_width);
        let mut center_text = self.center.render(content_width);
        let mut right_text = self.right.render(content_width);

        if !left_text.is_empty() {
            left_text = metrics::truncate(&left_text, max_content, "…");
        }
        let left_width = if left_text.is_empty() {
            0
        } else {
            metrics::visible_width(&left_text)
        };

        if !center_text.is_empty() {
            let max_center = max_content as i64 - left_width as i64 - 4;
            if max_center > 0 {
                #[allow(clippy::cast_sign_loss, reason = "checked > 0 above")]
                {
                    center_text = metrics::truncate(&center_text, max_center as usize, "…");
                }
            } else {
                center_text = String::new();
            }
        }
        let center_width = if center_text.is_empty() {
            0
        } else {
            metrics::visible_width(&center_text)
        };

        let used_by_left_center = left_width + center_width + if center_width > 0 { 2 } else { 0 };
        let available_for_right = max_content as i64 - used_by_left_center as i64 - 1;

        let mut right_width = 0;
        if !right_text.is_empty() {
            right_width = metrics::visible_width(&right_text);
            if available_for_right <= 0 {
                right_text = String::new();
                right_width = 0;
            } else {
                #[allow(clippy::cast_sign_loss, reason = "checked > 0 above")]
                let avail = available_for_right as usize;
                if right_width > avail {
                    right_text = metrics::truncate_from_start(&right_text, avail);
                    right_width = metrics::visible_width(&right_text);
                }
            }
        }

        let center_col = if center_text.is_empty() {
            0
        } else {
            ((max_content.saturating_sub(center_width)) / 2).max(left_width + 1)
        };
        let right_col = if right_text.is_empty() {
            max_content
        } else {
            max_content - right_width
        };

        if !left_text.is_empty() {
            buf.push_str(&left_text);
        }
        let mut current_pos = left_width;

        if !center_text.is_empty() {
            if center_col > current_pos {
                buf.push_str(&" ".repeat(center_col - current_pos));
            }
            buf.push_str(&center_text);
            current_pos = center_col + center_width;
        }

        let fill_end = if right_text.is_empty() {
            max_content
        } else {
            right_col
        };
        if fill_end > current_pos {
            buf.push_str(&" ".repeat(fill_end - current_pos));
        }

        if !right_text.is_empty() {
            buf.push_str(&right_text);
            buf.push_str(ansi::RESET_FG);
        }

        buf.push_str(ansi::RESET);
        if trailing_newline {
            buf.push('\n');
        }
    }
}

/// Header/body/footer line collection.
#[derive(Default)]
pub struct Section {
    /// Lines in order.
    pub lines: Vec<Line>,
}

impl Section {
    /// Append a line with optional background; returns it for writing.
    pub fn add_line(&mut self, background: Option<&'static str>) -> &mut Line {
        self.lines.push(Line::new(background));
        let idx = self.lines.len() - 1;
        &mut self.lines[idx]
    }
}

/// Frame builder (`tui.rb:350-472`): header + body + sticky footer, gap
/// fill, input-cursor positioning, single write.
pub struct Screen {
    /// Header section.
    pub header: Section,
    /// Body section (clipped to available space).
    pub body: Section,
    /// Sticky footer.
    pub footer: Section,
    /// The screen's single input field, if any.
    pub input_field: Option<InputField>,
    width: usize,
    height: usize,
}

impl Screen {
    /// New screen sized from the environment/terminal.
    #[must_use]
    pub fn new(env: &crate::env::Env) -> Self {
        let (height, width) = terminal::size(env);
        Self {
            header: Section::default(),
            body: Section::default(),
            footer: Section::default(),
            input_field: None,
            width,
            height,
        }
    }

    /// Terminal columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Terminal rows.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Register the screen's input field and get its rendered text.
    pub fn input(&mut self, value: &str, cursor: usize) -> String {
        let field = InputField::new(value, cursor);
        let rendered = field.render();
        self.input_field = Some(field);
        rendered
    }

    /// Build and write the frame in one `write` (`tui.rb:386-471`).
    pub fn flush(self, io: &mut dyn Write) {
        let mut buf = String::from(ansi::HOME);
        let mut cursor_row = None;
        let mut cursor_col = None;
        let mut current_row = 0usize;

        for line in &self.header.lines {
            if let Some(field) = &self.input_field {
                if line.has_input {
                    cursor_row = Some(current_row + 1);
                    cursor_col = Some(line.input_prefix_width + field.cursor() + 1);
                }
            }
            line.render_into(&mut buf, self.width, true);
            current_row += 1;
        }

        let footer_lines = self.footer.lines.len();
        let body_space = self.height as i64 - current_row as i64 - footer_lines as i64;

        let mut body_rendered = 0i64;
        for line in &self.body.lines {
            if body_rendered >= body_space {
                break;
            }
            if let Some(field) = &self.input_field {
                if line.has_input {
                    cursor_row = Some(current_row + 1);
                    cursor_col = Some(line.input_prefix_width + field.cursor() + 1);
                }
            }
            line.render_into(&mut buf, self.width, true);
            current_row += 1;
            body_rendered += 1;
        }

        // Gap fill between body and sticky footer (tui.rb:423-436)
        let gap = body_space - body_rendered;
        if gap > 0 {
            let spaces = " ".repeat(self.width.saturating_sub(1));
            let blank_line = format!("\r{}{}\n", ansi::CLEAR_EOL, spaces);
            let blank_line_no_newline = format!("\r{}{}", ansi::CLEAR_EOL, spaces);
            for i in 0..gap {
                if i == gap - 1 && self.footer.lines.is_empty() {
                    buf.push_str(&blank_line_no_newline);
                } else {
                    buf.push_str(&blank_line);
                }
                current_row += 1;
            }
        }

        for (idx, line) in self.footer.lines.iter().enumerate() {
            if let Some(field) = &self.input_field {
                if line.has_input {
                    cursor_row = Some(current_row + 1);
                    cursor_col = Some(line.input_prefix_width + field.cursor() + 1);
                }
            }
            let last = idx == footer_lines - 1;
            line.render_into(&mut buf, self.width, !last);
            current_row += 1;
        }

        if let (Some(row), Some(col), true) = (cursor_row, cursor_col, self.input_field.is_some()) {
            use std::fmt::Write as _;
            let _ = write!(buf, "\x1b[{row};{col}H");
            buf.push_str(ansi::SHOW);
        } else {
            buf.push_str(ansi::HIDE);
        }
        buf.push_str(ansi::RESET);

        let _ = io.write_all(buf.as_bytes());
        let _ = io.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_table_is_naive_on_purpose() {
        assert_eq!(metrics::char_width(0x1F4C1), 2); // 📁
        assert_eq!(metrics::char_width(0xFE0F), 0); // VS16
        assert_eq!(metrics::char_width('→' as u32), 1); // arrows are 1, not 2
        assert_eq!(metrics::char_width('─' as u32), 1);
        assert_eq!(metrics::visible_width("🗑️"), 2); // emoji + VS16
        assert_eq!(metrics::visible_width("\x1b[1;33mab\x1b[0m"), 2);
    }

    #[test]
    fn truncate_preserves_escapes_and_rstrips() {
        let t = metrics::truncate("\x1b[2mhello world\x1b[0m", 7, "…");
        assert!(t.starts_with("\x1b[2m"));
        assert!(t.ends_with('…'));
        // "hello " rstripped to "hello"
        assert!(t.contains("hello"));
        assert!(!t.contains("hello …"));
    }

    #[test]
    fn truncate_from_start_keeps_leading_escapes_and_tail() {
        let t = metrics::truncate_from_start("\x1b[2mabcdef\x1b[0m", 3);
        assert!(t.starts_with("\x1b[2m"));
        assert!(t.ends_with("def\x1b[0m"));
    }

    #[test]
    fn input_field_inverts_cursor_cell() {
        let _guard = TEST_COLOR_LOCK.lock().unwrap();
        set_colors_enabled(true);
        let f = InputField::new("abc", 1);
        assert_eq!(f.render(), format!("a{}b{}c", "\x1b[7m", "\x1b[27m"));
        // Cursor at end: inverted trailing space
        let f = InputField::new("ab", 2);
        assert_eq!(f.render(), format!("ab{} {}", "\x1b[7m", "\x1b[27m"));
    }

    #[test]
    fn text_wrappers_respect_color_gate() {
        let _guard = TEST_COLOR_LOCK.lock().unwrap();
        set_colors_enabled(false);
        assert_eq!(text::dim("x"), "x");
        assert_eq!(text::highlight("x"), "x");
        set_colors_enabled(true);
        assert_eq!(text::highlight("x"), "\x1b[1;33mx\x1b[39m\x1b[22m");
    }
}
