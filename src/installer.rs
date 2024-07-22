use crate::commands::NuShell;
use crate::embedded::{
    BUILT_HELIX_CONF, BUILT_IN_ZELLIJ_CONF, BUILT_STARSHIP_CONF, CONF_NU, CONF_YAML, ENV_NU,
    FNM_NU, OS_NU_ENV, STARSHIP_NU, ZOXIDE_NU,
};
use crate::parser::SwissConfig;
use crate::paths::CARGO_PATH;
use crate::persistence::{
    Dependency, DependencyStatus, DependencyType, SwissCache, FILES_KEY, FOLDERS_KEY, NUSHELL_DEP,
};
use crate::{paths, NU_CONF_LOADER, NU_ENV_LOADER};
use std::path::PathBuf;

pub type InstallerResult<T> = Result<T, anyhow::Error>;

#[derive(thiserror::Error, Debug)]
pub enum InstallerError {
    #[error("Admin Required Error")]
    AdminRequiredError,
}

pub struct Installer {
    cache: SwissCache,
    config: SwissConfig,
}

impl Default for Installer {
    fn default() -> Self {
        Self::from(CONF_YAML)
    }
}

impl From<&str> for Installer {
    fn from(conf: &str) -> Self {
        Self::new(SwissConfig::from(conf))
    }
}

impl Installer {
    pub fn new(config: SwissConfig) -> Self {
        Self {
            cache: SwissCache::load().unwrap_or_default(),
            config,
        }
    }

    pub fn configure_nu(&self) -> InstallerResult<()> {
        let env_path = NuShell::get_env_value("$nu.env-path")?;
        let conf_path = NuShell::get_env_value("$nu.config-path")?;

        let mut env_data = std::fs::read_to_string(&env_path)?;
        if !env_data.contains(NU_ENV_LOADER) {
            env_data.push_str(
                format!(
                    "\n# Swiss environment loader\n{}\n# Swiss environment loader end line\n",
                    NU_ENV_LOADER
                )
                .as_str(),
            );
            std::fs::write(&env_path, env_data)?;
            log::debug!("Added {} to {}", NU_ENV_LOADER, env_path);
        }

        let conf_data = std::fs::read_to_string(&conf_path)?;
        if !conf_data.contains(NU_CONF_LOADER) {
            let mut conf_data = std::fs::read_to_string(&conf_path)?;
            conf_data.push_str(
                format!(
                    "\n# Swiss configuration loader\n{}\n# Swiss configuration loader end line\n",
                    NU_CONF_LOADER
                )
                .as_str(),
            );
            std::fs::write(&conf_path, conf_data)?;
            log::debug!("Added {} to {}", NU_CONF_LOADER, conf_path);
        }
        Ok(())
    }

    pub fn install_binstall(&mut self) -> InstallerResult<()> {
        for (name, config) in self.config.dependencies.cargo.iter() {
            let config = config.clone().unwrap_or_default();
            if name == "cargo-binstall" {
                if let Err(error) = config.install(name) {
                    log::error!("Installing cargo dependency error {}", error);
                    continue;
                }
                self.cache.set_dep(
                    name,
                    &Dependency::new(
                        name.to_owned(),
                        Some(config.version.clone()),
                        DependencyStatus::UpToDate,
                        DependencyType::Cargo,
                    ),
                );

                self.cache.set_aliases(&config.alias);
            }
        }

        Ok(())
    }

    pub fn install_cargo(&mut self) -> InstallerResult<()> {
        for (name, config) in self.config.dependencies.cargo.iter() {
            let config = config.clone().unwrap_or_default();

            if !config.is_installable() {
                log::warn!("{} is not installable: reason: {:?}", name, config);
                continue;
            }

            if !config.is_generic_version() {
                if let Some(installed) = self.cache.get_cargo_package(name) {
                    if installed
                        .version
                        .as_ref()
                        .map(|v| v == &config.version)
                        .unwrap_or(false)
                    {
                        // If versions are the same, skip installing.
                        // TODO: Check if new version available.
                        continue;
                    }
                }
            }

            if let Err(error) = config.install(name) {
                log::error!("Installing cargo dependency error {}", error);
                continue;
            }

            self.cache.set_dep(
                name,
                &Dependency::new(
                    name.to_owned(),
                    Some(config.version.clone()),
                    DependencyStatus::UpToDate,
                    DependencyType::Cargo,
                ),
            );

            self.cache.set_aliases(&config.alias);
        }
        Ok(())
    }

    pub fn install_custom(&mut self) -> InstallerResult<()> {
        let home_dir = home::home_dir()
            .expect("Unable to obtain home directory")
            .join(crate::persistence::CUSTOM_MODULES_PATH);

        for (name, config) in self.config.dependencies.customs.iter() {
            // let mut command = std::process::Command::new(&nu_path);

            let command_args = self
                .cache
                .get_dep(name)
                .map(|_| &config.update)
                .unwrap_or(&config.install);

            if command_args.is_none() {
                continue;
            }

            let command_args = command_args.as_ref().unwrap();
            if command_args.is_empty() {
                continue;
            }

            let script = format!("\"{}\"", command_args.join(";"));
            log::info!("Running command: [{:?}]nu -c {}", home_dir, &script);

            // NuShell::run(&["-c", &script])?;
            // command.arg("-c").arg(script).current_dir(&home_dir);

            if let Err(error) = NuShell::run(&["-c", &script], Some(home_dir.to_str().unwrap())) {
                log::error!("ERROR installing {:?}: {}", name, error);
            } else {
                self.cache.set_dep(
                    &name,
                    &Dependency::new(
                        name.clone(),
                        None,
                        DependencyStatus::UpToDate,
                        DependencyType::Custom,
                    ),
                );
            }
        }
        Ok(())
    }

    pub fn prepare_latest_nu(&mut self) -> InstallerResult<()> {
        if !NuShell::is_installed() {
            log::info!(
                "Nu is not installed, installing {} version",
                &self.config.nushell.version
            );
            NuShell::install(&self.config.nushell.version)?;
        }

        let installed_version = NuShell::version()?;
        if !installed_version.contains(self.config.nushell.version.as_str()) {
            log::info!(
                "Nu {} is outdated, installing {} version",
                installed_version,
                &self.config.nushell.version
            );

            NuShell::install(&self.config.nushell.version)?;
        } else {
            log::info!("Nu {} is up to date", installed_version);
        }

        self.cache.set_dep(
            NUSHELL_DEP,
            &Dependency::new(
                NUSHELL_DEP.to_owned(),
                Some(self.config.nushell.version.clone()),
                DependencyStatus::Installed,
                DependencyType::Cargo,
            ),
        );
        Ok(())
    }

    pub fn check_directory(path: &PathBuf) -> InstallerResult<()> {
        if !path.exists() {
            log::debug!("Creating directory: {}", path.to_string_lossy());
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }

    pub fn check_file(path: &PathBuf, content: &[u8], force: bool) -> InstallerResult<()> {
        if force || !path.exists() {
            if let Some(parent) = path.parent() {
                Self::check_directory(&parent.to_path_buf())?;
            }
            if force && (path.exists() || path.is_symlink()) {
                log::debug!("Removing file: {}", path.to_string_lossy());
                std::fs::remove_file(path)?;
            }
            log::debug!("Writing file {}", path.to_string_lossy());
            std::fs::write(path, content)?;
        }
        Ok(())
    }

    #[allow(unused)]
    pub fn delete_file(path: &PathBuf) -> InstallerResult<()> {
        if path.exists() {
            log::debug!("Disabling file {}", path.to_string_lossy());
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn create_folders(&mut self) -> InstallerResult<()> {
        let home = home::home_dir().unwrap();
        Self::check_directory(&home.join(".config/swiss/env"))?;
        Self::check_directory(&home.join(".config/swiss/conf"))?;
        Self::check_directory(&home.join(crate::persistence::CUSTOM_MODULES_PATH))?;
        Self::check_directory(&home.join(".swiss/env"))?;
        Self::check_directory(&home.join(".swiss/conf"))?;

        self.cache.set(FOLDERS_KEY, "true");
        log::info!("Folders created");
        Ok(())
    }

    pub fn create_files(&mut self, force: bool) -> InstallerResult<()> {
        Self::check_file(&paths::DYN_ENV, &[], force)?;
        Self::check_file(&paths::DYN_CONF, &[], force)?;

        Self::check_file(
            &paths::SWISS_ENV,
            format!("{}\n{}", CARGO_PATH, ENV_NU).as_bytes(),
            force,
        )?;

        Self::check_file(&paths::SWISS_CONF, CONF_NU, force)?;
        Self::check_file(&paths::ENV_FOLDER.join("fnm.nu"), FNM_NU, force)?;
        Self::check_file(&paths::ENV_FOLDER.join("starship.nu"), STARSHIP_NU, force)?;

        // Old versions has zoxide in env, so wi need to remove it
        // Self::delete_file(&paths::CONFIG_HOME.join("env/zoxide.nu"))?;
        Self::check_file(&paths::CONF_FOLDER.join("zoxide.nu"), ZOXIDE_NU, force)?;

        Self::check_file(&paths::OS_ENV, OS_NU_ENV, force)?;

        Self::check_file(
            &paths::CONFIG_PATH.join("starship.toml"),
            BUILT_STARSHIP_CONF,
            force,
        )?;
        Self::check_file(
            &paths::CONFIG_PATH.join("helix").join("helix.toml"),
            BUILT_HELIX_CONF,
            force,
        )?;
        Self::check_file(
            &paths::CONFIG_PATH.join("zellij").join("config.yaml"),
            BUILT_IN_ZELLIJ_CONF,
            force,
        )?;

        self.cache.set(FILES_KEY, "true");
        log::info!("Files created");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub fn is_admin() -> InstallerResult<bool> {
        // use std::process::Command;
        // let output = Command::new("powershell")
        //     .arg("/c")
        //     .arg("whoami")
        //     .output()?;
        // let stdout = String::from_utf8_lossy(&output.stdout);
        // Ok(stdout.contains("administrator"))
        Ok(true)
    }

    #[cfg(target_os = "linux")]
    pub fn is_admin() -> InstallerResult<bool> {
        Ok(true)
        // let output = std::process::Command::new("id").arg("-u").output()?;
        // let stdout = String::from_utf8_lossy(&output.stdout);
        // Ok(stdout.contains('0'))
    }

    #[cfg(target_os = "macos")]
    pub fn is_admin() -> InstallerResult<bool> {
        Ok(true)
        // use std::process::Command;
        // let output = Command::new("whoami").output()?;
        // let stdout = String::from_utf8_lossy(&output.stdout);
        // Ok(stdout.contains("root"))
    }
}
