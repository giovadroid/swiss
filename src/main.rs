#![feature(trivial_bounds)]

mod cli;
mod commands;
mod installer;
mod loader;
mod logger;
mod parser;
mod persistence;
mod paths;
mod embedded;

use crate::commands::CommandResult;
use crate::installer::{Installer, InstallerError};
use crate::persistence::{NU_CONF_LOADER, NU_ENV_LOADER};
use clap::Parser;
use cli::Command;
use persistence::VERSION;
use std::collections::HashMap;

#[derive(thiserror::Error, Debug)]
pub enum CommandError {
    #[error("Default error")]
    DefaultError,
}

/// The initializer for nu shell
/// Will write all content of ~/.config/swiss/env/ to ~/.config/swiss/env.dyn.nu
/// Will write all content of ~/.config/swiss/conf/ to ~/.config/swiss/conf.dyn.nu
/// Will write all content of ~/.swiss/env/ to ~/.config/swiss/env.dyn.nu
/// Will write all content of ~/.swiss/conf/ to ~/.config/swiss/conf.dyn.nu
///
pub fn command_init() -> CommandResult<()> {
    let (home_dir, user_dir) = loader::initialize_nu_files()?;
    let data_envs: HashMap<String, String> = HashMap::from_iter(vec![
        ("SWISS_VERSION".to_string(), VERSION.to_string()),
        (
            "SWISS_HOME".to_string(),
            home_dir
                .to_str()
                .expect("Unable to get SWISS_HOME")
                .to_string(),
        ),
        (
            "SWISS_USER_HOME".to_string(),
            user_dir
                .to_str()
                .expect("Unable to get SWISS_USER_HOME")
                .to_string(),
        ),
    ]);

    println!(
        "{}",
        serde_yaml::to_string(&data_envs).expect("Failed to serialize")
    );
    // TODO: Now support to print export new vars from here.
    Ok(())
}

/// Read yaml file
/// build one line command to run in nu shell
/// Nushell will be bound always with version, so we check if last supported version is installed
/// cargo install ... all dependencies from yaml: `dependencies.cargo`
/// Then run all custom install if not installed and update if installed
/// Needs to have stored as persistent file in ~/.config/swiss/.cache
/// After ~/.config/swiss/swiss.yaml will do same for ~/.swiss/config.yaml
/// .cache will contain all dependencies and their versions
/// **NOTE**: If one package is not installed, needs to perform link to `/usr/local/bin` if sudo is available
pub fn command_update() -> CommandResult<()> {
    let mut installer_tool = Installer::default();

    installer_tool.create_folders()?;

    installer_tool.create_files(false)?;

    Ok(())
}

/// Will require admin rights to run
///
/// Check if nushell is installed, if not install it
/// else check if nushell is up to date, if not update it
///
///
/// Will create folder structures for swiss
/// - ~/.config/swiss/
/// - ~/.swiss
///     - env
///     - conf
///
/// Then will unpack their files in SWISS_HOME based on `./defaults` structure
///
/// Then will create empty dynamic files for env and conf
/// - ~/.config/swiss/env.dyn.nu
/// - ~/.config/swiss/conf.dyn.nu
///
/// Will install all dependencies from yaml
/// Will link all installed dependencies
///
/// Will generate dynamic files for env and conf
///
/// If /etc/shells not contain nushell register it
pub fn command_setup() -> CommandResult<()> {
    if !Installer::is_admin()? {
        return Err(InstallerError::AdminRequiredError.into());
    }

    let mut installer_tool = Installer::default();

    installer_tool.prepare_latest_nu()?;

    installer_tool.create_folders()?;

    installer_tool.install_binstall()?;

    installer_tool.install_cargo()?;

    installer_tool.install_custom()?;

    installer_tool.create_files(true)?;

    installer_tool.configure_nu()?;
    Ok(())
}

fn command_files(force: &bool) -> CommandResult<()> {
    let mut installer_tool = Installer::default();

    installer_tool.create_folders()?;

    installer_tool.create_files(force.to_owned())?;

    installer_tool.configure_nu()?;

    Ok(())
}

fn main() {
    let cli = cli::Cli::parse();

    logger::init(cli.verbose);

    let command_result = match &cli.command {
        Command::Init {} => command_init(),
        Command::Setup {} => command_setup(),
        #[cfg(debug_assertions)]
        Command::Update {} => command_update(),
        #[cfg(debug_assertions)]
        Command::Test { nu } => command_test(nu),
        Command::Files { force } => command_files(force),
    };

    if let Err(err) = command_result {
        log::debug!("ERROR: {:?}", err);

        std::process::exit(-1);
    }
}

#[cfg(debug_assertions)]
fn command_test(nu: &bool) -> CommandResult<()> {
    let mut installer_tool = Installer::default();

    if *nu {
        installer_tool.prepare_latest_nu()?;
    }
    Ok(())
}
