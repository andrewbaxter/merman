//! Loads the file-extension -> syntax-file mappings from `merman.json` /
//! `.merman.json` config files.
//!
//! Config files are looked for in the directory containing the source file and
//! each of its parents, then the same for the working directory, then in the
//! user config directory (and a `merman` subdirectory of it). Mappings from
//! closer config files win; mappings from further ones are still available, so
//! a repo config can add to the user's.
use loga::{ea, ResultContext};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CONFIG_NAMES: [&str; 2] = ["merman.json", ".merman.json"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpecConfig {
    /// Maps file extension (with or without a leading `.`) to the syntax
    /// definition to view files with that extension. Relative paths are
    /// relative to the config file.
    #[serde(default)]
    extensions: HashMap<String, PathBuf>,
}

/// An extension mapping plus where it came from, for error messages.
pub struct Mapping {
    pub syntax: PathBuf,
    pub config: PathBuf,
}

pub struct Config {
    /// Keyed by lowercase extension without a leading `.`.
    pub extensions: HashMap<String, Mapping>,
    /// Every config file that was read, closest first.
    pub sources: Vec<PathBuf>,
}

fn normalize_ext(ext: &str) -> String {
    return ext.trim_start_matches('.').to_lowercase();
}

/// Directories to look for config files in, closest first.
fn search_dirs(source: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut add = |dir: PathBuf| {
        if !out.contains(&dir) {
            out.push(dir);
        }
    };
    let source_abs = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    for dir in source_abs.parent().into_iter().flat_map(|p| p.ancestors()) {
        add(dir.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
        for dir in cwd.ancestors() {
            add(dir.to_path_buf());
        }
    }
    if let Some(dirs) = directories::BaseDirs::new() {
        let config_dir = dirs.config_dir();
        add(config_dir.join("merman"));
        add(config_dir.to_path_buf());
    }
    return out;
}

fn read_config(path: &Path) -> Result<SpecConfig, loga::Error> {
    let text = std::fs::read_to_string(path).context("Error reading config file")?;
    return Ok(serde_json::from_str(&text).context("Error parsing config file")?);
}

/// Read all config files that apply to `source` and merge them.
pub fn load(source: &Path) -> Result<Config, loga::Error> {
    let mut extensions = HashMap::new();
    let mut sources = vec![];
    for dir in search_dirs(source) {
        for name in CONFIG_NAMES {
            let path = dir.join(name);
            if !path.is_file() {
                continue;
            }
            let spec = read_config(&path).context_with("Error loading config", ea!(path = path.display()))?;
            for (ext, syntax) in spec.extensions {
                // Closer config files are visited first, so don't overwrite.
                extensions.entry(normalize_ext(&ext)).or_insert_with(|| Mapping {
                    syntax: dir.join(syntax),
                    config: path.clone(),
                });
            }
            sources.push(path);
        }
    }
    return Ok(Config { extensions, sources });
}

impl Config {
    /// Find the syntax to use for a source file, by its extension.
    pub fn syntax_for(&self, source: &Path) -> Result<&Mapping, loga::Error> {
        let Some(ext) = source.extension().and_then(|e| e.to_str()) else {
            return Err(loga::err_with(
                "Source file has no extension, so no syntax can be looked up for it",
                ea!(source = source.display()),
            ));
        };
        let Some(mapping) = self.extensions.get(&normalize_ext(ext)) else {
            let mut known = self.extensions.keys().cloned().collect::<Vec<_>>();
            known.sort();
            return Err(loga::err_with(
                "No syntax is configured for this file extension",
                ea!(
                    source = source.display(),
                    extension = ext,
                    configured_extensions = known.join(", "),
                    config_files = self
                        .sources
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        };
        return Ok(mapping);
    }
}
