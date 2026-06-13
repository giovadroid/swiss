use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Operating systems supported by manifest gates. Variants other than the
/// current platform are still constructed by cross-OS plan tests.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Macos,
    Windows,
}

impl Os {
    pub fn current() -> Self {
        #[cfg(target_os = "linux")]
        {
            Os::Linux
        }
        #[cfg(target_os = "macos")]
        {
            Os::Macos
        }
        #[cfg(target_os = "windows")]
        {
            Os::Windows
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Os::Linux => "linux",
            Os::Macos => "macos",
            Os::Windows => "windows",
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwissConfig {
    #[serde(default)]
    pub dependencies: Dependencies,
    #[serde(default)]
    pub rust: RustConfig,
    #[serde(default)]
    pub nushell: Option<NushellConfig>,
    #[serde(default)]
    pub package_manager: PackageManagerConfig,
    #[serde(default)]
    pub env: BTreeMap<String, EnvRequirement>,
    #[serde(default)]
    pub files: Vec<FileSpec>,
    #[serde(default)]
    pub shells: BTreeMap<String, ShellTargetConfig>,
    #[serde(default)]
    pub shell_modules: BTreeMap<String, ShellModuleSpec>,
}

impl SwissConfig {
    pub fn parse(raw: &str) -> Result<Self, anyhow::Error> {
        serde_yaml::from_str::<SwissConfig>(raw)
            .map_err(|error| anyhow::anyhow!("Invalid manifest YAML: {}", error))
    }

    /// Whether this manifest wants Nushell on the host: an explicit `nushell`
    /// section, or the nushell shell enabled in `shells`.
    pub fn manages_nushell(&self) -> bool {
        self.nushell.is_some()
            || self
                .shells
                .get("nushell")
                .map(|target| target.enabled && target.mode != Some(ShellMode::Print))
                .unwrap_or(false)
    }

    /// Pinned Nushell version, when one is requested.
    pub fn nushell_version(&self) -> Option<&str> {
        self.nushell
            .as_ref()
            .and_then(|nushell| nushell.version.as_deref())
    }
}

/// Optional Nushell management. Nushell is just another shell Swiss can
/// install: enabling `shells.nushell` is enough to get the latest release;
/// this section only exists to pin a version explicitly.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NushellConfig {
    /// Pin to this version; absent = latest.
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustConfig {
    #[serde(default)]
    pub toolchain: String,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub installer: OsScripts,
}

pub type CargoDependenciesMap = BTreeMap<String, Option<CargoCustomConfig>>;
pub type CustomDependenciesMap = BTreeMap<String, CustomDependency>;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependencies {
    #[serde(default)]
    pub cargo: CargoDependenciesMap,
    #[serde(default)]
    pub customs: CustomDependenciesMap,
}

fn default_as_true() -> bool {
    true
}

pub fn default_is_asterisk() -> String {
    String::from("*")
}

/// A command entry: either a plain string (runs through Nushell) or a
/// detailed form with explicit shell and admin requirement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandSpec {
    Plain(String),
    Detailed {
        run: String,
        #[serde(default)]
        shell: Option<String>,
        #[serde(default)]
        requires_admin: bool,
    },
}

/// Plain string commands run through the portable system shell, never through
/// a shell Swiss may not have installed yet. Use the detailed form
/// (`shell: nu`) for shell-specific syntax.
#[cfg(not(target_os = "windows"))]
pub const DEFAULT_COMMAND_SHELL: &str = "sh";
#[cfg(target_os = "windows")]
pub const DEFAULT_COMMAND_SHELL: &str = "powershell";

impl CommandSpec {
    pub fn run(&self) -> &str {
        match self {
            CommandSpec::Plain(run) => run,
            CommandSpec::Detailed { run, .. } => run,
        }
    }

    pub fn shell(&self) -> &str {
        match self {
            CommandSpec::Plain(_) => DEFAULT_COMMAND_SHELL,
            CommandSpec::Detailed { shell, .. } => {
                shell.as_deref().unwrap_or(DEFAULT_COMMAND_SHELL)
            }
        }
    }

    pub fn requires_admin(&self) -> bool {
        match self {
            CommandSpec::Plain(_) => false,
            CommandSpec::Detailed { requires_admin, .. } => *requires_admin,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.run().trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CargoCustomConfig {
    #[serde(default = "default_is_asterisk")]
    pub version: String,
    #[serde(default = "default_as_true")]
    pub windows: bool,
    #[serde(default = "default_as_true")]
    pub linux: bool,
    #[serde(default = "default_as_true")]
    pub macos: bool,
    #[serde(default)]
    pub alias: BTreeMap<String, String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
}

impl Default for CargoCustomConfig {
    fn default() -> Self {
        CargoCustomConfig {
            version: default_is_asterisk(),
            windows: default_as_true(),
            linux: default_as_true(),
            macos: default_as_true(),
            alias: BTreeMap::new(),
            args: Vec::new(),
            commands: Vec::new(),
        }
    }
}

impl CargoCustomConfig {
    pub(crate) fn is_generic_version(&self) -> bool {
        self.version == "*"
    }

    pub fn package_name(&self, crate_name: &str) -> String {
        if self.is_generic_version() {
            crate_name.to_string()
        } else {
            format!("{}@{}", crate_name, self.version)
        }
    }

    pub fn is_installable(&self, os: Os) -> bool {
        match os {
            Os::Linux => self.linux,
            Os::Macos => self.macos,
            Os::Windows => self.windows,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomDependency {
    #[serde(default)]
    pub update: Option<Vec<CommandSpec>>,
    #[serde(default)]
    pub install: Option<Vec<CommandSpec>>,
    #[serde(default)]
    pub git: Option<GitConfig>,
    #[serde(default)]
    pub windows: Option<bool>,
    #[serde(default)]
    pub linux: Option<bool>,
    #[serde(default)]
    pub macos: Option<bool>,
    #[serde(default, rename = "post-install")]
    pub post_install: Option<OsCommands>,
}

impl CustomDependency {
    pub fn is_installable(&self, os: Os) -> bool {
        match os {
            Os::Linux => self.linux.unwrap_or(true),
            Os::Macos => self.macos.unwrap_or(true),
            Os::Windows => self.windows.unwrap_or(true),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitConfig {
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsCommands {
    pub windows: Option<Vec<CommandSpec>>,
    pub linux: Option<Vec<CommandSpec>>,
    pub macos: Option<Vec<CommandSpec>>,
}

impl OsCommands {
    /// macOS falls back to the linux commands when undefined, mirroring the
    /// historical behavior of the embedded manifest.
    pub fn for_os(&self, os: Os) -> Option<&Vec<CommandSpec>> {
        match os {
            Os::Linux => self.linux.as_ref(),
            Os::Windows => self.windows.as_ref(),
            Os::Macos => self.macos.as_ref().or(self.linux.as_ref()),
        }
    }
}

pub type OsScripts = OsCommands;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageManagerConfig {
    #[serde(default)]
    pub linux: Option<LinuxPackageManagerConfig>,
    #[serde(default)]
    pub windows: Option<WindowsPackageManagerConfig>,
    #[serde(default)]
    pub macos: Option<MacosPackageManagerConfig>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinuxPackageManagerConfig {
    #[serde(default)]
    pub apt: Option<PackageManagerBackendConfig>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowsPackageManagerConfig {
    #[serde(default)]
    pub scoop: Option<PackageManagerBackendConfig>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacosPackageManagerConfig {
    #[serde(default)]
    pub brew: Option<PackageManagerBackendConfig>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageManagerBackendConfig {
    #[serde(default)]
    pub packages: Vec<String>,
    /// Run the manager refresh (e.g. `apt-get update`) before installing.
    #[serde(default)]
    pub update_index: bool,
}

/// Environment requirements: references, never secret values.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvRequirement {
    #[serde(default)]
    pub from_env: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
}

/// A file declared by the manifest. Either `source` (relative to the
/// manifest file) or inline `content` must be set.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSpec {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    pub dest: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub append_if_missing: bool,
    #[serde(default)]
    pub backup: bool,
    /// Write the file with elevated privileges (e.g. `sudo` on Unix). Use for
    /// destinations outside the user's home such as `/etc`.
    #[serde(default)]
    pub admin: bool,
    /// Restrict to these OS names (`linux`, `macos`, `windows`). Empty = all.
    #[serde(default)]
    pub os: Vec<String>,
}

impl FileSpec {
    pub fn applies_to(&self, os: Os) -> bool {
        self.os.is_empty() || self.os.iter().any(|name| name == os.name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellMode {
    ManagedLoader,
    Snippet,
    Profile,
    Print,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellTargetConfig {
    #[serde(default = "default_as_true")]
    pub enabled: bool,
    #[serde(default)]
    pub mode: Option<ShellMode>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub modules: Vec<String>,
}

impl Default for ShellTargetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: None,
            target: None,
            modules: Vec::new(),
        }
    }
}

/// Value of a shell module environment variable: plain string or path list
/// operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    Plain(String),
    PathOps {
        #[serde(default)]
        prepend: Vec<String>,
        #[serde(default)]
        append: Vec<String>,
    },
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellModuleSpec {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvValue>,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// Shell name -> init line emitted verbatim for that shell.
    #[serde(default)]
    pub init: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn parses_workstation_example_and_keeps_bootstrap_fields() {
        let config = SwissConfig::parse(crate::embedded::WORKSTATION_YAML).unwrap();

        let tealdeer = config
            .dependencies
            .cargo
            .get("tealdeer")
            .and_then(|config| config.as_ref())
            .unwrap();
        assert_eq!(tealdeer.commands[0].run(), "tldr --update");
        assert_eq!(tealdeer.commands[0].shell(), DEFAULT_COMMAND_SHELL);

        let helix = config.dependencies.customs.get("helix").unwrap();
        assert_eq!(
            helix.git.as_ref().unwrap().repo,
            "https://github.com/helix-editor/helix"
        );
        assert!(helix.post_install.as_ref().unwrap().linux.is_some());

        assert_eq!(config.rust.components, vec!["rust-analyzer"]);
        assert!(config.package_manager.linux.unwrap().apt.is_some());
        assert!(config.shells.contains_key("nushell"));
        assert!(!config.files.is_empty());
    }

    #[test]
    fn parses_minimal_manifest_without_defaults() {
        let config = SwissConfig::parse(indoc! {"
            dependencies:
              cargo:
                ripgrep:
        "})
        .unwrap();

        assert!(config.nushell.is_none());
        assert!(config.dependencies.cargo.contains_key("ripgrep"));
        assert!(config.shells.is_empty());
        assert!(config.files.is_empty());
    }

    #[test]
    fn invalid_yaml_returns_useful_error() {
        let error = SwissConfig::parse("dependencies: [not a map").unwrap_err();
        assert!(error.to_string().contains("Invalid manifest YAML"));
    }

    #[test]
    fn command_spec_supports_explicit_shell_and_admin() {
        let config = SwissConfig::parse(indoc! {r#"
            dependencies:
              customs:
                tool:
                  install:
                    - "plain command"
                    - run: ./configure && make install
                      shell: sh
                      requires_admin: true
        "#})
        .unwrap();

        let install = config
            .dependencies
            .customs
            .get("tool")
            .unwrap()
            .install
            .as_ref()
            .unwrap();
        assert_eq!(install[0].shell(), DEFAULT_COMMAND_SHELL);
        assert!(!install[0].requires_admin());
        assert_eq!(install[1].shell(), "sh");
        assert!(install[1].requires_admin());
        assert_eq!(install[1].run(), "./configure && make install");
    }

    #[test]
    fn file_spec_os_gate_filters_other_systems() {
        let spec = FileSpec {
            dest: "~/.swiss/env/macos.nu".to_owned(),
            os: vec!["macos".to_owned()],
            ..FileSpec::default()
        };
        assert!(spec.applies_to(Os::Macos));
        assert!(!spec.applies_to(Os::Linux));

        let unrestricted = FileSpec {
            dest: "~/.config/x".to_owned(),
            ..FileSpec::default()
        };
        assert!(unrestricted.applies_to(Os::Windows));
    }

    #[test]
    fn shell_sections_parse_roadmap_shapes() {
        let config = SwissConfig::parse(indoc! {r#"
            shells:
              nushell:
                enabled: true
                mode: managed-loader
                modules: [fnm, starship, zoxide]
              zsh:
                mode: snippet
                target: ~/.zshrc
                modules: [path, starship]
              bash:
                enabled: false
            shell_modules:
              path:
                env:
                  PATH:
                    prepend: ["~/.cargo/bin", "~/.local/bin"]
              starship:
                package: starship
                init:
                  zsh: eval "$(starship init zsh)"
        "#})
        .unwrap();

        let nushell = config.shells.get("nushell").unwrap();
        assert_eq!(nushell.mode, Some(ShellMode::ManagedLoader));
        assert_eq!(nushell.modules, vec!["fnm", "starship", "zoxide"]);
        assert!(!config.shells.get("bash").unwrap().enabled);
        assert!(config.manages_nushell());

        let path = config.shell_modules.get("path").unwrap();
        match path.env.get("PATH").unwrap() {
            EnvValue::PathOps { prepend, .. } => {
                assert_eq!(prepend, &vec!["~/.cargo/bin", "~/.local/bin"])
            }
            other => panic!("expected path ops, got {:?}", other),
        }
    }

    #[test]
    fn nushell_section_version_is_optional() {
        let pinned = SwissConfig::parse("nushell:\n  version: \"0.101.0\"").unwrap();
        assert_eq!(pinned.nushell_version(), Some("0.101.0"));
        assert!(pinned.manages_nushell());

        let latest = SwissConfig::parse("nushell: {}").unwrap();
        assert!(latest.nushell.is_some());
        assert_eq!(latest.nushell_version(), None);
        assert!(latest.manages_nushell());

        let absent = SwissConfig::parse("dependencies:\n  cargo:\n    bat:\n").unwrap();
        assert!(!absent.manages_nushell());

        // A disabled or print-only nushell shell does not manage nu either.
        let disabled = SwissConfig::parse("shells:\n  nushell:\n    enabled: false\n").unwrap();
        assert!(!disabled.manages_nushell());
        let print_only = SwissConfig::parse("shells:\n  nushell:\n    mode: print\n").unwrap();
        assert!(!print_only.manages_nushell());
    }
}
