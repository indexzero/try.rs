//! Environment snapshot — the only place process env is read.
//!
//! Core code receives this struct instead of touching `std::env`, so tests
//! (and the conformance suite's env-driven cases) exercise the same code
//! paths as production.

/// Snapshot of the environment variables upstream reads.
#[derive(Debug, Default, Clone)]
pub struct Env {
    /// `TRY_PATH` — tries root override, read in-process (`try.rb:12`),
    /// not just via the wrapper.
    pub try_path: Option<String>,
    /// `TRY_PROJECTS` — graduate destination override (`try.rb:13`).
    pub try_projects: Option<String>,
    /// `NO_COLOR` — no-color.org standard, honored in the dispatcher
    /// (`try.rb:1013`).
    pub no_color: Option<String>,
    /// `NO_COLORS` — plural variant honored by the TUI layer (`tui.rb:25`).
    pub no_colors: Option<String>,
    /// `SHELL` — shell detection for init/install.
    pub shell: Option<String>,
    /// `PSModulePath` — PowerShell session marker.
    pub psmodulepath: Option<String>,
    /// `PROFILE` — PowerShell profile path.
    pub profile: Option<String>,
    /// `USERPROFILE` — Windows home.
    pub userprofile: Option<String>,
    /// `HOME` — for `~` expansion.
    pub home: Option<String>,
    /// `PATH` — for resolving a bare `argv[0]` to the real binary location
    /// (wrapper emission must embed a path that exists).
    pub path: Option<String>,
    /// `TRY_WIDTH` — terminal width override (TUI layer).
    pub try_width: Option<String>,
    /// `TRY_HEIGHT` — terminal height override (TUI layer).
    pub try_height: Option<String>,
}

impl Env {
    /// Capture the real process environment.
    #[must_use]
    pub fn from_process() -> Self {
        let get = |k: &str| std::env::var(k).ok();
        Self {
            try_path: get("TRY_PATH"),
            try_projects: get("TRY_PROJECTS"),
            no_color: get("NO_COLOR"),
            no_colors: get("NO_COLORS"),
            shell: get("SHELL"),
            psmodulepath: get("PSModulePath"),
            profile: get("PROFILE"),
            userprofile: get("USERPROFILE"),
            home: get("HOME"),
            path: get("PATH"),
            try_width: get("TRY_WIDTH"),
            try_height: get("TRY_HEIGHT"),
        }
    }
}
