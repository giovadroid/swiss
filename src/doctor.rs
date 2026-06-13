use crate::config_loader::LoadedManifest;
use crate::parser::{CommandSpec, Os, SwissConfig};
use crate::plan::{build_plan, PlanContext};
use crate::shellgen::ShellKind;

const SUPPORTED_COMMAND_SHELLS: &[&str] = &[
    "nu",
    "nushell",
    "sh",
    "bash",
    "zsh",
    "pwsh",
    "powershell",
    "cmd",
];

/// One diagnostic finding about a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    Error(String),
    Warning(String),
}

impl Finding {
    pub fn is_error(&self) -> bool {
        matches!(self, Finding::Error(_))
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::Error(message) => write!(formatter, "error: {}", message),
            Finding::Warning(message) => write!(formatter, "warning: {}", message),
        }
    }
}

/// Validates a loaded manifest: builds the plan (which checks env
/// requirements, file templates and shell module references) and adds
/// static lint findings on top.
pub fn validate_manifest(manifest: &LoadedManifest, context: &PlanContext) -> Vec<Finding> {
    let mut findings = Vec::new();

    match build_plan(manifest, context) {
        Ok(plan) => {
            if plan.is_empty() {
                findings.push(Finding::Warning(
                    "the plan is empty: the manifest produces no steps on this OS".to_owned(),
                ));
            }
        }
        Err(error) => findings.push(Finding::Error(format!("{:#}", error))),
    }

    findings.extend(lint_config(&manifest.config, context.os));
    findings
}

fn lint_config(config: &SwissConfig, os: Os) -> Vec<Finding> {
    let mut findings = Vec::new();

    for (name, command) in all_commands(config) {
        let shell = command.shell();
        if !SUPPORTED_COMMAND_SHELLS.contains(&shell) {
            findings.push(Finding::Error(format!(
                "'{}' uses unsupported command shell '{}' (supported: {})",
                name,
                shell,
                SUPPORTED_COMMAND_SHELLS.join(", ")
            )));
        }
    }

    let uses_nu_commands = all_commands(config)
        .iter()
        .any(|(_, command)| matches!(command.shell(), "nu" | "nushell"));
    if uses_nu_commands && !config.manages_nushell() {
        findings.push(Finding::Warning(
            "manifest runs commands through Nushell but neither a 'nushell' section \
             nor an enabled nushell shell installs it; nu must already be on the host"
                .to_owned(),
        ));
    }

    for shell_name in config.shells.keys() {
        if ShellKind::from_name(shell_name).is_none() {
            findings.push(Finding::Error(format!(
                "unsupported shell '{}' in 'shells' section",
                shell_name
            )));
        }
    }

    for (module_name, module) in &config.shell_modules {
        if let Some(package) = &module.package {
            if !package_is_declared(config, package) {
                findings.push(Finding::Warning(format!(
                    "shell module '{}' references package '{}', but no dependency or \
                     package_manager section installs it",
                    module_name, package
                )));
            }
        }
        for init_shell in module.init.keys() {
            if ShellKind::from_name(init_shell).is_none() {
                findings.push(Finding::Warning(format!(
                    "shell module '{}' has an init entry for unknown shell '{}'",
                    module_name, init_shell
                )));
            }
        }
    }

    let has_pm_for_os = match os {
        Os::Linux => config.package_manager.linux.is_some(),
        Os::Macos => config.package_manager.macos.is_some(),
        Os::Windows => config.package_manager.windows.is_some(),
    };
    let has_pm_elsewhere = config.package_manager.linux.is_some()
        || config.package_manager.macos.is_some()
        || config.package_manager.windows.is_some();
    if !has_pm_for_os && has_pm_elsewhere {
        findings.push(Finding::Warning(format!(
            "manifest defines system packages, but none for the current OS ({})",
            os.name()
        )));
    }

    findings
}

/// Whether anything in the manifest installs `package`: a cargo dependency, a
/// custom dependency, or a system package on any OS.
fn package_is_declared(config: &SwissConfig, package: &str) -> bool {
    if config.dependencies.cargo.contains_key(package)
        || config.dependencies.customs.contains_key(package)
    {
        return true;
    }
    let backends = [
        config
            .package_manager
            .linux
            .as_ref()
            .and_then(|linux| linux.apt.as_ref()),
        config
            .package_manager
            .macos
            .as_ref()
            .and_then(|macos| macos.brew.as_ref()),
        config
            .package_manager
            .windows
            .as_ref()
            .and_then(|windows| windows.scoop.as_ref()),
    ];
    backends
        .into_iter()
        .flatten()
        .any(|backend| backend.packages.iter().any(|name| name == package))
}

/// Collects every command in the manifest together with a human label.
fn all_commands(config: &SwissConfig) -> Vec<(String, CommandSpec)> {
    let mut commands = Vec::new();

    for (name, dep_config) in &config.dependencies.cargo {
        if let Some(dep_config) = dep_config {
            for command in &dep_config.commands {
                commands.push((format!("cargo dependency '{}'", name), command.clone()));
            }
        }
    }

    for (name, dependency) in &config.dependencies.customs {
        let label = format!("custom dependency '{}'", name);
        for list in [&dependency.install, &dependency.update]
            .into_iter()
            .flatten()
        {
            for command in list {
                commands.push((label.clone(), command.clone()));
            }
        }
        if let Some(hooks) = &dependency.post_install {
            for list in [&hooks.linux, &hooks.macos, &hooks.windows]
                .into_iter()
                .flatten()
            {
                for command in list {
                    commands.push((label.clone(), command.clone()));
                }
            }
        }
    }

    commands
}

/// Installation checks: (tool, available, required). Non-required tools are
/// the ones `swiss apply` can bootstrap itself (rustup/cargo via the rust
/// installer, nu via the `nushell` section), so missing them on a fresh host
/// is expected rather than an error.
pub fn installation_checks(os: Os) -> Vec<(&'static str, bool, bool)> {
    let mut checks: Vec<(&'static str, bool, bool)> = vec![
        ("nu", tool_available("nu", &["--version"]), false),
        ("cargo", tool_available("cargo", &["--version"]), false),
        ("rustup", tool_available("rustup", &["--version"]), false),
        ("git", tool_available("git", &["--version"]), true),
    ];
    match os {
        Os::Linux => checks.push(("apt-get", tool_available("apt-get", &["--version"]), true)),
        Os::Macos => checks.push(("brew", tool_available("brew", &["--version"]), true)),
        Os::Windows => checks.push(("scoop", tool_available("scoop", &["--version"]), true)),
    }
    checks
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::SwissCache;
    use indoc::indoc;
    use std::path::PathBuf;

    fn manifest_from(yaml: &str) -> LoadedManifest {
        LoadedManifest {
            config: SwissConfig::parse(yaml).unwrap(),
            base_dir: PathBuf::from("/manifests"),
            fingerprint: "test".to_owned(),
        }
    }

    fn test_context() -> PlanContext {
        PlanContext {
            os: Os::Linux,
            home: PathBuf::from("/home/tester"),
            base_dir: PathBuf::from("/manifests"),
            cache: SwissCache::default(),
            nu_installed: true,
            nu_version: Some("0.101.0".to_owned()),
            cargo_installed: true,
            binstall_installed: true,
            available_shells: ["zsh", "bash", "pwsh"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }

    fn has_error(findings: &[Finding]) -> bool {
        findings.iter().any(Finding::is_error)
    }

    fn messages(findings: &[Finding]) -> Vec<String> {
        findings.iter().map(|finding| finding.to_string()).collect()
    }

    #[test]
    fn clean_manifest_has_no_errors() {
        let manifest = manifest_from(indoc! {"
            nushell:
              version: \"0.101.0\"
            dependencies:
              cargo:
                ripgrep:
        "});
        let findings = validate_manifest(&manifest, &test_context());
        assert!(!has_error(&findings), "findings: {:?}", findings);
    }

    #[test]
    fn unsupported_command_shell_is_an_error() {
        let manifest = manifest_from(indoc! {"
            dependencies:
              customs:
                tool:
                  install:
                    - run: make install
                      shell: fish
        "});
        let findings = validate_manifest(&manifest, &test_context());
        assert!(findings
            .iter()
            .any(|finding| finding.is_error() && finding.to_string().contains("fish")));
    }

    #[test]
    fn nu_commands_without_nushell_section_warn() {
        // Plain strings now run through the portable system shell, so an
        // explicit `shell: nu` is what pulls in the Nushell requirement.
        let manifest = manifest_from(indoc! {"
            dependencies:
              customs:
                tool:
                  install:
                    - run: ls | first
                      shell: nu
        "});
        let findings = validate_manifest(&manifest, &test_context());
        assert!(messages(&findings)
            .iter()
            .any(|message| message.contains("neither a 'nushell' section")));

        // A nushell shell enabled in `shells` also satisfies the requirement.
        let manifest = manifest_from(indoc! {"
            shells:
              nushell:
                mode: managed-loader
            dependencies:
              customs:
                tool:
                  install:
                    - run: ls | first
                      shell: nu
        "});
        let findings = validate_manifest(&manifest, &test_context());
        assert!(!messages(&findings)
            .iter()
            .any(|message| message.contains("neither a 'nushell' section")));
    }

    #[test]
    fn module_package_without_installer_warns() {
        let manifest = manifest_from(indoc! {"
            shells:
              zsh:
                modules: [starship]
            shell_modules:
              starship:
                package: starship
                init:
                  zsh: eval \"$(starship init zsh)\"
        "});
        let findings = validate_manifest(&manifest, &test_context());
        assert!(messages(&findings)
            .iter()
            .any(|message| message.contains("references package 'starship'")));

        // Declaring it as a cargo dependency silences the warning.
        let manifest = manifest_from(indoc! {"
            dependencies:
              cargo:
                starship:
            shells:
              zsh:
                modules: [starship]
            shell_modules:
              starship:
                package: starship
                init:
                  zsh: eval \"$(starship init zsh)\"
        "});
        let findings = validate_manifest(&manifest, &test_context());
        assert!(!messages(&findings)
            .iter()
            .any(|message| message.contains("references package")));
    }

    #[test]
    fn missing_pm_for_current_os_warns() {
        let manifest = manifest_from(indoc! {"
            package_manager:
              windows:
                scoop:
                  packages: [libssl-dev]
        "});
        let findings = validate_manifest(&manifest, &test_context());
        assert!(messages(&findings)
            .iter()
            .any(|message| message.contains("none for the current OS")));
    }

    #[test]
    fn broken_plan_is_reported_as_error() {
        let manifest = manifest_from(indoc! {"
            files:
              - dest: ~/.config/x
        "});
        let findings = validate_manifest(&manifest, &test_context());
        assert!(has_error(&findings));
    }
}
