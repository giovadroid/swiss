//! Run reporting: a timestamped log file that captures everything (command
//! output and diagnostics), plus a modern progress view on the terminal — a
//! spinner per step that resolves to a check or a cross. Execution never stops
//! at the first failure; failures are collected and surfaced in the summary.

use colored::*;
use std::fs::{File, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

static RUN_LOG: Mutex<Option<File>> = Mutex::new(None);
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static VERBOSE: AtomicBool = AtomicBool::new(false);
/// True while a `Reporter` owns the terminal, so the logger keeps its records
/// off stderr (file only) and never garbles the spinner.
static PROGRESS_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(value: bool) {
    VERBOSE.store(value, Ordering::Relaxed);
}

pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

pub fn progress_active() -> bool {
    PROGRESS_ACTIVE.load(Ordering::Relaxed)
}

/// Opens a fresh timestamped log under `~/.config/swiss/logs`. Best-effort:
/// returns `None` (and logging stays terminal-only) if the file can't be made.
pub fn open_run_log() -> Option<PathBuf> {
    let dir = home::home_dir()?.join(".config/swiss/logs");
    std::fs::create_dir_all(&dir).ok()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("run-{}-{}.log", stamp, std::process::id()));
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    *RUN_LOG.lock().ok()? = Some(file);
    *LOG_PATH.lock().ok()? = Some(path.clone());
    file_log(&format!("# swiss run log — {}", path.display()));
    Some(path)
}

pub fn log_path() -> Option<PathBuf> {
    LOG_PATH.lock().ok().and_then(|guard| guard.clone())
}

/// Appends one line to the run log file (no terminal output). No-op when no
/// run log is open.
pub fn file_log(line: &str) {
    if let Ok(mut guard) = RUN_LOG.lock() {
        if let Some(file) = guard.as_mut() {
            let _ = writeln!(file, "{}", line);
        }
    }
}

/// One line of a child process's output: always to the log file, and echoed
/// to stderr (dimmed) when running verbose.
pub fn command_output(stream: &str, line: &str) {
    file_log(&format!("  [{}] {}", stream, line));
    if is_verbose() {
        eprintln!("    {}", line.dimmed());
    }
}

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const MAX_LABEL: usize = 72;

/// One-line, length-capped label for the terminal (the full text still goes to
/// the log file).
fn shorten(label: &str) -> String {
    let single = label.replace('\n', " ");
    if single.chars().count() <= MAX_LABEL {
        single
    } else {
        let head: String = single.chars().take(MAX_LABEL - 1).collect();
        format!("{}…", head)
    }
}

/// A background spinner animating a single terminal line until stopped.
struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    fn start(label: String) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            let mut frame = 0usize;
            while !flag.load(Ordering::Relaxed) {
                eprint!("\r\x1b[2K{} {}", FRAMES[frame % FRAMES.len()].cyan(), label);
                let _ = std::io::stderr().flush();
                frame += 1;
                std::thread::sleep(Duration::from_millis(90));
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Outcome of a whole run: counts, the labels that failed, and where the full
/// log lives.
#[derive(Debug, Default)]
pub struct Outcome {
    pub completed: usize,
    pub failed: usize,
    pub failures: Vec<String>,
    pub log_path: Option<PathBuf>,
}

impl Outcome {
    pub fn succeeded(&self) -> bool {
        self.failed == 0
    }
}

/// Drives the per-step terminal view. Animates a spinner on a TTY (unless
/// verbose, where output streams linearly instead).
pub struct Reporter {
    animate: bool,
    total: usize,
    index: usize,
    completed: usize,
    failed: usize,
    failures: Vec<String>,
}

impl Reporter {
    pub fn new(total: usize) -> Self {
        PROGRESS_ACTIVE.store(true, Ordering::Relaxed);
        Self {
            animate: std::io::stderr().is_terminal() && !is_verbose(),
            total,
            index: 0,
            completed: 0,
            failed: 0,
            failures: Vec::new(),
        }
    }

    /// Runs one step's `action`, showing its progress and resolving to a check
    /// or a cross. Returns whether it succeeded.
    pub fn step<F>(&mut self, label: &str, action: F) -> bool
    where
        F: FnOnce() -> anyhow::Result<()>,
    {
        self.index += 1;
        let short = shorten(label);
        let prefix = format!("[{}/{}]", self.index, self.total);
        file_log(&format!("\n=== {} {} ===", prefix, label));

        let spinner = if self.animate {
            Some(Spinner::start(format!("{} {}", prefix.dimmed(), short)))
        } else {
            if is_verbose() {
                eprintln!("{} {}", prefix.dimmed(), short);
            }
            None
        };

        let result = action();
        if let Some(spinner) = spinner {
            spinner.stop();
        }
        if self.animate {
            eprint!("\r\x1b[2K");
        }

        match result {
            Ok(()) => {
                self.completed += 1;
                eprintln!("{} {} {}", "✓".green(), prefix.dimmed(), short);
                file_log("--> ok");
                true
            }
            Err(error) => {
                self.failed += 1;
                self.failures.push(short.clone());
                eprintln!("{} {} {}", "✗".red(), prefix.dimmed(), short);
                file_log(&format!("--> FAILED: {:#}", error));
                false
            }
        }
    }

    pub fn into_outcome(self) -> Outcome {
        PROGRESS_ACTIVE.store(false, Ordering::Relaxed);
        Outcome {
            completed: self.completed,
            failed: self.failed,
            failures: self.failures,
            log_path: log_path(),
        }
    }
}

/// Final tally printed after a run.
pub fn print_summary(outcome: &Outcome) {
    eprintln!();
    if outcome.succeeded() {
        eprintln!("{} {} step(s) completed", "✓".green(), outcome.completed);
    } else {
        eprintln!(
            "{} {} completed   {} {} failed",
            "✓".green(),
            outcome.completed,
            "✗".red(),
            outcome.failed
        );
        for failure in &outcome.failures {
            eprintln!("    {} {}", "✗".red(), failure);
        }
    }
    if let Some(path) = &outcome.log_path {
        eprintln!(
            "  {} {}",
            "log:".dimmed(),
            path.display().to_string().dimmed()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_caps_long_labels() {
        let long = "x".repeat(200);
        let short = shorten(&long);
        assert!(short.chars().count() <= MAX_LABEL);
        assert!(short.ends_with('…'));
    }

    #[test]
    fn shorten_flattens_newlines() {
        assert_eq!(shorten("a\nb"), "a b");
    }

    #[test]
    fn outcome_succeeds_only_without_failures() {
        let mut outcome = Outcome {
            completed: 3,
            ..Outcome::default()
        };
        assert!(outcome.succeeded());
        outcome.failed = 1;
        assert!(!outcome.succeeded());
    }
}
