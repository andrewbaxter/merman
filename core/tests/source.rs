use {
    merman_core::matcher::source_parse,
    serde_json::Value,
};

fn nested(depth: usize) -> String {
    return format!("{}{}", "[".repeat(depth), "]".repeat(depth));
}

#[test]
fn source_parse_reads_nesting_deeper_than_serde_json_allows() {
    let text = nested(512);
    assert!(serde_json::from_str::<Value>(&text).is_err());
    let parsed = source_parse(&text).unwrap();
    let mut at = &parsed;
    let mut levels = 1;
    while let Value::Array(items) = at {
        match items.first() {
            Some(inner) => at = inner,
            None => break,
        }
        levels += 1;
    }
    assert_eq!(levels, 512);
}

#[test]
fn source_parse_rejects_nesting_past_its_limit() {
    let error = source_parse(&nested(513)).unwrap_err();
    assert!(error.to_string().contains("deeper than 512"));
}

#[test]
fn source_parse_ignores_brackets_in_strings() {
    let text = format!("[\"{}\\\"{}\"]", "[".repeat(600), "{".repeat(600));
    assert!(source_parse(&text).is_ok());
}

#[test]
fn source_parse_rejects_trailing_text() {
    assert!(source_parse("{} {}").is_err());
}
