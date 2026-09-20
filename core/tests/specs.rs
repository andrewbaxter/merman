mod common;

use common::{
    build,
    load_document,
    load_syntax,
    render_text,
    settle,
};
use merman3_core::serialize::serialize_atom;

const UNIT: f64 = 12. * 0.6;
const SYNTAX: &str = r##"{
  "background": "#333333",
  "display_unit": "px",
  "font_size": 16,
  "groups": [{"id": "any", "members": ["word"]}],
  "root": {
    "back": {
      "fixed_array": [
        {"id": {}},
        {"fixed_string": "call"},
        {"sub_array": {"id": "args", "element": "word"}}
      ]
    },
    "front": [
      {"symbol": {"text": {"text": "call("}}},
      {"array": {"field": "args", "separator": [{"text": {"text": ", "}}]}},
      {"symbol": {"text": {"text": ")"}}}
    ]
  },
  "types": [
    {
      "id": "word",
      "back": {"string": {"id": "text"}},
      "front": [{"primitive": {"field": "text"}}]
    }
  ]
}"##;
const AS_ATOM_SYNTAX: &str = r##"{
  "background": "#333333",
  "display_unit": "px",
  "font_size": 16,
  "groups": [{"id": "any", "members": ["word"]}],
  "root": {
    "back": {
      "fixed_record": [
        {"key": "only", "value": {"array": {"id": "only", "element": "word"}}}
      ]
    },
    "front": [
      {"symbol": {"text": {"text": "<"}}},
      {"array_as_atom": {"field": "only"}},
      {"symbol": {"text": {"text": ">"}}}
    ]
  },
  "types": [
    {
      "id": "word",
      "back": {"string": {"id": "text"}},
      "front": [{"primitive": {"field": "text"}}]
    }
  ]
}"##;

#[test]
fn sub_array_splices_into_the_enclosing_array() {
    let syntax = load_syntax(SYNTAX);
    let document = load_document(&syntax, r#"[7, "call", "a", "b", "c"]"#);
    let (mut ctx, display, _environment) = build(syntax, document, 600., 600.);
    settle(&mut ctx);
    assert_eq!(render_text(&display, UNIT), vec!["call(a, b, c)".to_string()]);
}

#[test]
fn sub_array_accepts_an_empty_run() {
    let syntax = load_syntax(SYNTAX);
    let document = load_document(&syntax, r#"[7, "call"]"#);
    let (mut ctx, display, _environment) = build(syntax, document, 600., 600.);
    settle(&mut ctx);
    assert_eq!(render_text(&display, UNIT), vec!["call()".to_string()]);
}

#[test]
fn sub_array_and_id_round_trip() {
    let syntax = load_syntax(SYNTAX);
    let source = r#"[7,"call","a","b","c"]"#;
    let document = load_document(&syntax, source);
    let written = serialize_atom(&syntax, &document, document.root);
    assert_eq!(serde_json::to_string(&written).unwrap(), source);
}

#[test]
fn array_as_atom_shows_the_first_element() {
    let syntax = load_syntax(AS_ATOM_SYNTAX);
    let document = load_document(&syntax, r#"{"only": ["x"]}"#);
    let (mut ctx, display, _environment) = build(syntax, document, 600., 600.);
    settle(&mut ctx);
    assert_eq!(render_text(&display, UNIT), vec!["<x>".to_string()]);
}

#[test]
fn array_as_atom_tolerates_an_empty_array() {
    let syntax = load_syntax(AS_ATOM_SYNTAX);
    let document = load_document(&syntax, r#"{"only": []}"#);
    let (mut ctx, display, _environment) = build(syntax, document, 600., 600.);
    settle(&mut ctx);
    assert_eq!(render_text(&display, UNIT), vec!["<>".to_string()]);
}

#[test]
fn two_sub_arrays_in_one_array_are_rejected() {
    let spec: merman3_core::spec::SpecSyntax = serde_json::from_str(r##"{
      "background": "#000",
      "display_unit": "px",
      "groups": [{"id": "any", "members": ["word"]}],
      "root": {
        "back": {
          "fixed_array": [
            {"sub_array": {"id": "a", "element": "word"}},
            {"sub_array": {"id": "b", "element": "word"}}
          ]
        },
        "front": []
      },
      "types": [{"id": "word", "back": {"string": {"id": "text"}}, "front": []}]
    }"##).unwrap();
    let errors = merman3_core::syntax::Syntax::syntax_resolve(spec).err().expect("should not resolve");
    assert!(
        errors.0.iter().any(|e| e.kind == merman3_core::error::ErrorKind::ArrayMultipleAtoms),
        "expected an ambiguity error, got {}",
        errors
    );
}

const DISCARD_SYNTAX: &str = r##"{
  "background": "#333333",
  "display_unit": "px",
  "font_size": 16,
  "root": {
    "back": {"fixed_record": [{"key": "ignored"}, {"key": "kept", "value": {"string": {"id": "text"}}}]},
    "front": [{"primitive": {"field": "text"}}]
  },
  "types": []
}"##;

#[test]
fn a_valueless_entry_is_read_and_left_out_when_written() {
    let syntax = load_syntax(DISCARD_SYNTAX);
    let document = load_document(&syntax, r#"{"ignored": {"anything": [1, 2]}, "kept": "hi"}"#);
    let (mut ctx, display, _environment) = build(syntax.clone(), document.clone(), 600., 600.);
    settle(&mut ctx);
    assert_eq!(render_text(&display, UNIT), vec!["hi".to_string()]);
    let written = serialize_atom(&syntax, &document, document.root);
    assert_eq!(serde_json::to_string(&written).unwrap(), r#"{"kept":"hi"}"#);
}
