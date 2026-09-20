mod common;

use common::{
    build,
    load_document,
    load_syntax,
    render_text,
    settle,
    Clock,
};
use merman3_core::context::{
    Context,
    Vector,
};
use merman3_core::matcher::match_document;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;

/// The json syntax uses a 16px font, which merman treats as 16pt = 12px; the fixed
/// measurer makes every grapheme 0.6em, so 7.2px, and the 28.8px indent is 4
/// characters.
const UNIT: f64 = 12. * 0.6;

fn json_context(text: &str, edge_chars: f64) -> (Context, Clock) {
    let syntax = load_syntax(include_str!("../../syntaxes/json.json"));
    let doc = load_document(&syntax, text);
    let pad = syntax.spec_root.pad.converse_start + syntax.spec_root.pad.converse_end;
    let mut ctx = build(syntax, doc, edge_chars * UNIT + pad, 600.);
    let mut clock = Clock(0.);
    settle(&mut ctx, &mut clock);
    return (ctx, clock);
}

fn layout_text(text: &str, edge_chars: f64) -> Vec<String> {
    let (ctx, _) = json_context(text, edge_chars);
    return render_text(&ctx.render_snapshot(), UNIT);
}

fn resize(ctx: &mut Context, clock: &mut Clock, edge_chars: f64) {
    let pad = ctx.syntax.spec_root.pad.converse_start + ctx.syntax.spec_root.pad.converse_end;
    ctx.context_resize(edge_chars * UNIT + pad, 600.);
    settle(ctx, clock);
}

#[test]
fn single_line_fits() {
    let rows = layout_text(r#"{"a": 1, "b": [true, null]}"#, 100.);
    assert_eq!(rows, vec!["{a: 1, b: [true, null]}".to_string()]);
}

#[test]
fn compacts_outer_first() {
    let rows = layout_text(r#"{"a": 1, "b": [true, null]}"#, 19.);
    assert_eq!(
        rows,
        vec!["{".to_string(), "    a: 1, ".to_string(), "    b: [true, null]".to_string(), "}".to_string(),]
    );
}

#[test]
fn expands_when_edge_grows_and_recompacts() {
    let (mut ctx, mut clock) = json_context(r#"{"a": 1, "b": [true, null]}"#, 19.);
    assert_eq!(render_text(&ctx.render_snapshot(), UNIT).len(), 4);
    resize(&mut ctx, &mut clock, 100.);
    assert_eq!(render_text(&ctx.render_snapshot(), UNIT), vec!["{a: 1, b: [true, null]}".to_string()]);
    resize(&mut ctx, &mut clock, 12.);
    assert_eq!(
        render_text(&ctx.render_snapshot(), UNIT),
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
    resize(&mut ctx, &mut clock, 13.);
    assert_eq!(render_text(&ctx.render_snapshot(), UNIT).len(), 7);
    resize(&mut ctx, &mut clock, 20.);
    assert_eq!(
        render_text(&ctx.render_snapshot(), UNIT),
        vec!["{".to_string(), "    a: 1, ".to_string(), "    b: [true, null]".to_string(), "}".to_string(),]
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

    // The record (lowest depth) breaks; the array still fits on its line.
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
fn unwraps_string_when_edge_grows() {
    let (mut ctx, mut clock) = json_context(r#"["the quick brown fox jumps over the lazy dog"]"#, 20.);
    assert_eq!(render_text(&ctx.render_snapshot(), UNIT).len(), 5);
    resize(&mut ctx, &mut clock, 100.);
    assert_eq!(
        render_text(&ctx.render_snapshot(), UNIT),
        vec!["[\"the quick brown fox jumps over the lazy dog\"]".to_string()]
    );
    resize(&mut ctx, &mut clock, 30.);
    assert_eq!(
        render_text(&ctx.render_snapshot(), UNIT),
        vec![
            "[".to_string(),
            "    \"the quick brown fox jumps ".to_string(),
            "    over the lazy dog\"".to_string(),
            "]".to_string(),
        ]
    );
}

#[test]
fn hover_click_and_copy() {
    let (mut ctx, mut clock) = json_context(r#"{"a": 1, "b": [true, null]}"#, 100.);
    let snapshot = ctx.render_snapshot();
    let row = &snapshot.rows[0];
    let true_brick = row.bricks.iter().find(|b| b.text == "true").unwrap();
    let point = Vector::new(true_brick.converse + 1., row.transverse + 1.);
    ctx.mouse_moved(point);
    settle(&mut ctx, &mut clock);
    assert!(ctx.hover.is_some(), "hovering a brick should produce a hoverable");
    assert_eq!(ctx.render_snapshot().drawings.len(), 1, "hover draws one border");
    assert!(ctx.mouse_button(true, &mut || clock.now()));
    ctx.mouse_button(false, &mut || clock.now());
    settle(&mut ctx, &mut clock);
    assert!(ctx.cursor.is_some());
    assert_eq!(
        ctx.cursor_syntax_path(ctx.cursor.unwrap()),
        vec!["named", "value", "named", "entries", "1", "named", "value", "named", "elements", "0"]
    );
    ctx.key_copy(&mut || clock.now());
    assert_eq!(ctx.clipboard.take().unwrap(), "[\n  true\n]");
    let brace = row.bricks.iter().find(|b| b.text == "{").unwrap();
    ctx.mouse_moved(Vector::new(brace.converse + 1., row.transverse + 1.));
    settle(&mut ctx, &mut clock);
    let snapshot = ctx.render_snapshot();
    assert_eq!(snapshot.transverse.0, 0.);
    assert_eq!(snapshot.drawings.len(), 2);
}

#[test]
fn mismatch_reports_alternatives() {
    let spec: SpecSyntax = serde_json::from_str(r##"{
          "background": "#000",
          "display_unit": "px",
          "root": {"back": {"fixed_record": [{"key": "v1", "value": {"fixed_literal": "true"}}]}, "front": []},
          "types": []
        }"##).unwrap();
    let syntax = Syntax::syntax_resolve(spec).unwrap_or_else(|e| panic!("{}", e.join("\n")));
    let value: serde_json::Value = serde_json::from_str(r#"{"v1": false}"#).unwrap();
    let err = match_document(&syntax, &value).err().expect("should mismatch");
    assert_eq!(err.mismatch_format().trim(), "/v1: expected literal true, got false");
}

#[test]
fn syntax_validation_reports_unused_field() {
    let spec: SpecSyntax = serde_json::from_str(r##"{
          "background": "#000",
          "display_unit": "px",
          "root": {"back": {"string": {"id": "x"}}, "front": []},
          "types": []
        }"##).unwrap();
    let errors = Syntax::syntax_resolve(spec).err().expect("should fail");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("field `x` is captured"), "{}", errors[0]);
}
