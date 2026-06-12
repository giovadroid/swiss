use anyhow::Result;
use bytecheck::CheckBytes;
use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{check_archived_root, AlignedVec, Archive, Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const NUSHELL_DEP: &str = "nushell";
pub const FINGERPRINT_KEY: &str = "sk_manifest_fingerprint";
/// Path of the manifest registered by setup/apply; consumed by `swiss update`.
pub const MANIFEST_PATH_KEY: &str = "sk_manifest_path";
/// Newline-separated profiles registered together with the manifest path.
pub const MANIFEST_PROFILES_KEY: &str = "sk_manifest_profiles";
const CACHE_PATH: &str = ".config/swiss/.cache";
pub(crate) const NU_ENV_LOADER: &str = "source ~/.config/swiss/env.nu;";
pub(crate) const NU_CONF_LOADER: &str = "source ~/.config/swiss/conf.nu;";

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone, Default)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SwissCache {
    inner: HashMap<String, String>,
    deps: HashMap<String, Dependency>,
    aliases: HashMap<String, String>,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone, Default)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct Dependency {
    pub(crate) name: String,
    pub(crate) version: Option<String>,
    pub(crate) status: DependencyStatus,
    pub(crate) r#type: DependencyType,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone, Default)]
#[archive_attr(derive(CheckBytes, Debug))]
pub enum DependencyStatus {
    Installed,
    #[default]
    NotInstalled,
    UpToDate,
}

impl DependencyStatus {
    pub fn name(&self) -> &'static str {
        match self {
            DependencyStatus::Installed => "installed",
            DependencyStatus::NotInstalled => "not-installed",
            DependencyStatus::UpToDate => "up-to-date",
        }
    }
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone, Default)]
#[archive_attr(derive(CheckBytes, Debug))]
pub enum DependencyType {
    Cargo,
    #[default]
    Custom,
}

impl DependencyType {
    pub fn name(&self) -> &'static str {
        match self {
            DependencyType::Cargo => "cargo",
            DependencyType::Custom => "custom",
        }
    }
}

impl Dependency {
    pub fn new(
        name: String,
        version: Option<String>,
        status: DependencyStatus,
        r#type: DependencyType,
    ) -> Self {
        Self {
            name,
            version,
            status,
            r#type,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> Option<&String> {
        self.version.as_ref()
    }

    pub fn status(&self) -> &DependencyStatus {
        &self.status
    }

    pub fn dependency_type(&self) -> &DependencyType {
        &self.r#type
    }
}

impl SwissCache {
    /// Creates a new cache from file.
    pub fn load() -> Result<Self> {
        let cache_path = home::home_dir().unwrap().join(CACHE_PATH);
        if cache_path.exists() {
            Ok(Self::deserialize(std::fs::read(cache_path)?))
        } else {
            Ok(Self::default())
        }
    }

    /// Save the cache to the cache file.
    pub fn save(&self) -> Result<()> {
        let path = home::home_dir().unwrap().join(CACHE_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.serialize())?;
        Ok(())
    }

    /// Deletes the cache file.
    pub fn clean() -> Result<bool> {
        let path = home::home_dir().unwrap().join(CACHE_PATH);
        if path.exists() {
            std::fs::remove_file(path)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Deserialize the cache from a byte slice.
    pub(super) fn deserialize(bytes: Vec<u8>) -> SwissCache {
        match check_archived_root::<SwissCache>(&bytes) {
            Ok(archived) => archived
                .deserialize(&mut rkyv::Infallible)
                .expect("Swiss cache deserialization cannot fail after validation"),
            Err(error) => {
                log::warn!("Ignoring invalid Swiss cache: {}", error);
                SwissCache::default()
            }
        }
    }

    /// Serialize the cache to a byte slice.
    pub(super) fn serialize(&self) -> AlignedVec {
        let mut serializer = AllocSerializer::<0>::default();
        serializer.serialize_value(&self.clone()).unwrap();
        serializer.into_serializer().into_inner()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner.get(key).map(|s| s.as_str())
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.inner.insert(key.to_string(), value.to_string());
        self.save().unwrap_or(());
    }

    pub fn get_dep(&self, key: &str) -> Option<&Dependency> {
        self.deps.get(key)
    }

    pub fn set_dep(&mut self, key: &str, value: &Dependency) {
        self.set_dep_in_memory(key, value);
        self.save().unwrap_or(());
    }

    /// Inserts without persisting; used by tests and plan fixtures.
    pub fn set_dep_in_memory(&mut self, key: &str, value: &Dependency) {
        self.deps.insert(key.to_string(), value.clone());
    }

    pub fn deps(&self) -> &HashMap<String, Dependency> {
        &self.deps
    }

    pub fn set_alias(&mut self, key: &str, value: &str) {
        self.aliases.insert(key.to_string(), value.to_string());
        self.save().unwrap_or(());
    }

    pub(crate) fn set_aliases(&mut self, aliases: &BTreeMap<String, String>) {
        for (alias_name, alias_command) in aliases.iter() {
            self.set_alias(alias_name, alias_command)
        }
    }

    pub fn aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_serialization_roundtrip_keeps_deps_and_aliases() {
        let mut cache = SwissCache::default();
        cache.set_dep_in_memory(
            "ripgrep",
            &Dependency::new(
                "ripgrep".to_owned(),
                Some("14.0.0".to_owned()),
                DependencyStatus::UpToDate,
                DependencyType::Cargo,
            ),
        );
        cache.aliases.insert("ll".to_owned(), "ls -la".to_owned());
        cache
            .inner
            .insert(FINGERPRINT_KEY.to_owned(), "abc123".to_owned());

        let restored = SwissCache::deserialize(cache.serialize().to_vec());

        assert_eq!(restored, cache);
        assert_eq!(
            restored.get_dep("ripgrep").unwrap().version(),
            Some(&"14.0.0".to_owned())
        );
        assert_eq!(restored.get(FINGERPRINT_KEY), Some("abc123"));
    }

    #[test]
    fn invalid_cache_bytes_fall_back_to_default() {
        let restored = SwissCache::deserialize(vec![1, 2, 3, 4]);
        assert_eq!(restored, SwissCache::default());
    }
}
