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
    #[cfg(debug_assertions)]
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
    #[cfg(debug_assertions)]
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
