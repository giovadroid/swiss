mod nu;

pub use nu::NuShell;

use std::ffi::OsStr;
use std::fmt::Debug;
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::atomic::AtomicUsize;

static MAX_WIDTH: AtomicUsize = AtomicUsize::new(0);

fn fill(value: &str) -> String {
    let width = MAX_WIDTH.load(std::sync::atomic::Ordering::Relaxed);
    if width < value.len() {
        MAX_WIDTH.store(value.len(), std::sync::atomic::Ordering::Relaxed);
        return value.to_owned();
    }
    format!("{}{}", value, " ".repeat(width - value.len()))
}

pub type CommandResult<T> = Result<T, anyhow::Error>;

pub(crate) fn run_command<I, S>(
    bin_path: &str,
    args: I,
    working_dir: Option<&str>,
) -> CommandResult<String>
where
    I: IntoIterator<Item = S> + Debug + Clone,
    S: AsRef<OsStr>,
{
    log::debug!("Running command: {} {:?}", bin_path, &args);
    let mut command_builder = std::process::Command::new(bin_path);
    command_builder
        .args(args.clone())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(working_dir) = working_dir {
        command_builder.current_dir(working_dir);
    }
    let mut command = command_builder.spawn()?;

    log::debug!("Command spawned with pid: {}", command.id());

    let reader = BufReader::new(command.stderr.take().unwrap());
    let mut stdout_lines: Vec<String> = Vec::new();
    log::debug!("Reading stderr");
    let mut stdout = std::io::stdout();
    for line in reader.lines() {
        let content = line?.replace('\"', "").trim().to_owned();
        stdout_lines.push(content.clone());
        print!("\r{}", fill(&content));
        stdout.flush()?;
    }

    let reader = BufReader::new(command.stdout.take().unwrap());
    log::debug!("Reading stdout");

    for line in reader.lines() {
        let content = line?.replace('\"', "").trim().to_owned();
        stdout_lines.push(content.clone());
        print!("\r{}", fill(&content));
        stdout.flush()?;
    }

    print!("\r{}", fill(""));
    print!("\r");
    stdout.flush()?;

    if !command.wait()?.success() {
        anyhow::bail!("Error running command: {} {:?}", bin_path, &args);
    }
    Ok(stdout_lines.join("\r"))
}
