use crate::{commands, commands::CommandResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwissConfig {
    pub dependencies: Dependencies,
    pub rust: RustConfig,
    pub nushell: NushellConfig,
}

impl From<&str> for SwissConfig {
    fn from(s: &str) -> Self {
        serde_yaml::from_str::<SwissConfig>(s).unwrap_or_default()
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NushellConfig {
    pub version: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustConfig {
    pub toolchain: String,
}

pub type CargoDependenciesMap = BTreeMap<String, Option<CargoCustomConfig>>;
pub type CustomDependenciesMap = BTreeMap<String, CustomDependency>;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependencies {
    pub cargo: CargoDependenciesMap,
    pub customs: CustomDependenciesMap,
}

fn default_as_true() -> bool {
    true
}

pub fn default_is_asterisk() -> String {
    String::from("*")
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
        }
    }
}

impl CargoCustomConfig {
    pub(crate) fn is_generic_version(&self) -> bool {
        self.version == "*"
    }

    pub fn package_name(&self, crate_name: &str) -> String {
        if self.version == "*" {
            crate_name.to_string()
        } else {
            format!("{} {}", crate_name, self.version)
        }
    }

    pub fn install(&self, crate_name: &str) -> CommandResult<String> {
        commands::Cargo::install(self.package_name(crate_name).as_str(), &self.args)
    }

    #[cfg(target_os = "linux")]
    pub fn is_installable(&self) -> bool {
        self.linux
    }

    #[cfg(target_os = "windows")]
    pub fn is_installable(&self) -> bool {
        self.windows
    }

    #[cfg(target_os = "macos")]
    pub fn is_installable(&self) -> bool {
        self.macos
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomDependency {
    pub update: Option<Vec<String>>,
    pub install: Option<Vec<String>>,
    pub uninstall: Option<Vec<String>>,
    pub post_install: Option<OsCommands>,
    pub pre_uninstall: Option<OsCommands>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsCommands {
    pub windows: Option<Vec<String>>,
    pub linux: Option<Vec<String>>,
    pub macos: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opposite() {
        let mut config = SwissConfig::default();

        config
            .dependencies
            .cargo
            .insert("name".to_string(), Some(CargoCustomConfig::default()));

        let value = serde_yaml::to_string(&config).unwrap();

        log::debug!("{}", value);
    }

    #[test]
    fn test_parse_yaml() {
        let data = raw_example();

        if let Err(err) = serde_yaml::from_str::<SwissConfig>(data) {
            // log::debug!("error: {}", err);
            panic!("{}", err);
        }
    }

    fn temp_example() -> &'static str {
        let yaml = indoc::indoc! {r#"
       dependencies:
            cargo:
                sample:
                    windows: false
                    version: "0.1.0"
                    alias:
                        "sample": "sample"
                sample2:
                    name: sample2
                sample3:
            customs:
                helix:
                    install: "git clone https://github.com/helix-editor/helix; cd helix; cargo install --path helix-term; hx --grammar fetch; hx --grammar build"
                    update: "cd helix; git pull; cargo install --path helix-term; hx --grammar fetch; hx --grammar build"
                    uninstall: "cd helix; cargo uninstall --path helix-term; cd ..; rm -rf helix"
       rust:
            toolchain: "nightly"
       nushell:
            version: "0.1.0"
        "#};

        yaml
    }

    fn raw_example() -> &'static str {
        crate::installer::CONF_YAML
    }

    fn basic_yaml_example() -> &'static str {
        r#"
        dependencies:
            cargo:
        - bottom:
            name: bottom
            - zellij:
            name: zellij
        windows: no
            - sample:
            name: sample
        windows: no
        linux: yes
        mac: yes
        version: "0.1.0"
        customs:
        - helix:
            name: helix
        install: "git clone https://github.com/helix-editor/helix; cd helix; cargo install --path helix-term; hx --grammar fetch; hx --grammar build"
        update: "cd helix; git pull; cargo install --path helix-term; hx --grammar fetch; hx --grammar build"
        uninstall: "cd helix; cargo uninstall --path helix-term; cd ..; rm -rf helix"
            - rust-analyzer:
            name: rust-analyzer
        install: "git clone https://github.com/rust-analyzer/rust-analyzer.git; cd rust-analyzer; cargo xtask install --server"
        update: "cd rust-analyzer; git pull; cargo xtask install --server"
        uninstall: "cd rust-analyzer; cargo xtask uninstall --server; cd ..; rm -rf rust-analyzer"
         "#
    }
}
