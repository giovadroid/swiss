use crate::commands;
use crate::commands::CommandResult;
use home::home_dir;
use std::ffi::OsStr;
use std::fmt::Debug;

#[derive(Debug, Default)]
pub struct Cargo {}
const BIN_PATH: &str = ".cargo/bin/cargo";
const INSTALL_COMMAND: &str = "binstall";

impl Cargo {
    pub fn install(package: &str, args: &[String]) -> CommandResult<String> {
        log::info!("Installing {}", package);
        let mut run_args = vec![INSTALL_COMMAND.to_owned(), "-y".to_owned(), package.to_owned()];
        run_args.extend(args.iter().cloned());
        Self::run(&run_args, None)
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
