pub use merman3_core::keys::SpecKeys;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

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
