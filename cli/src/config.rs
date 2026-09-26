pub use merman_core::keys::SpecKeys;
use {
    merman_core::spec::{
        SpecObbox,
        SpecTextStyle,
        SpecTheme,
    },
    loga::{
        ResultContext,
        ea,
    },
    serde::Deserialize,
    std::{
        collections::{
            BTreeMap,
            HashMap,
        },
        path::{
            Path,
            PathBuf,
        },
    },
};

pub struct Config {
    pub extensions: HashMap<String, Mapping>,
    pub keys: SpecKeys,
    pub sources: Vec<PathBuf>,
    pub theme: SpecTheme,
}

impl Config {
    pub fn config_files(&self) -> String {
        return self.sources.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
    }

    pub fn config_mapping(&self, path: &Path) -> Option<&Mapping> {
        let ext = path.extension()?.to_str()?;
        return self.extensions.get(&normalize_ext(ext));
    }
}

pub fn config_load(dir: &Path, cwd: &Path) -> Result<Config, loga::Error> {
    let mut extensions = std::collections::HashMap::new();
    let mut keys = SpecKeys::default();
    let mut sources = vec![];
    let mut background = None;
    let mut cursor = None;
    let mut error_color = None;
    let mut font_family = None;
    let mut font_size = None;
    let mut hover = None;
    let mut icon_color = None;
    let mut text_color = None;
    let mut text_styles = BTreeMap::new();
    let dirs = {
        let mut out = vec![];
        let mut add = |dir: PathBuf| {
            if !out.contains(&dir) {
                out.push(dir);
            }
        };
        for ancestor in dir.ancestors() {
            add(ancestor.to_path_buf());
        }
        for ancestor in cwd.ancestors() {
            add(ancestor.to_path_buf());
        }
        if let Some(dirs) = directories::BaseDirs::new() {
            let config_dir = dirs.config_dir();
            add(config_dir.join("merman"));
            add(config_dir.to_path_buf());
        }
        out
    };
    for dir in dirs {
        for name in ["merman.json", ".merman.json"] {
            let path = dir.join(name);
            if !path.is_file() {
                continue;
            }
            let spec = (|| -> Result<SpecConfig, loga::Error> {
                let text = std::fs::read_to_string(&path).context("Error reading config file")?;
                return Ok(serde_json::from_str(&text).context("Error parsing config file")?);
            })().context_with("Error loading config", ea!(path = path.display()))?;
            for (ext, syntax) in spec.extensions {
                extensions.entry(normalize_ext(&ext)).or_insert_with(|| Mapping {
                    syntax: dir.join(syntax),
                    config: path.clone(),
                });
            }
            for (
                into,
                from,
            ) in [
                (&mut keys.common, spec.keys.common),
                (&mut keys.atom, spec.keys.atom),
                (&mut keys.array, spec.keys.array),
                (&mut keys.primitive, spec.keys.primitive),
            ] {
                for (action, bindings) in from {
                    into.entry(action).or_insert(bindings);
                }
            }
            background = background.or(spec.background);
            cursor = cursor.or(spec.cursor);
            error_color = error_color.or(spec.error_color);
            font_family = font_family.or(spec.font_family);
            font_size = font_size.or(spec.font_size);
            hover = hover.or(spec.hover);
            icon_color = icon_color.or(spec.icon_color);
            text_color = text_color.or(spec.text_color);
            for (name, style) in spec.text_styles {
                text_styles.entry(name).or_insert(style);
            }
            sources.push(path);
        }
    }
    return Ok(Config {
        extensions: extensions,
        keys: keys,
        sources: sources,
        theme: {
            let default = SpecTheme::default();
            SpecTheme {
                background: background.unwrap_or(default.background),
                cursor: cursor.unwrap_or(default.cursor),
                error_color: error_color.unwrap_or(default.error_color),
                font_family: font_family.unwrap_or(default.font_family),
                font_size: font_size.unwrap_or(default.font_size),
                hover: hover.unwrap_or(default.hover),
                icon_color: icon_color.unwrap_or(default.icon_color),
                text_color: text_color.unwrap_or(default.text_color),
                text_styles: text_styles,
            }
        },
    });
}

pub struct Mapping {
    pub config: PathBuf,
    pub syntax: PathBuf,
}

pub fn normalize_ext(ext: &str) -> String {
    return ext.trim_start_matches('.').to_lowercase();
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecConfig {
    pub background: Option<String>,
    pub cursor: Option<SpecObbox>,
    pub error_color: Option<String>,
    #[serde(default)]
    pub extensions: HashMap<String, PathBuf>,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub hover: Option<SpecObbox>,
    pub icon_color: Option<String>,
    #[serde(default)]
    pub keys: SpecKeys,
    pub text_color: Option<String>,
    #[serde(default)]
    pub text_styles: BTreeMap<String, SpecTextStyle>,
}
