mod cli;
mod commands;
mod config_loader;
mod embedded;
mod executor;
mod expand;
mod loader;
mod logger;
mod parser;
mod persistence;
mod plan;
mod shellgen;

use crate::cli::{Command, ManifestArgs, ShellCommand};
use crate::commands::{CommandResult, NuShell};
use crate::config_loader::LoadedManifest;
use crate::executor::Executor;
use crate::parser::Os;
use crate::persistence::{SwissCache, FINGERPRINT_KEY, VERSION};
use crate::plan::{build_plan, Plan, PlanContext};
use crate::shellgen::ShellKind;
use anyhow::{bail, Context};
use clap::Parser;
use std::collections::HashMap;
use std::io::Write;

/// The initializer for nu shell.
/// Aggregates ~/.config/swiss/{env,conf} and ~/.swiss/{env,conf} into the
/// dynamic loader files and prints the Swiss environment as YAML.
fn command_init() -> CommandResult<()> {
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
    Ok(())
}

fn load_manifest(args: &ManifestArgs) -> CommandResult<LoadedManifest> {
    config_loader::load_manifest(&args.manifest, &args.profiles).context(
        "Could not load the bootstrap manifest. Pass one with --manifest <path>, \
         or generate a starter with `swiss init-config --output bootstrap.yaml`",
    )
}

fn plan_context(manifest: &LoadedManifest) -> PlanContext {
    let nu_installed = NuShell::is_installed();
    let nu_version = if nu_installed {
        NuShell::version().ok()
    } else {
        None
    };
    PlanContext {
        os: Os::current(),
        home: home::home_dir().expect("Unable to obtain home directory"),
        base_dir: manifest.base_dir.clone(),
        cache: SwissCache::load().unwrap_or_default(),
        nu_installed,
        nu_version,
    }
}

fn apply_env_defaults(manifest: &LoadedManifest) {
    for (name, requirement) in &manifest.config.env {
        let source = requirement.from_env.clone().unwrap_or_else(|| name.clone());
        if std::env::var(&source).is_err() {
            if let Some(default) = &requirement.default {
                std::env::set_var(&source, default);
            }
        }
    }
}

fn confirm(plan: &Plan, yes: bool) -> CommandResult<bool> {
    println!("{}", plan.render());
    if yes {
        return Ok(true);
    }
    print!("Apply {} step(s)? [y/N] ", plan.steps.len());
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn execute_plan(plan: &Plan, manifest: &LoadedManifest, yes: bool) -> CommandResult<()> {
    if plan.is_empty() {
        println!("Nothing to do: everything is up to date.");
        return Ok(());
    }
    if !confirm(plan, yes)? {
        println!("Aborted.");
        return Ok(());
    }
    apply_env_defaults(manifest);
    let mut executor = Executor::new(SwissCache::load().unwrap_or_default());
    executor.execute(plan, Some(&manifest.fingerprint))?;
    println!("Done: {} step(s) applied.", plan.steps.len());
    Ok(())
}

fn command_plan(args: &ManifestArgs) -> CommandResult<()> {
    let manifest = load_manifest(args)?;
    let context = plan_context(&manifest);
    let plan = build_plan(&manifest, &context)?;
    println!("{}", plan.render());
    Ok(())
}

fn command_apply(
    args: &ManifestArgs,
    yes: bool,
    dry_run: bool,
    first_run_checks: bool,
) -> CommandResult<()> {
    let manifest = load_manifest(args)?;
    let context = plan_context(&manifest);
    let plan = build_plan(&manifest, &context)?;

    if dry_run {
        println!("{}", plan.render());
        return Ok(());
    }

    if plan.requires_admin() && !executor::is_admin()? {
        if first_run_checks {
            bail!(
                "This plan contains steps that require administrator rights. \
                 Re-run as administrator, or use `swiss apply` to proceed anyway."
            );
        }
        log::warn!("Some steps require administrator rights and may fail.");
    }

    execute_plan(&plan, &manifest, yes)
}

fn command_files(args: &ManifestArgs, force: bool) -> CommandResult<()> {
    let manifest = load_manifest(args)?;
    let context = plan_context(&manifest);
    let plan = build_plan(&manifest, &context)?.file_steps(force);

    if plan.is_empty() {
        println!("The manifest declares no files or shell integration.");
        return Ok(());
    }
    apply_env_defaults(&manifest);
    let mut executor = Executor::new(SwissCache::load().unwrap_or_default());
    executor.execute(&plan, None)?;
    println!("Done: {} file step(s) applied.", plan.steps.len());
    Ok(())
}

fn shell_only_plan(manifest: &LoadedManifest, shell: &str) -> CommandResult<Plan> {
    let kind =
        ShellKind::from_name(shell).ok_or_else(|| anyhow::anyhow!("Unknown shell '{}'", shell))?;
    let single = config_for_single_shell(manifest, kind)?;
    let context = plan_context(manifest);
    Ok(Plan {
        steps: shellgen::shell_steps(&single.config, &context)?,
    })
}

fn config_for_single_shell(
    manifest: &LoadedManifest,
    kind: ShellKind,
) -> CommandResult<LoadedManifest> {
    let mut single = manifest.clone();
    let Some(target) = single.config.shells.get(kind.name()).cloned() else {
        bail!("Shell '{}' is not configured in the manifest", kind.name());
    };
    single.config.shells.clear();
    single.config.shells.insert(kind.name().to_owned(), target);
    Ok(single)
}

fn command_shell(command: &ShellCommand) -> CommandResult<()> {
    match command {
        ShellCommand::Plan { manifest, shell } => {
            let manifest = load_manifest(manifest)?;
            let plan = shell_only_plan(&manifest, shell)?;
            println!("{}", plan.render());
            Ok(())
        }
        ShellCommand::Apply { manifest, shell } => {
            let manifest = load_manifest(manifest)?;
            let plan = shell_only_plan(&manifest, shell)?;
            if plan.is_empty() {
                println!("Nothing to do for shell '{}'.", shell);
                return Ok(());
            }
            let mut executor = Executor::new(SwissCache::load().unwrap_or_default());
            executor.execute(&plan, None)?;
            println!("Shell integration applied for '{}'.", shell);
            Ok(())
        }
        ShellCommand::Print { manifest, shell } => {
            let manifest = load_manifest(manifest)?;
            let kind = ShellKind::from_name(shell)
                .ok_or_else(|| anyhow::anyhow!("Unknown shell '{}'", shell))?;
            print!("{}", shellgen::render_print(&manifest.config, kind)?);
            Ok(())
        }
    }
}

fn command_status() -> CommandResult<()> {
    let cache = SwissCache::load().unwrap_or_default();
    println!("swiss {}", VERSION);
    match cache.get(FINGERPRINT_KEY) {
        Some(fingerprint) => println!("last applied manifest: {}", fingerprint),
        None => println!("last applied manifest: none"),
    }

    let mut deps: Vec<_> = cache.deps().values().collect();
    deps.sort_by(|a, b| a.name().cmp(b.name()));
    if deps.is_empty() {
        println!("no dependencies recorded yet");
    } else {
        println!("{} dependenc(ies) recorded:", deps.len());
        for dep in deps {
            println!(
                "  {:<20} {:<10} {:<12} {}",
                dep.name(),
                dep.version().map(String::as_str).unwrap_or("-"),
                dep.status().name(),
                dep.dependency_type().name(),
            );
        }
    }
    println!("{} alias(es) recorded", cache.aliases().len());
    Ok(())
}

fn tool_available(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_doctor() -> CommandResult<()> {
    let mut checks: Vec<(&str, bool)> = vec![
        ("nu", tool_available("nu", &["--version"])),
        ("cargo", tool_available("cargo", &["--version"])),
        ("rustup", tool_available("rustup", &["--version"])),
        ("git", tool_available("git", &["--version"])),
    ];
    match Os::current() {
        Os::Linux => checks.push(("apt-get", tool_available("apt-get", &["--version"]))),
        Os::Macos => checks.push(("brew", tool_available("brew", &["--version"]))),
        Os::Windows => checks.push(("scoop", tool_available("scoop", &["--version"]))),
    }

    let mut all_ok = true;
    for (name, available) in checks {
        println!("{:<10} {}", name, if available { "ok" } else { "MISSING" });
        all_ok &= available;
    }
    if !all_ok {
        bail!("Some required tools are missing");
    }
    Ok(())
}

fn command_clean_cache() -> CommandResult<()> {
    if SwissCache::clean()? {
        println!("Cache deleted. The next apply will re-run every step.");
    } else {
        println!("No cache to delete.");
    }
    Ok(())
}

fn command_init_config(template: &str, output: &std::path::Path, force: bool) -> CommandResult<()> {
    let content = match template {
        "workstation" => embedded::WORKSTATION_YAML,
        "service-host" => embedded::SERVICE_HOST_YAML,
        "dev-shell" => embedded::DEV_SHELL_YAML,
        other => bail!(
            "Unknown template '{}'. Available: workstation, service-host, dev-shell",
            other
        ),
    };

    if output.exists() && !force {
        bail!(
            "Refusing to overwrite {}: pass --force to replace it",
            output.display()
        );
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(output, content)?;
    println!("Wrote {} template to {}", template, output.display());
    if template == "workstation" {
        println!(
            "Note: this template references file templates relative to the manifest \
             (see examples/templates in the Swiss repository)."
        );
    }
    Ok(())
}

fn main() {
    let cli = cli::Cli::parse();

    logger::init(cli.verbose);

    let command_result = match &cli.command {
        Command::Init {} => command_init(),
        Command::Plan { manifest } => command_plan(manifest),
        Command::Apply {
            manifest,
            yes,
            dry_run,
        } => command_apply(manifest, *yes, *dry_run, false),
        Command::Setup {
            manifest,
            yes,
            dry_run,
        } => command_apply(manifest, *yes, *dry_run, true),
        Command::Files { manifest, force } => command_files(manifest, *force),
        Command::Shell { command } => command_shell(command),
        Command::Status {} => command_status(),
        Command::Doctor {} => command_doctor(),
        Command::CleanCache {} => command_clean_cache(),
        Command::InitConfig {
            template,
            output,
            force,
        } => command_init_config(template, output, *force),
    };

    if let Err(err) = command_result {
        eprintln!("ERROR: {:#}", err);
        log::debug!("ERROR: {:?}", err);
        std::process::exit(1);
    }
}
