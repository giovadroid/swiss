use crate::report;
use colored::*;
use log::{Level, LevelFilter, Metadata, Record};

/// Logger that always records to the run log file and, only when it won't
/// disturb the progress view, mirrors records to stderr. Verbose lowers the
/// stderr threshold to Debug; otherwise Info and above.
struct SwissLogger {
    verbose: bool,
}

fn colored_level(level: Level) -> ColoredString {
    match level {
        Level::Error => "ERROR".red(),
        Level::Warn => "WARN".yellow(),
        Level::Info => "INFO".green(),
        Level::Debug => "DEBUG".blue(),
        Level::Trace => "TRACE".magenta(),
    }
}

impl log::Log for SwissLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        report::file_log(&format!("{:<5} {}", record.level(), record.args()));

        let threshold = if self.verbose {
            Level::Debug
        } else {
            Level::Info
        };
        // While a run owns the terminal, keep records off stderr (the run log
        // still has them) so the spinner is never garbled.
        if record.level() <= threshold && !report::progress_active() {
            eprintln!("{: <5} - {}", colored_level(record.level()), record.args());
        }
    }

    fn flush(&self) {}
}

pub fn init(verbose: bool) {
    report::set_verbose(verbose);
    let _ = log::set_boxed_logger(Box::new(SwissLogger { verbose }));
    // Capture everything to the file; stderr gating happens inside `log`.
    log::set_max_level(LevelFilter::Debug);
}
