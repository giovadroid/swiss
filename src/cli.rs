use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

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

#[derive(Args, Debug, Clone)]
pub struct ManifestArgs {
    /// Path to the bootstrap manifest (YAML)
    #[clap(short, long)]
    pub manifest: PathBuf,

    /// Profile(s) to overlay on top of the manifest, in order
    #[clap(short, long = "profile")]
    pub profiles: Vec<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Print the Swiss environment for the Nushell loader (used at shell startup)
    Init {},

    /// Show the execution plan for a manifest without changing anything
    Plan {
        #[clap(flatten)]
        manifest: ManifestArgs,
    },

    /// Apply a manifest: install packages, tools, files and shell integration
    Apply {
        #[clap(flatten)]
        manifest: ManifestArgs,

        /// Do not ask for confirmation
        #[clap(short, long)]
        yes: bool,

        /// Print the plan instead of executing it
        #[clap(long)]
        dry_run: bool,
    },

    /// Apply a manifest with first-run checks (admin requirements, doctor)
    Setup {
        #[clap(flatten)]
        manifest: ManifestArgs,

        /// Do not ask for confirmation
        #[clap(short, long)]
        yes: bool,

        /// Print the plan instead of executing it
        #[clap(long)]
        dry_run: bool,
    },

    /// Create folders and files declared by the manifest (no installs)
    Files {
        #[clap(flatten)]
        manifest: ManifestArgs,

        /// Overwrite existing files
        #[clap(short, long, default_value = "false")]
        force: bool,
    },

    /// Shell integration: plan, apply or print generated shell code
    Shell {
        #[clap(subcommand)]
        command: ShellCommand,
    },

    /// Show cached dependency state and the last applied manifest fingerprint
    Status {},

    /// Check that the tools Swiss relies on are available
    Doctor {},

    /// Delete the Swiss cache so the next apply re-runs everything
    CleanCache {},

    /// Write a starter manifest from a built-in template
    InitConfig {
        /// Template name: workstation, service-host or dev-shell
        #[clap(short, long, default_value = "dev-shell")]
        template: String,

        /// Output path for the generated manifest
        #[clap(short, long)]
        output: PathBuf,

        /// Overwrite the output file if it exists
        #[clap(short, long, default_value = "false")]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ShellCommand {
    /// Show the shell integration steps for one shell
    Plan {
        #[clap(flatten)]
        manifest: ManifestArgs,

        /// Shell name: nushell, zsh, bash or pwsh
        #[clap(short, long)]
        shell: String,
    },

    /// Apply the shell integration steps for one shell
    Apply {
        #[clap(flatten)]
        manifest: ManifestArgs,

        /// Shell name: nushell, zsh, bash or pwsh
        #[clap(short, long)]
        shell: String,
    },

    /// Print the generated shell code to stdout
    Print {
        #[clap(flatten)]
        manifest: ManifestArgs,

        /// Shell name: nushell, zsh, bash or pwsh
        #[clap(short, long)]
        shell: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_requires_manifest() {
        let result = Cli::try_parse_from(["swiss", "setup"]);
        let error = result.err().expect("setup without manifest must fail");
        assert!(error.to_string().contains("--manifest"));
    }

    #[test]
    fn setup_parses_manifest_and_profiles() {
        let cli = Cli::try_parse_from([
            "swiss",
            "setup",
            "--manifest",
            "./bootstrap.yaml",
            "--profile",
            "base",
            "--profile",
            "devops",
            "--yes",
        ])
        .unwrap();

        match cli.command {
            Command::Setup { manifest, yes, .. } => {
                assert_eq!(manifest.manifest, PathBuf::from("./bootstrap.yaml"));
                assert_eq!(manifest.profiles, vec!["base", "devops"]);
                assert!(yes);
            }
            _ => panic!("expected setup command"),
        }
    }

    #[test]
    fn plan_and_apply_parse() {
        assert!(Cli::try_parse_from(["swiss", "plan", "-m", "x.yaml"]).is_ok());
        assert!(Cli::try_parse_from(["swiss", "apply", "-m", "x.yaml", "--dry-run"]).is_ok());
    }

    #[test]
    fn shell_subcommands_parse() {
        let cli =
            Cli::try_parse_from(["swiss", "shell", "print", "-m", "x.yaml", "--shell", "zsh"])
                .unwrap();
        match cli.command {
            Command::Shell {
                command: ShellCommand::Print { shell, .. },
            } => assert_eq!(shell, "zsh"),
            _ => panic!("expected shell print"),
        }
    }

    #[test]
    fn init_config_defaults_to_dev_shell_template() {
        let cli = Cli::try_parse_from(["swiss", "init-config", "--output", "boot.yaml"]).unwrap();
        match cli.command {
            Command::InitConfig { template, .. } => assert_eq!(template, "dev-shell"),
            _ => panic!("expected init-config"),
        }
    }

    #[test]
    fn debug_only_commands_are_gone() {
        assert!(Cli::try_parse_from(["swiss", "update"]).is_err());
        assert!(Cli::try_parse_from(["swiss", "test"]).is_err());
    }
}
