use {
    loga::{
        ResultContext,
        ea,
    },
    merman_core::spec::SpecCompression,
    std::path::Path,
};

pub fn format_document(value: &serde_json::Value) -> String {
    return serde_json::to_string(value).unwrap();
}

pub fn read_document(path: &Path, compression: SpecCompression) -> Result<String, loga::Error> {
    match compression {
        SpecCompression::None => {
            return std::fs::read_to_string(path).context_with("Error reading file", ea!(path = path.display()));
        },
        SpecCompression::Zstd => {
            let bytes = std::fs::read(path).context_with("Error reading file", ea!(path = path.display()))?;
            let decoded =
                zstd::decode_all(
                    bytes.as_slice(),
                ).context_with("Error decompressing zstd file", ea!(path = path.display()))?;
            return String::from_utf8(
                decoded,
            ).context_with("Decompressed file is not UTF-8", ea!(path = path.display()));
        },
    }
}

pub fn encode_document(text: &str, compression: SpecCompression) -> Result<Vec<u8>, loga::Error> {
    match compression {
        SpecCompression::None => return Ok(text.as_bytes().to_vec()),
        SpecCompression::Zstd => {
            return zstd::encode_all(text.as_bytes(), 19).context("Error compressing document");
        },
    }
}
