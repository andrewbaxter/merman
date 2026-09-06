use merman3_core::document::Document;
use merman3_core::layout::{Layout, LayoutConfig, Rows};
use merman3_core::matcher::match_document;
use merman3_core::measure::MeasureFixed;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use merman3_core::visual::Visual;
use std::rc::Rc;

/// The json syntax uses a 16px font, which merman treats as 16pt = 12px; the fixed
/// measurer makes every grapheme 0.6em, so 7.2px, and the 28.8px indent is 4 characters.
const UNIT: f64 = 12. * 0.6;

fn json_syntax() -> Syntax {
    let spec: SpecSyntax =
        serde_json::from_str(include_str!("../../syntaxes/json.json")).expect("syntax json");
    return Syntax::syntax_resolve(spec).unwrap_or_else(|e| panic!("{}", e.join("\n")));
}

fn parse(syntax: &Syntax, text: &str) -> Document {
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    return match_document(syntax, &value).unwrap_or_else(|e| panic!("{}", e.mismatch_format()));
}

fn render(rows: &Rows) -> Vec<String> {
    let unit = UNIT;
    let mut out = vec![];
    for row in &rows.rows {
        let mut line = String::new();
        for b in &row.bricks {
            let col = (b.converse / unit).round() as usize;
            while line.chars().count() < col {
                line.push(' ');
            }
            line.push_str(&b.text);
        }
        out.push(line);
    }
    return out;
}

fn build(text: &str, edge_chars: f64) -> (Layout, MeasureFixed) {
    let syntax = Rc::new(json_syntax());
    let doc = parse(&syntax, text);
    let mut measure = MeasureFixed;
    let visual = Rc::new(Visual::visual_build(&syntax, &doc, &mut measure));
    let layout = Layout::layout_build(
        syntax,
        visual,
        LayoutConfig::default(),
        edge_chars * UNIT,
        &mut measure,
    );
    return (layout, measure);
}

fn layout_text(text: &str, edge_chars: f64) -> Vec<String> {
    let (layout, _) = build(text, edge_chars);
    return render(&layout.layout_rows());
}

#[test]
fn single_line_fits() {
    let rows = layout_text(r#"{"a": 1, "b": [true, null]}"#, 100.);
    assert_eq!(rows, vec!["{a: 1, b: [true, null]}".to_string()]);
}

#[test]
fn compacts_outer_first() {
    let rows = layout_text(r#"{"a": 1, "b": [true, null]}"#, 19.);
    // The record (lowest depth) breaks; the array still fits on its line.
    assert_eq!(
        rows,
        vec![
            "{".to_string(),
            "    a: 1, ".to_string(),
            "    b: [true, null]".to_string(),
            "}".to_string(),
        ]
    );
}

#[test]
fn expands_when_edge_grows_and_recompacts() {
    let (mut layout, mut measure) = build(r#"{"a": 1, "b": [true, null]}"#, 19.);
    assert_eq!(render(&layout.layout_rows()).len(), 4);
    layout.layout_set_edge(100. * UNIT, &mut measure);
    assert_eq!(
        render(&layout.layout_rows()),
        vec!["{a: 1, b: [true, null]}".to_string()]
    );
    layout.layout_set_edge(12. * UNIT, &mut measure);
    assert_eq!(
        render(&layout.layout_rows()),
        vec![
            "{".to_string(),
            "    a: 1, ".to_string(),
            "    b: [".to_string(),
            "        true, ".to_string(),
            "        null".to_string(),
            "    ]".to_string(),
            "}".to_string(),
        ]
    );
    // Growing a little (under the retry factor) changes nothing; growing past it
    // expands only what fits.
    layout.layout_set_edge(13. * UNIT, &mut measure);
    assert_eq!(render(&layout.layout_rows()).len(), 7);
    layout.layout_set_edge(20. * UNIT, &mut measure);
    assert_eq!(
        render(&layout.layout_rows()),
        vec![
            "{".to_string(),
            "    a: 1, ".to_string(),
            "    b: [true, null]".to_string(),
            "}".to_string(),
        ]
    );
}

#[test]
fn nested_indent_chains() {
    let rows = layout_text(r#"{"a": {"bb": [1, 2, 3, 4, 5, 6]}}"#, 14.);
    assert_eq!(
        rows,
        vec![
            "{".to_string(),
            "    a: {".to_string(),
            "        bb: [".to_string(),
            "            1, ".to_string(),
            "            2, ".to_string(),
            "            3, ".to_string(),
            "            4, ".to_string(),
            "            5, ".to_string(),
            "            6".to_string(),
            "        ]".to_string(),
            "    }".to_string(),
            "}".to_string(),
        ]
    );
}

#[test]
fn soft_wraps_long_string() {
    let rows = layout_text(r#"["the quick brown fox jumps over the lazy dog"]"#, 20.);
    // Nearest-index split, so a line may hang half a character over the edge.
    assert_eq!(
        rows,
        vec![
            "[".to_string(),
            "    \"the quick brown ".to_string(),
            "    fox jumps over ".to_string(),
            "    the lazy dog\"".to_string(),
            "]".to_string(),
        ]
    );
}

#[test]
fn mismatch_reports_alternatives() {
    let syntax = json_syntax();
    let value: serde_json::Value = serde_json::from_str(r#"{"a": 1}"#).unwrap();
    // Rewrap so the syntax's fixed structure fails: a record inside an array is fine, so
    // instead feed a syntax-invalid document: a top level value of an unmatched kind is
    // impossible for json, so check the message for a record entry of a bad kind.
    let doc = match_document(&syntax, &value);
    assert!(doc.is_ok());
    let spec: SpecSyntax = serde_json::from_str(
        r##"{
          "background": "#000",
          "display_unit": "px",
          "root": {"back": {"fixed_record": [{"key": "v1", "value": {"fixed_literal": "true"}}]}, "front": []},
          "types": []
        }"##,
    )
    .unwrap();
    let syntax = Syntax::syntax_resolve(spec).unwrap_or_else(|e| panic!("{}", e.join("\n")));
    let value: serde_json::Value = serde_json::from_str(r#"{"v1": false}"#).unwrap();
    let err = match_document(&syntax, &value).err().expect("should mismatch");
    assert_eq!(err.mismatch_format().trim(), "/v1: expected literal true, got false");
}

#[test]
fn syntax_validation_reports_unused_field() {
    let spec: SpecSyntax = serde_json::from_str(
        r##"{
          "background": "#000",
          "display_unit": "px",
          "root": {"back": {"string": {"id": "x"}}, "front": []},
          "types": []
        }"##,
    )
    .unwrap();
    let errors = Syntax::syntax_resolve(spec).err().expect("should fail");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("field `x` is captured"), "{}", errors[0]);
}
