pub mod config;
pub mod compress;

use {
    loga::{
        ResultContext,
        ea,
    },
    merman_core::{
        document::Document,
        matcher::{
            match_document,
            source_parse,
        },
        spec::{
            SpecCompression,
            SpecSyntax,
        },
        syntax::Syntax,
    },
    std::path::Path,
};

pub struct LoadedDocument {
    pub syntax_text: String,
    pub syntax: Syntax,
    pub compression: SpecCompression,
    pub source_text: String,
    pub document: Document,
}

pub fn load_document(config: &config::Config, source: &Path) -> Result<LoadedDocument, loga::Error> {
    let Some(mapping) = config.config_mapping(source) else {
        let mut known = config.extensions.keys().cloned().collect::<Vec<_>>();
        known.sort();
        return Err(
            loga::err_with(
                "No syntax is configured for this file's extension",
                ea!(
                    source = source.display(),
                    configured_extensions = known.join(", "),
                    config_files = config.config_files()
                ),
            ),
        );
    };
    let syntax_path = &mapping.syntax;
    let syntax_text =
        std::fs::read_to_string(
            syntax_path,
        ).context_with(
            "Error reading the syntax configured for this file",
            ea!(syntax = syntax_path.display(), config = mapping.config.display()),
        )?;
    let spec =
        serde_json::from_str::<SpecSyntax>(
            &syntax_text,
        ).context_with("Error parsing syntax", ea!(syntax = syntax_path.display()))?;
    let compression = spec.compression;
    let syntax = match Syntax::syntax_resolve(spec, &config.theme) {
        Ok(s) => s,
        Err(errors) => {
            return Err(
                loga::agg_err_with(
                    "Errors in syntax",
                    errors.0.into_iter().map(|e| loga::err(e.to_string())).collect(),
                    ea!(syntax = syntax_path.display()),
                ),
            );
        },
    };
    let source_text = compress::read_document(source, compression)?;
    let value = source_parse(&source_text).context_with("Error parsing source", ea!(source = source.display()))?;
    let document = match match_document(&syntax, &value) {
        Ok(d) => d,
        Err(e) => {
            return Err(
                loga::err_with(
                    format!("Source doesn't match syntax:\n{}", e.mismatch_format()),
                    ea!(source = source.display(), syntax = syntax_path.display()),
                ),
            );
        },
    };
    return Ok(LoadedDocument {
        syntax_text: syntax_text,
        syntax: syntax,
        compression: compression,
        source_text: source_text,
        document: document,
    });
}
