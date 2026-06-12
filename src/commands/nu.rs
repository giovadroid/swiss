use crate::commands;
use crate::commands::CommandResult;
use std::ffi::OsStr;
use std::fmt::Debug;

#[cfg(not(target_os = "windows"))]
const BIN_NAME: &str = "nu";

#[cfg(target_os = "windows")]
const BIN_NAME: &str = "nu.exe";

pub struct NuShell {}

impl NuShell {
    pub fn run<I, S>(args: I, working_dir: Option<&str>) -> CommandResult<String>
    where
        I: IntoIterator<Item = S> + Debug + Clone,
        S: AsRef<OsStr>,
    {
        commands::run_command(BIN_NAME, args, working_dir)
    }

    pub fn is_installed() -> bool {
        std::process::Command::new(BIN_NAME)
            .arg("--version")
            .output()
            .is_ok()
    }

    pub(crate) fn get_env_value(env_name: &str) -> CommandResult<String> {
        #[cfg(not(target_os = "windows"))]
        let args = ["-c", env_name];

        #[cfg(target_os = "windows")]
        let command = format!("echo {}", env_name);
        #[cfg(target_os = "windows")]
        let args = ["-c", command.as_str()];

        Self::run(&args, None)
    }

    /// Quiet probe: no live output echo, suitable for plan/doctor contexts.
    pub fn version() -> CommandResult<String> {
        let output = std::process::Command::new(BIN_NAME)
            .arg("--version")
            .output()?;
        if !output.status.success() {
            anyhow::bail!("nu --version failed");
        }
        let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        log::debug!("Nu version: {}", version);
        Ok(version)
    }

    /// Symlink nu binaries from `~/.cargo/bin` into `/usr/local/bin` so login
    /// shells and `/etc/shells` entries can find them.
    pub fn link() -> CommandResult<()> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use std::path::Path;

            let cargo_bins = Path::new(&home::home_dir().unwrap()).join(".cargo/bin");
            std::fs::read_dir(cargo_bins)?.for_each(|entry| {
                let path = entry.unwrap().path();
                if path.to_string_lossy().contains("nu") {
                    let path = path.to_path_buf();
                    let file_name = path.file_name().unwrap().to_string_lossy();
                    let target_path = Path::new("/usr/local/bin/").join(file_name.to_string());

                    std::os::unix::fs::symlink(&path, &target_path).unwrap_or(());

                    log::info!(
                        "linked {} to {}",
                        path.to_string_lossy(),
                        target_path.to_string_lossy()
                    );
                }
            });
        }
        Ok(())
    }
}
