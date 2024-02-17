use std::path::PathBuf;
use lazy_static::lazy_static;


#[cfg(not(target_os = "windows"))]
pub const CARGO_PATH: &str = "$env.PATH = ($env.PATH | prepend ~/.cargo/bin)";
#[cfg(target_os = "windows")]
const CARGO_PATH: &str = "$env.Path = ($env.Path | prepend ~/.cargo/bin)\n$env.PATH = $env.Path";

const CONFIG: &str = ".config";
const SWISS: &str = "swiss";
const HOME_SWISS: &str = ".swiss";
const ENV_PATH: &str = "env";
const CONF_PATH: &str = "conf";


// Folders
lazy_static! {
    pub static ref HOME: PathBuf = home::home_dir().unwrap();
    pub static ref CONFIG_HOME: PathBuf = CONFIG_PATH.join(SWISS);
    pub static ref SWISS_HOME: PathBuf = HOME.join(HOME_SWISS);

    pub static ref CONFIG_PATH: PathBuf = HOME.join(CONFIG);
    pub static ref ENV_FOLDER: PathBuf = SWISS_HOME.join(ENV_PATH);
    pub static ref CONF_FOLDER: PathBuf = SWISS_HOME.join(CONF_PATH);
}

// Files
lazy_static!{
    pub static ref DYN_ENV: PathBuf = CONFIG_HOME.join("env.dyn.nu");
    pub static ref DYN_CONF: PathBuf = CONFIG_HOME.join("conf.dyn.nu");
    pub static ref SWISS_ENV: PathBuf = CONFIG_HOME.join("env.nu");
    pub static ref SWISS_CONF: PathBuf = CONFIG_HOME.join("conf.nu");
}
