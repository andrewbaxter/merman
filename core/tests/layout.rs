mod common;

use {
    common::{
        build,
        load_document,
        load_syntax,
        render_text,
        settle,
    },
    merman_core::{
        display::PIXELS_PER_MM,
        context::{
            Context,
            Vector,
        },
        display::DisplayTest,
        environment::EnvironmentTest,
        error::ErrorKind,
        matcher::match_document,
        spec::SpecSyntax,
        syntax::Syntax,
    },
};

const UNIT: f64 = 4. * PIXELS_PER_MM * 0.6;

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
    let (mut ctx, display, _environment) = json_context(r#"{"a": 1, "b": [true, null]}"#, 19.);
    assert_eq!(render_text(&display, UNIT).len(), 4);
    resize(&mut ctx, 100.);
    assert_eq!(render_text(&display, UNIT), vec!["{a: 1, b: [true, null]}".to_string()]);
    resize(&mut ctx, 12.);
    assert_eq!(
        render_text(&display, UNIT),
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
    resize(&mut ctx, 13.);
    assert_eq!(render_text(&display, UNIT).len(), 7);
    resize(&mut ctx, 20.);
    assert_eq!(
        render_text(&display, UNIT),
        vec!["{".to_string(), "    a: 1, ".to_string(), "    b: [true, null]".to_string(), "}".to_string(),]
    );
}

#[test]
fn hover_click_and_copy() {
    let (mut ctx, display, environment) = json_context(r#"{"a": 1, "b": [true, null]}"#, 100.);
    let rows = display.display_test_rows();
    let row = &rows[0];
    let true_brick = row.bricks.iter().find(|b| b.text == "true").unwrap();
    let pad = ctx.syntax.spec_root.pad.converse_start;
    ctx.mouse_moved(Vector::new(true_brick.converse + 1. + pad, row.transverse + 1.));
    settle(&mut ctx);
    assert!(ctx.hover.is_some(), "hovering a brick should produce a hoverable");
    assert_eq!(display.display_test_drawings(), 1, "hover draws one border");
    let drawn = display.display_test_drawn();
    let border = &drawn[0];
    assert!(
        border.size.converse > 0. && border.size.transverse > 0.,
        "the border is sized to what it draws, or a bitmap display clips it away"
    );
    assert!(
        border.transverse >= row.transverse - 20. && border.transverse <= row.transverse + 20.,
        "the border is placed at the row it marks, not at the origin: {} vs {}",
        border.transverse,
        row.transverse
    );
    assert!(ctx.mouse_button(true,));
    ctx.mouse_button(false);
    settle(&mut ctx);
    assert!(ctx.cursor.is_some());
    assert_eq!(
        ctx.cursor_syntax_path(ctx.cursor.unwrap()),
        vec!["named", "value", "named", "entries", "1", "named", "value", "named", "elements", "0"]
    );
    ctx.key_copy();
    assert_eq!(environment.0.borrow_mut().clipboard.take().unwrap(), "[\n  true\n]");
    let brace = row.bricks.iter().find(|b| b.text == "{").unwrap();
    ctx.mouse_moved(Vector::new(brace.converse + 1. + pad, row.transverse + 1.));
    settle(&mut ctx);
    assert_eq!(display.display_test_rows()[0].transverse, 0.);
    assert_eq!(display.display_test_drawings(), 2);
}

fn json_context(text: &str, edge_chars: f64) -> (Context, DisplayTest, EnvironmentTest) {
    let syntax = load_syntax(include_str!("../../syntaxes/json.json"));
    let doc = load_document(&syntax, text);
    let pad = syntax.spec_root.pad.converse_start + syntax.spec_root.pad.converse_end;
    let (mut ctx, display, environment) = build(syntax, doc, (edge_chars + 0.001) * UNIT + pad, 600.);
    settle(&mut ctx);
    return (ctx, display, environment);
}

fn layout_text(text: &str, edge_chars: f64) -> Vec<String> {
    let (_ctx, display, _environment) = json_context(text, edge_chars);
    return render_text(&display, UNIT);
}

#[test]
fn a_document_the_root_does_not_match_is_read_as_json() {
    let spec: SpecSyntax = serde_json::from_str(r##"{
          "root": {"back": {"fixed_record": [{"key": "v1", "value": {"fixed_literal": "true"}}]}, "front": []},
          "types": []
        }"##).unwrap();
    let syntax = Syntax::syntax_resolve(spec, &common::theme()).unwrap_or_else(|e| panic!("{}", e));
    let value: serde_json::Value = serde_json::from_str(r#"{"v1": false}"#).unwrap();
    let document = match_document(&syntax, &value).unwrap_or_else(|e| panic!("{}", e.mismatch_format()));
    let types: Vec<&str> = document.atoms.iter().map(|a| syntax.syntax_type(a.type_).id.as_str()).collect();
    assert_eq!(types, vec!["__json_root", "__json_object", "__json_pair", "__json_false"]);
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

fn resize(ctx: &mut Context, edge_chars: f64) {
    let pad = ctx.syntax.spec_root.pad.converse_start + ctx.syntax.spec_root.pad.converse_end;
    ctx.context_resize((edge_chars + 0.001) * UNIT + pad, 600.);
    settle(ctx);
}

#[test]
fn single_line_fits() {
    let rows = layout_text(r#"{"a": 1, "b": [true, null]}"#, 100.);
    assert_eq!(rows, vec!["{a: 1, b: [true, null]}".to_string()]);
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
fn syntax_validation_reports_unused_field() {
    let spec: SpecSyntax = serde_json::from_str(r##"{
          "root": {"back": {"string": {"id": "x"}}, "front": []},
          "types": []
        }"##).unwrap();
    let errors = Syntax::syntax_resolve(spec, &common::theme()).err().expect("should fail");
    assert_eq!(errors.0.len(), 1, "{}", errors);
    assert_eq!(errors.0[0].kind, ErrorKind::UnusedBackData { unused: "x".to_string() }, "{}", errors);
}

#[test]
fn unwraps_string_when_edge_grows() {
    let (mut ctx, display, _environment) = json_context(r#"["the quick brown fox jumps over the lazy dog"]"#, 20.);
    assert_eq!(render_text(&display, UNIT).len(), 5);
    resize(&mut ctx, 100.);
    assert_eq!(render_text(&display, UNIT), vec!["[\"the quick brown fox jumps over the lazy dog\"]".to_string()]);
    resize(&mut ctx, 30.);
    assert_eq!(
        render_text(&display, UNIT),
        vec![
            "[".to_string(),
            "    \"the quick brown fox jumps ".to_string(),
            "    over the lazy dog\"".to_string(),
            "]".to_string(),
        ]
    );
}
