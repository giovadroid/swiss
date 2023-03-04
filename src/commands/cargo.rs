use crate::commands;
use crate::commands::CommandResult;
use home::home_dir;
use std::ffi::OsStr;
use std::fmt::Debug;

pub struct Cargo {}
const BIN_PATH: &str = ".cargo/bin/cargo";
impl Default for Cargo {
    fn default() -> Self {
        Self {}
    }
}

impl Cargo {
    pub fn install(package: &str) -> CommandResult<String> {
        log::info!("Installing {}", package);
        Self::run(&["install", package, "-j", "2"], None)
    }

    fn get_bin_path() -> CommandResult<String> {
        Ok(home_dir()
            .map(|path| path.join(BIN_PATH).to_str().unwrap().to_owned())
            .unwrap())
    }

    pub fn run<I, S>(args: I, working_dir: Option<&str>) -> CommandResult<String>
    where
        I: IntoIterator<Item = S> + Debug + Clone,
        S: AsRef<OsStr>,
    {
        commands::run_command(&Cargo::get_bin_path()?, args, working_dir)
    }
}
