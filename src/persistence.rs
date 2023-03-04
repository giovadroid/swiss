use crate::installer::InstallerResult;
use bytecheck::CheckBytes;
use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{AlignedVec, Archive, Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const NUSHELL_DEP: &str = "nushell";
pub const FOLDERS_KEY: &str = "sk_dir";
pub const FILES_KEY: &str = "sk_file";
const CACHE_PATH: &str = ".config/swiss/.cache";
pub(crate) const CUSTOM_MODULES_PATH: &str = ".config/swiss/crates/";

pub(crate) const NU_ENV_LOADER: &str = "source ~/.config/swiss/env.nu;";
pub(crate) const NU_CONF_LOADER: &str = "source ~/.config/swiss/conf.nu;";

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone, Default)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SwissCache {
    inner: HashMap<String, String>,
    deps: HashMap<String, Dependency>,
    aliases: HashMap<String, String>,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone, Default)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct Dependency {
    pub(crate) name: String,
    pub(crate) version: Option<String>,
    pub(crate) status: DependencyStatus,
    pub(crate) r#type: DependencyType,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub enum DependencyStatus {
    Installed,
    NotInstalled,
    UpToDate,
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq, Clone)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub enum DependencyType {
    Cargo,
    Custom,
}

impl Default for DependencyType {
    fn default() -> Self {
        DependencyType::Custom
    }
}

impl Default for DependencyStatus {
    fn default() -> Self {
        DependencyStatus::NotInstalled
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
}

impl SwissCache {
    /// Creates a new cache with the default values.
    pub fn default() -> Self {
        Self {
            inner: HashMap::new(),
            deps: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    /// Creates a new cache from file.
    pub fn load() -> InstallerResult<Self> {
        let cache_path = home::home_dir().unwrap().join(CACHE_PATH);
        if cache_path.exists() {
            Ok(Self::deserialize(std::fs::read(cache_path)?))
        } else {
            Ok(Self::default())
        }
    }

    /// Save the cache to the cache file.
    pub fn save(&self) -> InstallerResult<()> {
        std::fs::write(home::home_dir().unwrap().join(CACHE_PATH), self.serialize())?;
        Ok(())
    }

    /// Deserialize the cache from a byte slice.
    pub(super) fn deserialize(bytes: Vec<u8>) -> SwissCache {
        let archived: &ArchivedSwissCache =
            unsafe { rkyv::archived_root::<SwissCache>(&bytes[..]) };
        archived.deserialize(&mut rkyv::Infallible).unwrap()
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

    pub(crate) fn get_cargo_package(&self, key: &str) -> Option<&Dependency> {
        self.get_dep(key)
    }

    pub fn set_dep(&mut self, key: &str, value: &Dependency) {
        self.deps.insert(key.to_string(), value.clone());
        self.save().unwrap_or(());
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

impl SwissCache {}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
