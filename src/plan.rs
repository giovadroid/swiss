use crate::config_loader::LoadedManifest;
use crate::expand::expand_path;
use crate::parser::{
    CommandSpec, FileSpec, GitConfig, Os, PackageManagerBackendConfig, SwissConfig,
};
use crate::persistence::{Dependency, DependencyStatus, DependencyType, SwissCache, NUSHELL_DEP};
use crate::shellgen;
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

pub const CUSTOM_MODULES_SUBDIR: &str = ".config/swiss/crates";

/// Everything the planner needs to know about the host. Building this is the
/// only place allowed to probe the system, so plan generation stays pure and
/// unit-testable.
#[derive(Debug, Clone, Default)]
pub struct PlanContext {
    pub os: Os,
    pub home: PathBuf,
    pub base_dir: PathBuf,
    pub cache: SwissCache,
    pub nu_installed: bool,
    pub nu_version: Option<String>,
}

impl Default for Os {
    fn default() -> Self {
        Os::current()
    }
}

/// One atomic action the executor knows how to perform.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Command {
        program: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        requires_admin: bool,
    },
    ShellCommand {
        shell: String,
        command: String,
        cwd: Option<PathBuf>,
        requires_admin: bool,
    },
    EnsureDir {
        path: PathBuf,
    },
    WriteFile {
        path: PathBuf,
        content: Vec<u8>,
        overwrite: bool,
        append_if_missing: bool,
        backup: bool,
    },
    GitSync {
        repo: String,
        branch: Option<String>,
        depth: Option<u32>,
        recursive: bool,
        dest: PathBuf,
    },
    /// Replace or append a bounded `# swiss begin/end` block in a startup file.
    PatchBlock {
        /// Unexpanded target (`~/.zshrc`, `$PROFILE`); resolved at execution.
        target: String,
        block: String,
        content: String,
    },
    /// Patch `$nu.env-path` / `$nu.config-path` with the Swiss source lines.
    ConfigureNu,
    /// Symlink nu binaries from `~/.cargo/bin` into `/usr/local/bin`.
    LinkNuBinaries,
    RecordDep {
        dep: Dependency,
    },
    RecordAliases {
        aliases: BTreeMap<String, String>,
    },
}

impl Step {
    pub fn requires_admin(&self) -> bool {
        match self {
            Step::Command { requires_admin, .. } => *requires_admin,
            Step::ShellCommand { requires_admin, .. } => *requires_admin,
            Step::LinkNuBinaries => cfg!(not(target_os = "windows")),
            _ => false,
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Command {
                program,
                args,
                cwd,
                requires_admin,
            } => {
                write!(formatter, "run: {} {}", program, args.join(" "))?;
                if let Some(cwd) = cwd {
                    write!(formatter, " (in {})", cwd.display())?;
                }
                if *requires_admin {
                    write!(formatter, " [admin]")?;
                }
                Ok(())
            }
            Step::ShellCommand {
                shell,
                command,
                cwd,
                requires_admin,
            } => {
                write!(formatter, "run ({}): {}", shell, command)?;
                if let Some(cwd) = cwd {
                    write!(formatter, " (in {})", cwd.display())?;
                }
                if *requires_admin {
                    write!(formatter, " [admin]")?;
                }
                Ok(())
            }
            Step::EnsureDir { path } => write!(formatter, "ensure dir: {}", path.display()),
            Step::WriteFile {
                path,
                content,
                overwrite,
                append_if_missing,
                ..
            } => {
                let mode = if *append_if_missing {
                    "append-if-missing"
                } else if *overwrite {
                    "overwrite"
                } else {
                    "create-if-missing"
                };
                write!(
                    formatter,
                    "write file: {} ({}, {} bytes)",
                    path.display(),
                    mode,
                    content.len()
                )
            }
            Step::GitSync { repo, dest, .. } => {
                write!(formatter, "git sync: {} -> {}", repo, dest.display())
            }
            Step::PatchBlock { target, block, .. } => {
                write!(formatter, "patch block '{}' in {}", block, target)
            }
            Step::ConfigureNu => write!(
                formatter,
                "patch nushell env/config files with Swiss loaders"
            ),
            Step::LinkNuBinaries => write!(
                formatter,
                "link nu binaries from ~/.cargo/bin to /usr/local/bin [admin]"
            ),
            Step::RecordDep { dep } => write!(formatter, "record dependency state: {}", dep.name()),
            Step::RecordAliases { aliases } => {
                write!(formatter, "record {} alias(es) in cache", aliases.len())
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub steps: Vec<Step>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn requires_admin(&self) -> bool {
        self.steps.iter().any(Step::requires_admin)
    }

    pub fn render(&self) -> String {
        if self.steps.is_empty() {
            return "Nothing to do: the plan is empty.".to_owned();
        }
        let mut output = String::new();
        for (index, step) in self.steps.iter().enumerate() {
            output.push_str(&format!("{:>3}. {}\n", index + 1, step));
        }
        output
    }

    /// `--force-files`: rewrite declared/generated files even if they exist.
    pub fn force_file_overwrites(self) -> Plan {
        let steps = self
            .steps
            .into_iter()
            .map(|step| match step {
                Step::WriteFile {
                    path,
                    content,
                    append_if_missing,
                    backup,
                    ..
                } => Step::WriteFile {
                    path,
                    content,
                    overwrite: true,
                    append_if_missing,
                    backup,
                },
                other => other,
            })
            .collect();
        Plan { steps }
    }
}

/// Builds the full execution plan from a loaded manifest. Pure: reads only
/// `files[].source` templates, never mutates the system.
pub fn build_plan(manifest: &LoadedManifest, context: &PlanContext) -> Result<Plan> {
    let config = &manifest.config;
    let mut steps = Vec::new();

    validate_env_requirements(config)?;

    steps.extend(package_manager_steps(config, context.os));
    steps.extend(rust_component_steps(config));
    steps.extend(nushell_steps(config, context));
    steps.extend(cargo_steps(config, context));
    steps.extend(custom_steps(config, context)?);
    steps.extend(file_steps(config, context)?);
    steps.extend(shellgen::shell_steps(config, context)?);

    Ok(Plan { steps })
}

fn validate_env_requirements(config: &SwissConfig) -> Result<()> {
    for (name, requirement) in &config.env {
        let source = requirement.from_env.as_deref().unwrap_or(name);
        if requirement.required && requirement.default.is_none() && std::env::var(source).is_err() {
            bail!(
                "Required environment variable '{}' (for '{}') is not set",
                source,
                name
            );
        }
    }
    Ok(())
}

pub(crate) fn package_manager_steps(config: &SwissConfig, os: Os) -> Vec<Step> {
    let mut steps = Vec::new();

    let backend: Option<(&PackageManagerBackendConfig, bool)> = match os {
        Os::Linux => config
            .package_manager
            .linux
            .as_ref()
            .and_then(|linux| linux.apt.as_ref())
            .map(|apt| (apt, true)),
        Os::Macos => config
            .package_manager
            .macos
            .as_ref()
            .and_then(|macos| macos.brew.as_ref())
            .map(|brew| (brew, false)),
        Os::Windows => config
            .package_manager
            .windows
            .as_ref()
            .and_then(|windows| windows.scoop.as_ref())
            .map(|scoop| (scoop, false)),
    };

    let Some((backend, is_apt)) = backend else {
        return steps;
    };
    if backend.packages.is_empty() {
        return steps;
    }

    match os {
        Os::Linux => {
            if backend.update_index {
                steps.push(Step::Command {
                    program: "sudo".to_owned(),
                    args: vec!["apt-get".to_owned(), "update".to_owned()],
                    cwd: None,
                    requires_admin: true,
                });
            }
            let mut args = vec!["apt-get".to_owned(), "install".to_owned(), "-y".to_owned()];
            args.extend(backend.packages.iter().cloned());
            steps.push(Step::Command {
                program: "sudo".to_owned(),
                args,
                cwd: None,
                requires_admin: is_apt,
            });
        }
        Os::Macos => {
            let mut args = vec!["install".to_owned()];
            args.extend(backend.packages.iter().cloned());
            steps.push(Step::Command {
                program: "brew".to_owned(),
                args,
                cwd: None,
                requires_admin: false,
            });
        }
        Os::Windows => {
            let mut args = vec!["install".to_owned()];
            args.extend(backend.packages.iter().cloned());
            steps.push(Step::Command {
                program: "scoop".to_owned(),
                args,
                cwd: None,
                requires_admin: false,
            });
        }
    }

    steps
}

pub(crate) fn rust_component_steps(config: &SwissConfig) -> Vec<Step> {
    config
        .rust
        .components
        .iter()
        .map(|component| Step::Command {
            program: "rustup".to_owned(),
            args: vec!["component".to_owned(), "add".to_owned(), component.clone()],
            cwd: None,
            requires_admin: false,
        })
        .collect()
}

fn cargo_bin(context: &PlanContext) -> String {
    context
        .home
        .join(".cargo/bin/cargo")
        .to_string_lossy()
        .to_string()
}

fn nushell_steps(config: &SwissConfig, context: &PlanContext) -> Vec<Step> {
    let Some(nushell) = &config.nushell else {
        return Vec::new();
    };

    let up_to_date = context.nu_installed
        && context
            .nu_version
            .as_ref()
            .map(|version| version.contains(nushell.version.as_str()))
            .unwrap_or(false);
    if up_to_date {
        return Vec::new();
    }

    vec![
        Step::Command {
            program: cargo_bin(context),
            args: vec![
                "install".to_owned(),
                format!("nu@{}", nushell.version),
                "--all-features".to_owned(),
            ],
            cwd: None,
            requires_admin: false,
        },
        Step::LinkNuBinaries,
        Step::RecordDep {
            dep: Dependency::new(
                NUSHELL_DEP.to_owned(),
                Some(nushell.version.clone()),
                DependencyStatus::Installed,
                DependencyType::Cargo,
            ),
        },
    ]
}

fn command_steps(commands: &[CommandSpec], cwd: Option<PathBuf>) -> Vec<Step> {
    commands
        .iter()
        .filter(|command| !command.is_empty())
        .map(|command| Step::ShellCommand {
            shell: command.shell().to_owned(),
            command: command.run().to_owned(),
            cwd: cwd.clone(),
            requires_admin: command.requires_admin(),
        })
        .collect()
}

fn cargo_steps(config: &SwissConfig, context: &PlanContext) -> Vec<Step> {
    let mut steps = Vec::new();
    let cargo = cargo_bin(context);

    // cargo-binstall bootstraps the rest, so it always goes first.
    let mut names: Vec<&String> = config.dependencies.cargo.keys().collect();
    names.sort_by_key(|name| (name.as_str() != "cargo-binstall", name.to_owned()));

    for name in names {
        let dep_config = config.dependencies.cargo[name].clone().unwrap_or_default();

        if !dep_config.is_installable(context.os) {
            log::debug!("{} is not installable on {}", name, context.os.name());
            continue;
        }

        if !dep_config.is_generic_version() {
            if let Some(installed) = context.cache.get_dep(name) {
                if installed
                    .version()
                    .map(|version| *version == dep_config.version)
                    .unwrap_or(false)
                {
                    continue;
                }
            }
        }

        let mut args = vec![
            "binstall".to_owned(),
            "-y".to_owned(),
            dep_config.package_name(name),
        ];
        args.extend(dep_config.args.iter().cloned());
        steps.push(Step::Command {
            program: cargo.clone(),
            args,
            cwd: None,
            requires_admin: false,
        });

        steps.extend(command_steps(&dep_config.commands, None));

        steps.push(Step::RecordDep {
            dep: Dependency::new(
                name.clone(),
                Some(dep_config.version.clone()),
                DependencyStatus::UpToDate,
                DependencyType::Cargo,
            ),
        });
        if !dep_config.alias.is_empty() {
            steps.push(Step::RecordAliases {
                aliases: dep_config.alias.clone(),
            });
        }
    }

    steps
}

fn custom_steps(config: &SwissConfig, context: &PlanContext) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    if config.dependencies.customs.is_empty() {
        return Ok(steps);
    }

    let modules_root = context.home.join(CUSTOM_MODULES_SUBDIR);
    steps.push(Step::EnsureDir {
        path: modules_root.clone(),
    });

    for (name, dependency) in &config.dependencies.customs {
        if !dependency.is_installable(context.os) {
            log::debug!("{} is not installable on {}", name, context.os.name());
            continue;
        }

        let module_dir = modules_root.join(name);
        let installed = context.cache.get_dep(name).is_some();

        if let Some(git) = &dependency.git {
            steps.push(Step::GitSync {
                repo: git.repo.clone(),
                branch: git.branch.clone(),
                depth: git.depth,
                recursive: git.recursive,
                dest: module_dir.clone(),
            });
        } else {
            steps.push(Step::EnsureDir {
                path: module_dir.clone(),
            });
        }

        let commands = if installed {
            dependency.update.as_ref().or(dependency.install.as_ref())
        } else {
            dependency.install.as_ref()
        };
        if let Some(commands) = commands {
            steps.extend(command_steps(commands, Some(module_dir.clone())));
        }

        if !installed {
            if let Some(post_install) = dependency
                .post_install
                .as_ref()
                .and_then(|hooks| hooks.for_os(context.os))
            {
                steps.extend(command_steps(post_install, Some(module_dir.clone())));
            }
        }

        steps.push(Step::RecordDep {
            dep: Dependency::new(
                name.clone(),
                None,
                DependencyStatus::UpToDate,
                DependencyType::Custom,
            ),
        });
    }

    Ok(steps)
}

fn file_steps(config: &SwissConfig, context: &PlanContext) -> Result<Vec<Step>> {
    let mut steps = Vec::new();

    for spec in &config.files {
        if !spec.applies_to(context.os) {
            continue;
        }
        let content = file_content(spec, context)?;
        let dest = expand_path(&spec.dest, &context.home);
        steps.push(Step::WriteFile {
            path: dest,
            content,
            overwrite: spec.overwrite,
            append_if_missing: spec.append_if_missing,
            backup: spec.backup,
        });
    }

    Ok(steps)
}

fn file_content(spec: &FileSpec, context: &PlanContext) -> Result<Vec<u8>> {
    match (&spec.source, &spec.content) {
        (Some(_), Some(_)) => bail!(
            "File entry for '{}' defines both 'source' and 'content'",
            spec.dest
        ),
        (None, None) => bail!(
            "File entry for '{}' needs either 'source' or 'content'",
            spec.dest
        ),
        (Some(source), None) => {
            let path = context.base_dir.join(source);
            std::fs::read(&path).map_err(|error| {
                anyhow::anyhow!("Failed to read file template {}: {}", path.display(), error)
            })
        }
        (None, Some(content)) => Ok(content.clone().into_bytes()),
    }
}

pub(crate) fn git_clone_args(git: &GitConfig, dest: &std::path::Path) -> Vec<String> {
    let mut args = vec!["clone".to_owned()];
    if let Some(depth) = git.depth {
        args.push("--depth".to_owned());
        args.push(depth.to_string());
    }
    if let Some(branch) = &git.branch {
        args.push("--branch".to_owned());
        args.push(branch.clone());
    }
    if git.recursive {
        args.push("--recursive".to_owned());
    }
    args.push(git.repo.clone());
    args.push(dest.to_string_lossy().to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{
        CargoCustomConfig, CustomDependency, LinuxPackageManagerConfig, MacosPackageManagerConfig,
        PackageManagerBackendConfig, PackageManagerConfig, RustConfig, WindowsPackageManagerConfig,
    };
    use crate::persistence::{Dependency, DependencyStatus, DependencyType};
    use indoc::indoc;
    use std::path::Path;

    fn manifest_from(yaml: &str, base_dir: &Path) -> LoadedManifest {
        LoadedManifest {
            config: SwissConfig::parse(yaml).unwrap(),
            base_dir: base_dir.to_path_buf(),
            fingerprint: "test".to_owned(),
        }
    }

    fn test_context(os: Os) -> PlanContext {
        PlanContext {
            os,
            home: PathBuf::from("/home/tester"),
            base_dir: PathBuf::from("/manifests"),
            cache: SwissCache::default(),
            nu_installed: false,
            nu_version: None,
        }
    }

    #[test]
    fn git_clone_args_include_optional_checkout_flags_before_repo() {
        let git = GitConfig {
            repo: "https://example.invalid/tool.git".to_owned(),
            branch: Some("main".to_owned()),
            depth: Some(1),
            recursive: true,
        };

        let args = git_clone_args(&git, Path::new("/tmp/swiss/tool"));

        assert_eq!(
            args,
            vec![
                "clone",
                "--depth",
                "1",
                "--branch",
                "main",
                "--recursive",
                "https://example.invalid/tool.git",
                "/tmp/swiss/tool",
            ]
        );
    }

    #[test]
    fn rust_component_steps_preserve_manifest_order() {
        let config = SwissConfig {
            rust: RustConfig {
                components: vec!["rust-analyzer".to_owned(), "clippy".to_owned()],
                ..RustConfig::default()
            },
            ..SwissConfig::default()
        };

        let steps = rust_component_steps(&config);
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0],
            Step::Command {
                program: "rustup".to_owned(),
                args: vec![
                    "component".to_owned(),
                    "add".to_owned(),
                    "rust-analyzer".to_owned()
                ],
                cwd: None,
                requires_admin: false,
            }
        );
    }

    #[test]
    fn package_manager_steps_empty_without_packages() {
        for os in [Os::Linux, Os::Macos, Os::Windows] {
            assert!(package_manager_steps(&SwissConfig::default(), os).is_empty());
        }
    }

    fn package_manager_config() -> SwissConfig {
        SwissConfig {
            package_manager: PackageManagerConfig {
                linux: Some(LinuxPackageManagerConfig {
                    apt: Some(PackageManagerBackendConfig {
                        packages: vec!["build-essential".to_owned(), "pkg-config".to_owned()],
                        ..PackageManagerBackendConfig::default()
                    }),
                }),
                macos: Some(MacosPackageManagerConfig {
                    brew: Some(PackageManagerBackendConfig {
                        packages: vec!["cmake".to_owned()],
                        ..PackageManagerBackendConfig::default()
                    }),
                }),
                windows: Some(WindowsPackageManagerConfig {
                    scoop: Some(PackageManagerBackendConfig {
                        packages: vec!["libssl-dev".to_owned()],
                        ..PackageManagerBackendConfig::default()
                    }),
                }),
            },
            ..SwissConfig::default()
        }
    }

    #[test]
    fn package_manager_steps_build_per_os_commands() {
        let config = package_manager_config();

        assert_eq!(
            package_manager_steps(&config, Os::Linux),
            vec![Step::Command {
                program: "sudo".to_owned(),
                args: vec![
                    "apt-get".to_owned(),
                    "install".to_owned(),
                    "-y".to_owned(),
                    "build-essential".to_owned(),
                    "pkg-config".to_owned(),
                ],
                cwd: None,
                requires_admin: true,
            }]
        );

        assert_eq!(
            package_manager_steps(&config, Os::Macos),
            vec![Step::Command {
                program: "brew".to_owned(),
                args: vec!["install".to_owned(), "cmake".to_owned()],
                cwd: None,
                requires_admin: false,
            }]
        );

        assert_eq!(
            package_manager_steps(&config, Os::Windows),
            vec![Step::Command {
                program: "scoop".to_owned(),
                args: vec!["install".to_owned(), "libssl-dev".to_owned()],
                cwd: None,
                requires_admin: false,
            }]
        );
    }

    #[test]
    fn apt_update_index_adds_update_step_first() {
        let mut config = package_manager_config();
        config
            .package_manager
            .linux
            .as_mut()
            .unwrap()
            .apt
            .as_mut()
            .unwrap()
            .update_index = true;

        let steps = package_manager_steps(&config, Os::Linux);
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0],
            Step::Command {
                program: "sudo".to_owned(),
                args: vec!["apt-get".to_owned(), "update".to_owned()],
                cwd: None,
                requires_admin: true,
            }
        );
    }

    #[test]
    fn custom_dependency_uses_install_then_update() {
        let yaml = indoc! {r#"
            dependencies:
              customs:
                tool:
                  install: ["echo install"]
                  update: ["echo update"]
        "#};
        let manifest = manifest_from(yaml, Path::new("/manifests"));

        // First run: not in cache -> install.
        let context = test_context(Os::Linux);
        let plan = build_plan(&manifest, &context).unwrap();
        assert!(plan.steps.iter().any(|step| matches!(
            step,
            Step::ShellCommand { command, .. } if command == "echo install"
        )));

        // Second run: cached -> update.
        let mut cached = test_context(Os::Linux);
        cached.cache.set_dep_in_memory(
            "tool",
            &Dependency::new(
                "tool".to_owned(),
                None,
                DependencyStatus::UpToDate,
                DependencyType::Custom,
            ),
        );
        let plan = build_plan(&manifest, &cached).unwrap();
        assert!(plan.steps.iter().any(|step| matches!(
            step,
            Step::ShellCommand { command, .. } if command == "echo update"
        )));
        assert!(!plan.steps.iter().any(|step| matches!(
            step,
            Step::ShellCommand { command, .. } if command == "echo install"
        )));
    }

    #[test]
    fn custom_dependency_skips_non_current_os() {
        let config = SwissConfig {
            dependencies: crate::parser::Dependencies {
                customs: [(
                    "winonly".to_owned(),
                    CustomDependency {
                        windows: Some(true),
                        linux: Some(false),
                        macos: Some(false),
                        install: Some(vec![CommandSpec::Plain("echo hi".to_owned())]),
                        ..CustomDependency::default()
                    },
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
            ..SwissConfig::default()
        };
        let context = test_context(Os::Linux);

        let steps = custom_steps(&config, &context).unwrap();
        // Only the modules root dir, no install/record steps.
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], Step::EnsureDir { .. }));
    }

    #[test]
    fn cargo_steps_skip_cached_pinned_versions() {
        let mut config = SwissConfig::default();
        config.dependencies.cargo.insert(
            "tool".to_owned(),
            Some(CargoCustomConfig {
                version: "1.2.3".to_owned(),
                ..CargoCustomConfig::default()
            }),
        );

        let mut context = test_context(Os::Linux);
        context.cache.set_dep_in_memory(
            "tool",
            &Dependency::new(
                "tool".to_owned(),
                Some("1.2.3".to_owned()),
                DependencyStatus::UpToDate,
                DependencyType::Cargo,
            ),
        );

        assert!(cargo_steps(&config, &context).is_empty());

        // Different cached version -> reinstall with name@version.
        let mut outdated = test_context(Os::Linux);
        outdated.cache.set_dep_in_memory(
            "tool",
            &Dependency::new(
                "tool".to_owned(),
                Some("1.0.0".to_owned()),
                DependencyStatus::UpToDate,
                DependencyType::Cargo,
            ),
        );
        let steps = cargo_steps(&config, &outdated);
        assert!(steps.iter().any(|step| matches!(
            step,
            Step::Command { args, .. } if args.contains(&"tool@1.2.3".to_owned())
        )));
    }

    #[test]
    fn cargo_binstall_is_planned_first() {
        let mut config = SwissConfig::default();
        config.dependencies.cargo.insert("bat".to_owned(), None);
        config
            .dependencies
            .cargo
            .insert("cargo-binstall".to_owned(), None);

        let context = test_context(Os::Linux);
        let steps = cargo_steps(&config, &context);
        match &steps[0] {
            Step::Command { args, .. } => assert!(args.contains(&"cargo-binstall".to_owned())),
            other => panic!("expected command, got {:?}", other),
        }
    }

    #[test]
    fn nushell_steps_skip_when_version_matches() {
        let config = SwissConfig::parse("nushell:\n  version: \"0.101.0\"").unwrap();

        let mut context = test_context(Os::Linux);
        context.nu_installed = true;
        context.nu_version = Some("0.101.0".to_owned());
        assert!(nushell_steps(&config, &context).is_empty());

        context.nu_version = Some("0.99.0".to_owned());
        let steps = nushell_steps(&config, &context);
        assert!(matches!(steps[0], Step::Command { .. }));
        assert!(steps.contains(&Step::LinkNuBinaries));
    }

    #[test]
    fn files_section_produces_write_steps_with_modes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("starship.toml"), "format = \"$all\"").unwrap();

        let yaml = indoc! {r#"
            files:
              - source: ./starship.toml
                dest: ~/.config/starship.toml
                overwrite: false
              - content: |
                  source ~/.config/swiss/env.nu
                dest: ~/.config/nushell/env.nu
                append_if_missing: true
        "#};
        let manifest = manifest_from(yaml, dir.path());
        let mut context = test_context(Os::Linux);
        context.base_dir = dir.path().to_path_buf();

        let plan = build_plan(&manifest, &context).unwrap();
        let writes: Vec<&Step> = plan
            .steps
            .iter()
            .filter(|step| matches!(step, Step::WriteFile { .. }))
            .collect();
        assert_eq!(writes.len(), 2);
        match writes[0] {
            Step::WriteFile { path, content, .. } => {
                assert_eq!(path, &PathBuf::from("/home/tester/.config/starship.toml"));
                assert_eq!(content, b"format = \"$all\"");
            }
            _ => unreachable!(),
        }
        match writes[1] {
            Step::WriteFile {
                append_if_missing, ..
            } => assert!(*append_if_missing),
            _ => unreachable!(),
        }
    }

    #[test]
    fn missing_required_env_fails_plan() {
        let yaml = indoc! {"
            env:
              GITHUB_TOKEN:
                from_env: SWISS_TEST_SURELY_UNSET_TOKEN
                required: true
        "};
        let manifest = manifest_from(yaml, Path::new("/manifests"));
        let context = test_context(Os::Linux);

        let error = build_plan(&manifest, &context).unwrap_err();
        assert!(error.to_string().contains("SWISS_TEST_SURELY_UNSET_TOKEN"));
    }

    #[test]
    fn force_file_overwrites_only_touches_write_steps() {
        let plan = Plan {
            steps: vec![
                Step::Command {
                    program: "x".to_owned(),
                    args: vec![],
                    cwd: None,
                    requires_admin: false,
                },
                Step::WriteFile {
                    path: PathBuf::from("/tmp/a"),
                    content: vec![],
                    overwrite: false,
                    append_if_missing: false,
                    backup: false,
                },
            ],
        };

        let forced = plan.force_file_overwrites();
        assert_eq!(forced.steps.len(), 2);
        assert!(matches!(forced.steps[0], Step::Command { .. }));
        assert!(matches!(
            forced.steps[1],
            Step::WriteFile {
                overwrite: true,
                ..
            }
        ));
    }
}
