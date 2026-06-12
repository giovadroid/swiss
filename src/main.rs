mod cli;
mod commands;
mod config_loader;
mod doctor;
mod embedded;
mod executor;
mod expand;
mod loader;
mod logger;
mod parser;
mod persistence;
mod plan;
mod shellgen;

use crate::cli::{Command, ManifestArgs};
use crate::commands::{CommandResult, NuShell};
use crate::config_loader::LoadedManifest;
use crate::executor::Executor;
use crate::parser::Os;
use crate::persistence::{
    SwissCache, FINGERPRINT_KEY, MANIFEST_PATH_KEY, MANIFEST_PROFILES_KEY, VERSION,
};
use crate::plan::{build_plan, Plan, PlanContext, Step};
use crate::shellgen::ShellKind;
use anyhow::{bail, Context};
use clap::Parser;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// `swiss init [--shell <name>]`: the per-shell startup hook.
///
/// - nushell (default): aggregates the dynamic loader files and prints the
///   Swiss environment as YAML for `load-env`.
/// - zsh/bash/pwsh: prints the generated init file so it can be sourced or
///   evaluated manually (`eval "$(swiss init --shell zsh)"`).
fn command_init(shell: Option<&str>) -> CommandResult<()> {
    let kind = match shell {
        None => ShellKind::Nushell,
        Some(name) => {
            ShellKind::from_name(name).ok_or_else(|| anyhow::anyhow!("Unknown shell '{}'", name))?
        }
    };

    if kind == ShellKind::Nushell {
        return command_init_nu();
    }

    let init_file = home::home_dir()
        .context("Unable to obtain home directory")?
        .join(".config/swiss")
        .join(kind.init_file_name().expect("non-nushell init file"));
    if init_file.exists() {
        print!("{}", std::fs::read_to_string(init_file)?);
    } else {
        log::debug!(
            "No generated init for {} yet; run `swiss setup` first",
            kind.name()
        );
    }
    Ok(())
}

fn command_init_nu() -> CommandResult<()> {
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

fn load_manifest(path: &Path, profiles: &[String]) -> CommandResult<LoadedManifest> {
    config_loader::load_manifest(path, profiles).context(
        "Could not load the bootstrap manifest. Pass one with --manifest <path>, \
         or generate a starter with `swiss setup --manifest bootstrap.yaml --template dev-shell`",
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

/// Stores the manifest path and profiles so `swiss update` can re-apply them.
fn register_manifest(path: &Path, profiles: &[String]) {
    let mut cache = SwissCache::load().unwrap_or_default();
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    cache.set(MANIFEST_PATH_KEY, &canonical.to_string_lossy());
    cache.set(MANIFEST_PROFILES_KEY, &profiles.join("\n"));
}

fn execute_plan(plan: &Plan, manifest: &LoadedManifest, yes: bool) -> CommandResult<bool> {
    if plan.is_empty() {
        println!("Nothing to do: everything is up to date.");
        return Ok(true);
    }
    if !confirm(plan, yes)? {
        println!("Aborted.");
        return Ok(false);
    }
    apply_env_defaults(manifest);
    let mut executor = Executor::new(SwissCache::load().unwrap_or_default());
    executor.execute(plan, Some(&manifest.fingerprint))?;
    println!("Done: {} step(s) applied.", plan.steps.len());
    Ok(true)
}

fn command_plan(args: &ManifestArgs) -> CommandResult<()> {
    let manifest = load_manifest(&args.manifest, &args.profiles)?;
    let context = plan_context(&manifest);
    let plan = build_plan(&manifest, &context)?;
    println!("{}", plan.render());
    Ok(())
}

fn command_apply(
    args: &ManifestArgs,
    yes: bool,
    dry_run: bool,
    force_files: bool,
) -> CommandResult<()> {
    let manifest = load_manifest(&args.manifest, &args.profiles)?;
    let context = plan_context(&manifest);
    let mut plan = build_plan(&manifest, &context)?;
    if force_files {
        plan = plan.force_file_overwrites();
    }

    if dry_run {
        println!("{}", plan.render());
        return Ok(());
    }

    if plan.requires_admin() && !executor::is_admin()? {
        log::warn!("Some steps require administrator rights and may fail.");
    }

    if execute_plan(&plan, &manifest, yes)? {
        register_manifest(&args.manifest, &args.profiles);
    }
    Ok(())
}

fn command_setup(
    args: &ManifestArgs,
    template: Option<&str>,
    shell: Option<&str>,
    yes: bool,
    dry_run: bool,
    force_files: bool,
) -> CommandResult<()> {
    materialize_template(&args.manifest, template)?;

    let manifest = load_manifest(&args.manifest, &args.profiles)?;
    let context = plan_context(&manifest);
    let mut plan = build_plan(&manifest, &context)?;
    if force_files {
        plan = plan.force_file_overwrites();
    }

    // Full integration: make sure the running/requested shell sources the
    // Swiss init even when the manifest does not configure it explicitly.
    let registration = shell_registration_steps(&manifest, &context, shell)?;
    plan.steps.extend(registration);

    if dry_run {
        println!("{}", plan.render());
        return Ok(());
    }

    if plan.requires_admin() && !executor::is_admin()? {
        bail!(
            "This plan contains steps that require administrator rights. \
             Re-run as administrator, or use `swiss apply` to proceed anyway."
        );
    }

    if execute_plan(&plan, &manifest, yes)? {
        register_manifest(&args.manifest, &args.profiles);
    }
    Ok(())
}

/// Writes the manifest from a built-in template when it does not exist yet.
fn materialize_template(path: &Path, template: Option<&str>) -> CommandResult<()> {
    let Some(template) = template else {
        if !path.exists() {
            bail!(
                "Manifest {} does not exist. Create it, or generate one with \
                 `swiss setup --manifest {} --template <workstation|service-host|dev-shell>`",
                path.display(),
                path.display()
            );
        }
        return Ok(());
    };

    if path.exists() {
        log::info!(
            "Manifest {} already exists; ignoring --template",
            path.display()
        );
        return Ok(());
    }

    let content = match template {
        "workstation" => embedded::WORKSTATION_YAML,
        "service-host" => embedded::SERVICE_HOST_YAML,
        "dev-shell" => embedded::DEV_SHELL_YAML,
        other => bail!(
            "Unknown template '{}'. Available: workstation, service-host, dev-shell",
            other
        ),
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)?;
    println!("Wrote {} template to {}", template, path.display());
    Ok(())
}

/// Detects the shell to register from $SHELL (zsh/bash); nushell when the
/// manifest manages it.
fn detect_shell(manifest: &LoadedManifest) -> Option<ShellKind> {
    if let Ok(shell_path) = std::env::var("SHELL") {
        if let Some(name) = Path::new(&shell_path).file_name().and_then(|n| n.to_str()) {
            if let Some(kind) = ShellKind::from_name(name) {
                return Some(kind);
            }
        }
    }
    if manifest.config.nushell.is_some() {
        return Some(ShellKind::Nushell);
    }
    None
}

/// Extra steps for `setup`: register the init hook for the running/requested
/// shell when the manifest's `shells` section does not already cover it.
fn shell_registration_steps(
    manifest: &LoadedManifest,
    context: &PlanContext,
    shell: Option<&str>,
) -> CommandResult<Vec<Step>> {
    let kind = match shell {
        Some(name) => Some(
            ShellKind::from_name(name)
                .ok_or_else(|| anyhow::anyhow!("Unknown shell '{}'", name))?,
        ),
        None => detect_shell(manifest),
    };
    let Some(kind) = kind else {
        log::debug!("No shell detected for registration; skipping");
        return Ok(Vec::new());
    };

    // If the manifest mentions the shell at all (even disabled), the manifest
    // is the source of truth and setup does not override it.
    if manifest.config.shells.contains_key(kind.name()) {
        return Ok(Vec::new());
    }

    log::info!(
        "Registering Swiss init for '{}' (not covered by the manifest)",
        kind.name()
    );

    if kind == ShellKind::Nushell {
        // Minimal managed loader so `swiss init` works at startup.
        let target = crate::parser::ShellTargetConfig::default();
        return shellgen::managed_loader_steps_for(&manifest.config, &target, context);
    }

    let init_file = kind.init_file_name().expect("non-nushell init file");
    let target_file = kind
        .default_target()
        .expect("non-nushell shells have a default target")
        .to_owned();
    Ok(vec![
        Step::WriteFile {
            path: context.home.join(".config/swiss").join(init_file),
            content: shellgen::init_file_content(&manifest.config, kind, &[])?.into_bytes(),
            overwrite: false,
            append_if_missing: false,
            backup: false,
            requires_admin: false,
        },
        Step::PatchBlock {
            target: target_file,
            block: shellgen::REGISTRATION_BLOCK.to_owned(),
            content: kind
                .registration_line()
                .expect("non-nushell registration line"),
        },
    ])
}

fn command_update(
    manifest_override: Option<&Path>,
    profiles_override: &[String],
    yes: bool,
    dry_run: bool,
) -> CommandResult<()> {
    let cache = SwissCache::load().unwrap_or_default();

    let (path, profiles): (PathBuf, Vec<String>) = match manifest_override {
        Some(path) => (path.to_path_buf(), profiles_override.to_vec()),
        None => {
            let path = cache.get(MANIFEST_PATH_KEY).ok_or_else(|| {
                anyhow::anyhow!(
                    "No manifest registered yet. Run `swiss setup --manifest <path>` first, \
                     or pass --manifest explicitly."
                )
            })?;
            let profiles = if profiles_override.is_empty() {
                cache
                    .get(MANIFEST_PROFILES_KEY)
                    .map(|raw| {
                        raw.split('\n')
                            .filter(|p| !p.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                profiles_override.to_vec()
            };
            (PathBuf::from(path), profiles)
        }
    };

    println!(
        "Updating from {}{}",
        path.display(),
        if profiles.is_empty() {
            String::new()
        } else {
            format!(" (profiles: {})", profiles.join(", "))
        }
    );

    let args = ManifestArgs {
        manifest: path,
        profiles,
    };
    command_apply(&args, yes, dry_run, false)
}

fn command_status() -> CommandResult<()> {
    let cache = SwissCache::load().unwrap_or_default();
    println!("swiss {}", VERSION);
    match cache.get(MANIFEST_PATH_KEY) {
        Some(path) => println!("registered manifest: {}", path),
        None => println!("registered manifest: none (run `swiss setup`)"),
    }
    if let Some(profiles) = cache.get(MANIFEST_PROFILES_KEY) {
        if !profiles.is_empty() {
            println!("registered profiles: {}", profiles.replace('\n', ", "));
        }
    }
    match cache.get(FINGERPRINT_KEY) {
        Some(fingerprint) => println!("last applied fingerprint: {}", fingerprint),
        None => println!("last applied fingerprint: none"),
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

fn command_doctor(manifest_path: Option<&Path>, profiles: &[String]) -> CommandResult<()> {
    println!("# installation");
    let mut all_ok = true;
    for (name, available) in doctor::installation_checks(Os::current()) {
        println!("{:<10} {}", name, if available { "ok" } else { "MISSING" });
        all_ok &= available;
    }

    if let Some(path) = manifest_path {
        println!("\n# manifest {}", path.display());
        let manifest = load_manifest(path, profiles)?;
        let context = plan_context(&manifest);
        let findings = doctor::validate_manifest(&manifest, &context);
        if findings.is_empty() {
            println!("manifest ok: no findings");
        } else {
            for finding in &findings {
                println!("{}", finding);
            }
        }
        all_ok &= !findings.iter().any(|finding| finding.is_error());
    }

    if !all_ok {
        bail!("Doctor found problems");
    }
    println!("\nAll good.");
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

fn main() {
    let cli = cli::Cli::parse();

    logger::init(cli.verbose);

    let command_result = match &cli.command {
        Command::Init { shell } => command_init(shell.as_deref()),
        Command::Plan { manifest } => command_plan(manifest),
        Command::Apply {
            manifest,
            yes,
            dry_run,
            force_files,
        } => command_apply(manifest, *yes, *dry_run, *force_files),
        Command::Setup {
            manifest,
            template,
            shell,
            yes,
            dry_run,
            force_files,
        } => command_setup(
            manifest,
            template.as_deref(),
            shell.as_deref(),
            *yes,
            *dry_run,
            *force_files,
        ),
        Command::Update {
            manifest,
            profiles,
            yes,
            dry_run,
        } => command_update(manifest.as_deref(), profiles, *yes, *dry_run),
        Command::Status {} => command_status(),
        Command::Doctor { manifest, profiles } => command_doctor(manifest.as_deref(), profiles),
        Command::CleanCache {} => command_clean_cache(),
    };

    if let Err(err) = command_result {
        eprintln!("ERROR: {:#}", err);
        log::debug!("ERROR: {:?}", err);
        std::process::exit(1);
    }
}
