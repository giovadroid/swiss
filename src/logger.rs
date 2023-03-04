use colored::*;
use std::io::Write;

struct ColoredLevel(log::Level);

impl From<log::Level> for ColoredLevel {
    fn from(level: log::Level) -> Self {
        ColoredLevel(level)
    }
}

impl std::fmt::Display for ColoredLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let colored_level = match self.0 {
            log::Level::Error => "ERROR".red(),
            log::Level::Warn => "WARN".yellow(),
            log::Level::Info => "INFO".green(),
            log::Level::Debug => "DEBUG".blue(),
            log::Level::Trace => "TRACE".magenta(),
        };

        write!(f, "{: <5}", colored_level)
    }
}

pub fn colorize_string(s: String) -> String {
    // TODO: Add colorizing for strings e.g. "This is a message \"String\"")
    //       or files.

    s
}

pub fn init(verbose: bool) {
    let level = if verbose || cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level))
        .format(|buf, record| {
            writeln!(
                buf,
                "{} - {}",
                ColoredLevel::from(record.level()),
                colorize_string(record.args().to_string())
            )
        })
        .init();
}
