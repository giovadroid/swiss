use crate::parser::SwissConfig;
use anyhow::{anyhow, bail, Context, Result};
use serde_yaml::Value;
use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

const INCLUDES_KEY: &str = "includes";
const PROFILES_KEY: &str = "profiles";

/// A manifest loaded from disk with includes resolved and profiles applied.
#[derive(Debug, Clone)]
pub struct LoadedManifest {
    pub config: SwissConfig,
    /// Directory of the entry manifest; `files[].source` paths resolve here.
    pub base_dir: PathBuf,
    /// Fingerprint of the merged manifest, stored in the cache after apply.
    pub fingerprint: String,
}

pub fn load_manifest(path: &Path, profiles: &[String]) -> Result<LoadedManifest> {
    let mut visited = BTreeSet::new();
    let mut value = load_value(path, &mut visited)?;

    let profile_definitions = value
        .as_mapping_mut()
        .and_then(|mapping| mapping.remove(PROFILES_KEY));

    for profile in profiles {
        let overlay = lookup_profile(profile_definitions.as_ref(), profile)?;
        value = merge_values(value, overlay);
    }

    let merged_yaml = serde_yaml::to_string(&value)
        .with_context(|| format!("Failed to re-serialize merged manifest {}", path.display()))?;

    let config = SwissConfig::parse(&merged_yaml)
        .with_context(|| format!("Failed to parse manifest {}", path.display()))?;

    Ok(LoadedManifest {
        config,
        base_dir: path
            .parent()
            .map(Path::to_path_buf)
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| PathBuf::from(".")),
        fingerprint: fingerprint(&merged_yaml),
    })
}

fn lookup_profile(definitions: Option<&Value>, profile: &str) -> Result<Value> {
    let mapping = definitions.and_then(Value::as_mapping).ok_or_else(|| {
        anyhow!(
            "Unknown profile '{}': the manifest defines no profiles",
            profile
        )
    })?;

    mapping
        .get(Value::String(profile.to_owned()))
        .cloned()
        .ok_or_else(|| {
            let available: Vec<String> = mapping
                .keys()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            anyhow!(
                "Unknown profile '{}'. Available profiles: {}",
                profile,
                available.join(", ")
            )
        })
}

fn load_value(path: &Path, visited: &mut BTreeSet<PathBuf>) -> Result<Value> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("Manifest not found: {}", path.display()))?;
    if !visited.insert(canonical) {
        bail!("Circular include detected at {}", path.display());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest {}", path.display()))?;
    let mut value: Value = serde_yaml::from_str(&raw)
        .with_context(|| format!("Invalid YAML in {}", path.display()))?;

    if value.is_null() {
        value = Value::Mapping(Default::default());
    }

    let includes = value
        .as_mapping_mut()
        .and_then(|mapping| mapping.remove(INCLUDES_KEY));

    let mut base = Value::Mapping(Default::default());
    if let Some(includes) = includes {
        let entries = includes
            .as_sequence()
            .cloned()
            .ok_or_else(|| anyhow!("'includes' must be a list of paths in {}", path.display()))?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        for entry in entries {
            let include_path = entry.as_str().ok_or_else(|| {
                anyhow!("'includes' entries must be strings in {}", path.display())
            })?;
            let resolved = parent.join(include_path);
            let included = load_value(&resolved, visited).with_context(|| {
                format!(
                    "Failed to load include '{}' from {}",
                    include_path,
                    path.display()
                )
            })?;
            base = merge_values(base, included);
        }
    }

    // The including file always wins over its includes.
    Ok(merge_values(base, value))
}

/// Deep merge: maps merge per key, sequences concatenate (skipping exact
/// duplicates), scalars are overridden by the overlay. A null overlay keeps
/// the base value, so `ripgrep:`-style empty keys do not erase earlier data.
pub(crate) fn merge_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (base, Value::Null) => base,
        (Value::Mapping(mut base_map), Value::Mapping(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.remove(&key) {
                    Some(base_value) => {
                        base_map.insert(key, merge_values(base_value, overlay_value));
                    }
                    None => {
                        base_map.insert(key, overlay_value);
                    }
                }
            }
            Value::Mapping(base_map)
        }
        (Value::Sequence(mut base_seq), Value::Sequence(overlay_seq)) => {
            for item in overlay_seq {
                if !base_seq.contains(&item) {
                    base_seq.push(item);
                }
            }
            Value::Sequence(base_seq)
        }
        (_, overlay) => overlay,
    }
}

fn fingerprint(merged_yaml: &str) -> String {
    let mut hasher = DefaultHasher::new();
    merged_yaml.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use std::fs;

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn loads_single_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "bootstrap.yaml",
            indoc! {"
                dependencies:
                  cargo:
                    ripgrep:
            "},
        );

        let manifest = load_manifest(&path, &[]).unwrap();
        assert!(manifest.config.dependencies.cargo.contains_key("ripgrep"));
        assert_eq!(manifest.base_dir, dir.path());
        assert_eq!(manifest.fingerprint.len(), 16);
    }

    #[test]
    fn includes_merge_relative_to_including_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("parts")).unwrap();
        write(
            dir.path(),
            "parts/base.yaml",
            indoc! {"
                package_manager:
                  linux:
                    apt:
                      packages: [git]
                dependencies:
                  cargo:
                    ripgrep:
            "},
        );
        let path = write(
            dir.path(),
            "bootstrap.yaml",
            indoc! {"
                includes:
                  - ./parts/base.yaml
                package_manager:
                  linux:
                    apt:
                      packages: [curl, git]
                dependencies:
                  cargo:
                    bat:
            "},
        );

        let manifest = load_manifest(&path, &[]).unwrap();
        let apt = manifest.config.package_manager.linux.unwrap().apt.unwrap();
        // include first, including file appended without duplicates
        assert_eq!(apt.packages, vec!["git", "curl"]);
        assert!(manifest.config.dependencies.cargo.contains_key("ripgrep"));
        assert!(manifest.config.dependencies.cargo.contains_key("bat"));
    }

    #[test]
    fn profiles_overlay_base_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "bootstrap.yaml",
            indoc! {"
                dependencies:
                  cargo:
                    ripgrep:
                profiles:
                  service-host:
                    package_manager:
                      linux:
                        apt:
                          packages: [docker.io]
                  devops:
                    dependencies:
                      cargo:
                        bottom:
            "},
        );

        let manifest =
            load_manifest(&path, &["service-host".to_owned(), "devops".to_owned()]).unwrap();
        assert!(manifest.config.dependencies.cargo.contains_key("ripgrep"));
        assert!(manifest.config.dependencies.cargo.contains_key("bottom"));
        assert_eq!(
            manifest
                .config
                .package_manager
                .linux
                .unwrap()
                .apt
                .unwrap()
                .packages,
            vec!["docker.io"]
        );
    }

    #[test]
    fn unknown_profile_fails_listing_available() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "bootstrap.yaml",
            indoc! {"
                profiles:
                  workstation:
                    dependencies:
                      cargo:
                        ripgrep:
            "},
        );

        let error = load_manifest(&path, &["nope".to_owned()]).unwrap_err();
        let message = format!("{}", error);
        assert!(message.contains("Unknown profile 'nope'"));
        assert!(message.contains("workstation"));
    }

    #[test]
    fn missing_include_fails_with_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "bootstrap.yaml",
            indoc! {"
                includes:
                  - ./missing.yaml
            "},
        );

        let error = load_manifest(&path, &[]).unwrap_err();
        assert!(format!("{:#}", error).contains("missing.yaml"));
    }

    #[test]
    fn invalid_yaml_fails_with_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "bootstrap.yaml", "dependencies: [broken");

        let error = load_manifest(&path, &[]).unwrap_err();
        assert!(format!("{:#}", error).contains("bootstrap.yaml"));
    }

    #[test]
    fn circular_includes_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.yaml", "includes: [./b.yaml]");
        let path_a = dir.path().join("a.yaml");
        write(dir.path(), "b.yaml", "includes: [./a.yaml]");

        let error = load_manifest(&path_a, &[]).unwrap_err();
        assert!(format!("{:#}", error).contains("Circular include"));
    }

    #[test]
    fn fingerprint_changes_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let path_a = write(
            dir.path(),
            "a.yaml",
            "dependencies:\n  cargo:\n    ripgrep:\n",
        );
        let path_b = write(dir.path(), "b.yaml", "dependencies:\n  cargo:\n    bat:\n");

        let first = load_manifest(&path_a, &[]).unwrap().fingerprint;
        let second = load_manifest(&path_b, &[]).unwrap().fingerprint;
        assert_ne!(first, second);
    }
}
