/*
Initial tasks for the initial commit:

Have to commands
* `swiss update` to update the swiss-cli and all tools
    * from yaml file
* `swiss init` to generate nu config loader
* `swiss setup` will update nu config files env and conf

On cargo install we need to prepare swiss environment

* Installation will be in `~/.config/swiss/` this will be SWISS_HOME
* Create two environment variables SWISS_HOME and SWISS_VERSION
* Two config files for nu, `env` and `conf`
* `env` will be in `~/.config/swiss/env.nu`
* `conf` will be in `~/.config/swiss/conf.nu`
* `~/.swiss/env/` will contain `.nu` files for each environment
* `~/.swiss/conf/` will contain `.nu` files for each configuration
* `~/.config/swiss/env.dyn.nu` will contain all env files content to be loaded at nu initialization
* `~/.config/swiss/conf.dyn.nu` will contain all conf files content to be loaded at nu initialization
* After install we need to update $nu.env-path and $nu.conf-path
  * env-path, will load `swiss init`
  * env-path, will source `~/.config/swiss/env.dyn.nu`
  * conf-path, will source `~/.config/swiss/conf.dyn.nu`
* In yaml file we store dependencies  to install
    * External dependencies
        * name of the dependency
        * install command, will run under nu shell
        * uninstall command, will run under nu shell
    * Cargo dependencies
        * name of the dependency
        * version of the dependency, if not defined will use latest available
        * windows, if no, will not be installed on windows
        * mac, if no, will not be installed on mac
        * linux, if no, will not be installed on linux

    * installed on `~/.config/swiss/swiss.yaml`
 */

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(author, about, version)]
#[clap(propagate_version = true)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Command,

    /// Turn debugging information on
    #[clap(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Install and update all components
    Update {
        // TODO: add update options
    },

    /// Initialize swiss environment
    Init {
        // TODO: add init options
    },

    /// Setup swiss and all components
    Setup {
        // TODO: add setup options
    },

    /// some call to random function
    Test {
        /// runs nu install
        #[clap(short, long, default_value = "false")]
        nu: bool,
        // Will drop table compatible with nushell
        // TODO: add table options
    },

    /// Create folders and files
    Files {
        /// Overwrite existing files
        #[clap(short, long, default_value = "false")]
        force: bool,
    },
}
