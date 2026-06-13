mod nu;

pub use nu::NuShell;

use crate::report;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

pub type CommandResult<T> = Result<T, anyhow::Error>;

/// Runs a child process to completion, capturing both streams. Output goes to
/// the run log (and to stderr only when verbose) instead of being streamed to
/// the terminal, so the progress view stays clean. Returns captured stdout.
pub(crate) fn run_command<I, S>(
    bin_path: &str,
    args: I,
    working_dir: Option<&str>,
) -> CommandResult<String>
where
    I: IntoIterator<Item = S> + Debug + Clone,
    S: AsRef<OsStr>,
{
    report::file_log(&format!("$ {} {:?}", bin_path, &args));

    let mut builder = Command::new(bin_path);
    builder
        .args(args.clone())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(working_dir) = working_dir {
        builder.current_dir(working_dir);
    }
    let mut child = builder.spawn()?;

    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");

    // Drain stdout on its own thread so a process that fills the stdout pipe
    // while we're blocked reading stderr can never deadlock.
    let stdout_reader = std::thread::spawn(move || {
        let mut lines = Vec::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            report::command_output("out", &line);
            lines.push(line);
        }
        lines
    });

    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        report::command_output("err", &line);
    }

    let stdout_lines = stdout_reader.join().unwrap_or_default();
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("Command failed ({}): {} {:?}", status, bin_path, &args);
    }
    Ok(stdout_lines.join("\n"))
}
