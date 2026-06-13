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
    /// Shell startup hook: prints the live init script for a shell
    Init {
        /// Shell to initialize: nushell, zsh, bash or pwsh (default: detect
        /// from $SHELL)
        #[clap(short, long)]
        shell: Option<String>,
    },

    /// Show the execution plan for a manifest without changing anything
    Plan {
        #[clap(flatten)]
        manifest: ManifestArgs,
    },

    /// Bootstrap only: execute the manifest plan (no shell registration)
    Apply {
        #[clap(flatten)]
        manifest: ManifestArgs,

        /// Do not ask for confirmation
        #[clap(short, long)]
        yes: bool,

        /// Print the plan instead of executing it
        #[clap(long)]
        dry_run: bool,

        /// Rewrite generated/declared files even if they exist
        #[clap(long)]
        force_files: bool,
    },

    /// Full setup: apply the manifest, register it as active and hook the
    /// shell init into your startup files
    Setup {
        #[clap(flatten)]
        manifest: ManifestArgs,

        /// Create the manifest from this template if it does not exist
        /// (workstation, service-host or dev-shell)
        #[clap(short, long)]
        template: Option<String>,

        /// Shell to register the init hook in (default: autodetect)
        #[clap(short, long)]
        shell: Option<String>,

        /// Do not ask for confirmation
        #[clap(short, long)]
        yes: bool,

        /// Print the plan instead of executing it
        #[clap(long)]
        dry_run: bool,

        /// Rewrite generated/declared files even if they exist
        #[clap(long)]
        force_files: bool,
    },

    /// Re-apply the registered manifest to update everything bootstrapped
    Update {
        /// Manifest to update from (default: the one registered by setup/apply)
        #[clap(short, long)]
        manifest: Option<PathBuf>,

        /// Profile(s) to overlay (default: the ones registered by setup/apply)
        #[clap(short, long = "profile")]
        profiles: Vec<String>,

        /// Do not ask for confirmation
        #[clap(short, long)]
        yes: bool,

        /// Print the plan instead of executing it
        #[clap(long)]
        dry_run: bool,
    },

    /// Show cached dependency state and the registered manifest
    Status {},

    /// Diagnose the installation and, optionally, a manifest
    Doctor {
        /// Manifest to validate (load, profiles, templates, plan)
        #[clap(short, long)]
        manifest: Option<PathBuf>,

        /// Profile(s) to overlay during validation
        #[clap(short, long = "profile")]
        profiles: Vec<String>,
    },

    /// Delete the Swiss cache so the next apply re-runs everything
    CleanCache {},
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
    fn setup_parses_template_shell_and_profiles() {
        let cli = Cli::try_parse_from([
            "swiss",
            "setup",
            "--manifest",
            "./bootstrap.yaml",
            "--template",
            "dev-shell",
            "--shell",
            "zsh",
            "--profile",
            "base",
            "--profile",
            "devops",
            "--yes",
        ])
        .unwrap();

        match cli.command {
            Command::Setup {
                manifest,
                template,
                shell,
                yes,
                ..
            } => {
                assert_eq!(manifest.manifest, PathBuf::from("./bootstrap.yaml"));
                assert_eq!(manifest.profiles, vec!["base", "devops"]);
                assert_eq!(template.as_deref(), Some("dev-shell"));
                assert_eq!(shell.as_deref(), Some("zsh"));
                assert!(yes);
            }
            _ => panic!("expected setup command"),
        }
    }

    #[test]
    fn init_accepts_optional_shell() {
        let cli = Cli::try_parse_from(["swiss", "init"]).unwrap();
        match cli.command {
            Command::Init { shell } => assert!(shell.is_none()),
            _ => panic!("expected init"),
        }

        let cli = Cli::try_parse_from(["swiss", "init", "--shell", "zsh"]).unwrap();
        match cli.command {
            Command::Init { shell } => assert_eq!(shell.as_deref(), Some("zsh")),
            _ => panic!("expected init"),
        }
    }

    #[test]
    fn update_works_without_manifest() {
        let cli = Cli::try_parse_from(["swiss", "update", "--yes"]).unwrap();
        match cli.command {
            Command::Update { manifest, yes, .. } => {
                assert!(manifest.is_none());
                assert!(yes);
            }
            _ => panic!("expected update"),
        }
    }

    #[test]
    fn doctor_accepts_optional_manifest() {
        assert!(Cli::try_parse_from(["swiss", "doctor"]).is_ok());
        let cli =
            Cli::try_parse_from(["swiss", "doctor", "-m", "x.yaml", "-p", "service-host"]).unwrap();
        match cli.command {
            Command::Doctor { manifest, profiles } => {
                assert_eq!(manifest, Some(PathBuf::from("x.yaml")));
                assert_eq!(profiles, vec!["service-host"]);
            }
            _ => panic!("expected doctor"),
        }
    }

    #[test]
    fn plan_and_apply_parse() {
        assert!(Cli::try_parse_from(["swiss", "plan", "-m", "x.yaml"]).is_ok());
        assert!(Cli::try_parse_from([
            "swiss",
            "apply",
            "-m",
            "x.yaml",
            "--dry-run",
            "--force-files"
        ])
        .is_ok());
    }

    #[test]
    fn removed_commands_are_gone() {
        assert!(Cli::try_parse_from(["swiss", "files", "-m", "x.yaml"]).is_err());
        assert!(Cli::try_parse_from(["swiss", "init-config", "-o", "x.yaml"]).is_err());
        assert!(
            Cli::try_parse_from(["swiss", "shell", "print", "-m", "x.yaml", "-s", "zsh"]).is_err()
        );
        assert!(Cli::try_parse_from(["swiss", "test"]).is_err());
    }
}
