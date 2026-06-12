//! Starter manifests embedded only for `swiss setup --template` and tests.
//! Swiss no longer auto-applies any embedded bootstrap manifest.

pub const WORKSTATION_YAML: &str = include_str!("../examples/workstation.yaml");
pub const SERVICE_HOST_YAML: &str = include_str!("../examples/service-host.yaml");
pub const DEV_SHELL_YAML: &str = include_str!("../examples/dev-shell.yaml");
