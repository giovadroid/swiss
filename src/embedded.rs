pub const ENV_NU: &str = include_str!("../defaults/env.nu");
pub const CONF_NU: &[u8] = include_bytes!("../defaults/conf.nu");
pub const FNM_NU: &[u8] = include_bytes!("../defaults/env/fnm.nu");
pub const STARSHIP_NU: &[u8] = include_bytes!("../defaults/env/starship.nu");
pub const ZOXIDE_NU: &[u8] = include_bytes!("../defaults/conf/zoxide.nu");

pub const BUILT_HELIX_CONF: &[u8] = include_bytes!("../defaults/built-settings/helix.toml");
pub const BUILT_STARSHIP_CONF: &[u8] = include_bytes!("../defaults/built-settings/starship.toml");
pub const BUILT_IN_ZELLIJ_CONF: &[u8] = include_bytes!("../defaults/built-settings/zellij.yaml");

#[cfg(target_os = "macos")]
pub const MACOS_NU_ENV: &[u8] = include_bytes!("../defaults/env/macos.nu");

#[cfg(not(target_os = "macos"))]
pub const OS_NU_ENV: &[u8] = &[];

pub const CONF_YAML: &str = include_str!("../defaults/swiss.yaml");
