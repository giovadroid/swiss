mod cli;
mod commands;
mod config_loader;
mod doctor;
mod embedded;
mod executor;
mod expand;
mod logger;
mod parser;
mod persistence;
mod plan;
mod report;
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
use std::io::Write;
use std::path::{Path, PathBuf};

/// `swiss init --shell <name>`: prints the complete init script for that
/// shell, rendered live from the registered manifest plus the dynamic state
/// (cached aliases, SWISS variables). Shells evaluate it at startup
/// (`eval "$(swiss init --shell zsh)"`); Nushell saves it to
/// `~/.config/swiss/init.nu` and sources it.
///
/// This command must never break shell startup: any problem degrades to a
/// minimal script and a stderr log, always exiting 0.
fn command_init(shell: Option<&str>) -> CommandResult<()> {
    let kind = match shell {
        Some(name) => {
            ShellKind::from_name(name).ok_or_else(|| anyhow::anyhow!("Unknown shell '{}'", name))?
        }
        None => detect_shell_from_env().ok_or_else(|| {
            anyhow::anyhow!("Could not detect the shell; pass --shell <nushell|zsh|bash|pwsh>")
        })?,
    };

    let cache = SwissCache::load().unwrap_or_default();
    let config = registered_config(&cache);
    let modules = config
        .shells
        .get(kind.name())
        .filter(|target| target.enabled)
        .map(|target| target.modules.clone())
        .unwrap_or_default();
    let aliases: std::collections::BTreeMap<String, String> = cache
        .aliases()
        .iter()
        .map(|(alias, command)| (alias.clone(), command.clone()))
        .collect();

    match shellgen::render_init_script(&config, kind, &modules, &aliases) {
        Ok(mut script) => {
            if kind == ShellKind::Nushell {
                if let Some(home) = home::home_dir() {
                    script.push_str(&nu_user_dropins(&home));
                }
            }
            print!("{}", script);
        }
        Err(error) => log::warn!("swiss init degraded to an empty script: {:#}", error),
    }
    Ok(())
}

/// The manifest registered by setup/apply, reloaded fresh so module changes
/// show up at the next shell start without re-applying. Missing or broken
/// state degrades to an empty config.
fn registered_config(cache: &SwissCache) -> crate::parser::SwissConfig {
    let Some(path) = cache.get(MANIFEST_PATH_KEY) else {
        return crate::parser::SwissConfig::default();
    };
    let profiles: Vec<String> = cache
        .get(MANIFEST_PROFILES_KEY)
        .map(|raw| {
            raw.split('\n')
                .filter(|profile| !profile.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    match config_loader::load_manifest(Path::new(path), &profiles) {
        Ok(manifest) => manifest.config,
        Err(error) => {
            log::warn!("Ignoring registered manifest {}: {:#}", path, error);
            crate::parser::SwissConfig::default()
        }
    }
}

/// User drop-in modules for Nushell: every `*.nu` under `~/.swiss/env` then
/// `~/.swiss/conf` is appended verbatim to the generated init script.
fn nu_user_dropins(home: &Path) -> String {
    let mut output = String::new();
    for dir in [home.join(".swiss/env"), home.join(".swiss/conf")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().map(|ext| ext == "nu").unwrap_or(false))
            .collect();
        files.sort();
        for file in files {
            match std::fs::read_to_string(&file) {
                Ok(body) => {
                    output.push('\n');
                    output.push_str(&format!(
                        "# swiss user module: {}\n{}\n",
                        file.display(),
                        body.trim_end()
                    ));
                }
                Err(error) => log::warn!("Skipping user module {}: {}", file.display(), error),
            }
        }
    }
    output
}

fn load_manifest(path: &Path, profiles: &[String]) -> CommandResult<LoadedManifest> {
    config_loader::load_manifest(path, profiles).context(
        "Could not load the bootstrap manifest. Pass one with --manifest <path>, \
         or generate a starter with `swiss setup --manifest bootstrap.yaml --template dev-shell`",
    )
}

fn tool_on_path(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn cargo_bin_exists(home: &Path, name: &str) -> bool {
    let binary = if cfg!(target_os = "windows") {
        format!("{}.exe", name)
    } else {
        name.to_owned()
    };
    home.join(".cargo/bin").join(binary).exists()
}

fn plan_context(manifest: &LoadedManifest) -> PlanContext {
    let nu_installed = NuShell::is_installed();
    let nu_version = if nu_installed {
        NuShell::version().ok()
    } else {
        None
    };
    let home = home::home_dir().expect("Unable to obtain home directory");

    // Only probe the shells the manifest actually mentions.
    let available_shells = manifest
        .config
        .shells
        .keys()
        .filter_map(|name| ShellKind::from_name(name))
        .filter(|kind| *kind != ShellKind::Nushell)
        .filter(|kind| tool_on_path(kind.name()))
        .map(|kind| kind.name().to_owned())
        .collect();

    PlanContext {
        cargo_installed: cargo_bin_exists(&home, "cargo") || tool_on_path("cargo"),
        binstall_installed: cargo_bin_exists(&home, "cargo-binstall")
            || tool_on_path("cargo-binstall"),
        available_shells,
        os: Os::current(),
        home,
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

/// Runs the plan. Returns `None` when the user aborted at the confirmation
/// prompt, or `Some(outcome)` describing what ran (possibly with failures).
fn execute_plan(
    plan: &Plan,
    manifest: &LoadedManifest,
    yes: bool,
) -> CommandResult<Option<report::Outcome>> {
    if plan.is_empty() {
        println!("Nothing to do: everything is up to date.");
        return Ok(Some(report::Outcome::default()));
    }
    if !confirm(plan, yes)? {
        println!("Aborted.");
        return Ok(None);
    }
    apply_env_defaults(manifest);
    report::open_run_log();
    let mut executor = Executor::new(SwissCache::load().unwrap_or_default());
    let outcome = executor.execute(plan, Some(&manifest.fingerprint));
    report::print_summary(&outcome);
    Ok(Some(outcome))
}

/// Bails when a run had failures, pointing at the log; otherwise returns Ok so
/// the caller can register the manifest. A clean or aborted run is a no-op.
fn finish_run(outcome: &report::Outcome) -> CommandResult<()> {
    if outcome.succeeded() {
        return Ok(());
    }
    let log_hint = outcome
        .log_path
        .as_ref()
        .map(|path| format!(" See the full log at {}.", path.display()))
        .unwrap_or_default();
    bail!(
        "{} step(s) failed; the rest were applied.{} Re-run to retry.",
        outcome.failed,
        log_hint
    )
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

    if let Some(outcome) = execute_plan(&plan, &manifest, yes)? {
        register_manifest(&args.manifest, &args.profiles);
        finish_run(&outcome)?;
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

    if let Some(outcome) = execute_plan(&plan, &manifest, yes)? {
        register_manifest(&args.manifest, &args.profiles);
        finish_run(&outcome)?;
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

/// Detects the running shell from $SHELL (zsh/bash/nu...).
fn detect_shell_from_env() -> Option<ShellKind> {
    let shell_path = std::env::var("SHELL").ok()?;
    let name = Path::new(&shell_path).file_name()?.to_str()?;
    ShellKind::from_name(name)
}

/// Shell to register during setup: the running one, or nushell when the
/// manifest manages it.
fn detect_shell(manifest: &LoadedManifest) -> Option<ShellKind> {
    detect_shell_from_env().or_else(|| {
        manifest
            .config
            .manages_nushell()
            .then_some(ShellKind::Nushell)
    })
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
        return Ok(shellgen::nushell_registration_steps(context));
    }

    let target_file = kind
        .default_target()
        .expect("non-nushell shells have a default target")
        .to_owned();
    Ok(vec![Step::PatchBlock {
        target: target_file,
        block: shellgen::REGISTRATION_BLOCK.to_owned(),
        content: kind
            .registration_line()
            .expect("non-nushell registration line"),
    }])
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
    for (name, available, required) in doctor::installation_checks(Os::current()) {
        let status = if available {
            "ok"
        } else if required {
            "MISSING"
        } else {
            "missing (bootstrapped by `swiss apply` when the manifest needs it)"
        };
        println!("{:<10} {}", name, status);
        all_ok &= available || !required;
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
