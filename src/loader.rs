use crate::persistence::SwissCache;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::thread::JoinHandle;

pub type LoaderResult<T> = Result<T, anyhow::Error>;

const CONF_FOLDER_NAME: &'static str = "conf";
const ENV_FOLDER_NAME: &'static str = "env";

const DYNAMIC_ALIAS_FILE: &'static str = "aliases.dyn.nu";
const DYNAMIC_CONF_FILE: &str = "conf.dyn.nu";
const DYNAMIC_ENV_FILE: &str = "env.dyn.nu";

const SWISS_HOME_SUB_PATH: &'static str = ".config/swiss";
const SWISS_USER_HOME_SUB_PATH: &'static str = ".swiss";

const ENV_USER_FOLDER: &str = ".swiss/env";
const CONF_USER_FOLDER: &str = ".swiss/conf";

const NU_SCRIPT_EXTENSION: &'static str = "nu";

const DYNAMIC_FILE_PRE_HEADER: &'static str = "#StartFile: ";
const DYNAMIC_FILE_PRE_FOOTER: &'static str = "#EndFile: ";

pub fn initialize_nu_files() -> LoaderResult<(PathBuf, PathBuf)> {
    let home_path = home::home_dir().expect("Failed to read home dir");
    let swiss_home = home_path.join(SWISS_HOME_SUB_PATH);
    let cache = SwissCache::load()?;

    // Load dynamic Environments
    let env_loader = DynamicFolderLoader::new(
        swiss_home.join(DYNAMIC_ENV_FILE),
        vec![
            swiss_home.join(ENV_FOLDER_NAME),
            home_path.join(ENV_USER_FOLDER),
        ],
        NU_SCRIPT_EXTENSION,
    )
    .write()?;

    // Load dynamic Config
    let conf_loader = DynamicFolderLoader::new(
        swiss_home.join(DYNAMIC_CONF_FILE),
        vec![
            swiss_home.join(CONF_FOLDER_NAME),
            home_path.join(CONF_USER_FOLDER),
        ],
        NU_SCRIPT_EXTENSION,
    )
    .write()?;

    load_alias_into_nu(swiss_home.join(DYNAMIC_ALIAS_FILE), cache.aliases())?;

    if let Err(error) = env_loader.join() {
        log::error!("Failed to load dynamic env: {:?}", error);
    }

    if let Err(error) = conf_loader.join() {
        log::error!("Failed to load dynamic conf: {:?}", error);
    }

    Ok((swiss_home, home_path.join(SWISS_USER_HOME_SUB_PATH)))
}

pub fn load_alias_into_nu(
    conf_file: PathBuf,
    aliases: &HashMap<String, String>,
) -> LoaderResult<()> {
    let mut buffer_file = std::fs::File::create(conf_file)?;

    for (alias, command) in aliases {
        if let Err(e) = buffer_file.write_all(format!("alias {} = {}\n", alias, command).as_bytes())
        {
            log::error!("Error writing to aliases: {}", e);
        };
        log::debug!("AliasCreated: {} = {}", alias, command);
    }
    buffer_file.flush()?;
    Ok(())
}

pub struct DynamicFolderLoader {
    file: PathBuf,
    folders: Vec<PathBuf>,
    filter: String,
}

impl DynamicFolderLoader {
    pub fn new(file: PathBuf, folders: Vec<PathBuf>, filter: &str) -> Self {
        Self {
            file,
            folders,
            filter: filter.to_owned(),
        }
    }

    pub fn copy(&self) -> Self {
        Self {
            file: self.file.clone(),
            folders: self.folders.clone(),
            filter: self.filter.clone(),
        }
    }

    pub fn write(&self) -> LoaderResult<JoinHandle<()>> {
        let me = self.copy();
        let handle = std::thread::spawn(move || {
            if let Ok(mut buffer_file) = std::fs::File::create(&me.file) {
                log::debug!("Creating: {}", me.file.display());
                for folder in me.folders.iter() {
                    Self::join_dir_on_file(folder, &mut buffer_file, &me.filter)
                        .unwrap_or_default();
                }
                buffer_file.flush().unwrap_or_default();
            }
        });
        Ok(handle)
    }
    fn push_to_file(buffer_file: &mut File, file: &PathBuf) -> LoaderResult<()> {
        let file_content = std::fs::read_to_string(file)?;
        buffer_file.write_all(
            format!(
                "{}{}\n{}\n{}{}\n\n",
                DYNAMIC_FILE_PRE_HEADER,
                file.display(),
                file_content,
                DYNAMIC_FILE_PRE_FOOTER,
                file.display()
            )
            .as_bytes(),
        )?;
        Ok(())
    }

    fn join_dir_on_file(
        target_dir: &PathBuf,
        buffer_file: &mut File,
        filter: &str,
    ) -> LoaderResult<()> {
        for file in std::fs::read_dir(target_dir).expect("Failed to read dir") {
            let target_file = file.expect("Failed to get file").path();
            if target_file
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                != filter
            {
                continue;
            }
            if let Err(e) = Self::push_to_file(buffer_file, &target_file) {
                log::debug!("Error: writing to system env: {}", e);
            } else {
                log::debug!("FileAdded: {}", target_file.display());
            }
        }
        Ok(())
    }
}
