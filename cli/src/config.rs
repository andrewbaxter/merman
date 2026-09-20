pub use merman3_core::keys::SpecKeys;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecConfig {
    #[serde(default)]
    pub extensions: HashMap<String, PathBuf>,
    #[serde(default)]
    pub keys: SpecKeys,
}

pub struct Mapping {
    pub syntax: PathBuf,
    pub config: PathBuf,
}

pub struct Config {
    pub extensions: HashMap<String, Mapping>,
    pub keys: SpecKeys,
    pub sources: Vec<PathBuf>,
}

pub fn normalize_ext(ext: &str) -> String {
    return ext.trim_start_matches('.').to_lowercase();
}

impl Config {
    pub fn config_mapping(&self, path: &Path) -> Option<&Mapping> {
        let ext = path.extension()?.to_str()?;
        return self.extensions.get(&normalize_ext(ext));
    }

    pub fn config_files(&self) -> String {
        return self.sources.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
    }
}
