//! Smoke test: the alligatorus example syntax lays out a real module.
mod common;

use common::{build, load_document, load_syntax, render_text, settle, Clock};

#[test]
fn lays_out_synth_module() {
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let doc = load_document(&syntax, include_str!("../../../../ecosystem/synth-midi-sine.at"));
    let unit = syntax.syntax_style(0).font.size * 0.6;
    let edge_chars = 100.;
    let pad = syntax.spec_root.pad.converse_start + syntax.spec_root.pad.converse_end;
    let mut ctx = build(syntax, doc, edge_chars * unit + pad, 800.);
    let mut clock = Clock(0.);
    settle(&mut ctx, &mut clock);
    let rows = render_text(&ctx.render_snapshot(), unit);
    let mut over = 0;
    for (i, line) in rows.iter().enumerate() {
        // Wrapped lines may hang half a character over (nearest-index split).
        if line.trim_end().chars().count() as f64 > edge_chars + 1. {
            over += 1;
        }
        if i < 40 {
            println!("{}", line);
        }
    }
    println!("rows: {}, over edge: {}", rows.len(), over);
    assert!(rows.len() > 100);
    assert_eq!(over, 0, "{} rows exceed the edge", over);
}

fn expr(id: u64, variant: &str, body: &str) -> String {
    return format!(
        r#"{{"id":{{"value":{}}},"variant":{{"{}":{}}}}}"#,
        id, variant, body
    );
}

fn number(id: u64, n: f64) -> String {
    return expr(id, "literal", &format!(r#"{{"value":{{"number":{{"value":{}}}}}}}"#, n));
}

fn sub(id: u64, base: &str, reference: &str) -> String {
    return expr(
        id,
        "operator_binary",
        &format!(r#"{{"op":"sub","base":{},"reference":{}}}"#, base, reference),
    );
}

fn render_module(expr: &str) -> String {
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let doc = load_document(&syntax, &format!(r#"{{"v1":{{"expr":{}}}}}"#, expr));
    let mut ctx = build(syntax, doc, 2000., 800.);
    let mut clock = Clock(0.);
    settle(&mut ctx, &mut clock);
    let snapshot = ctx.render_snapshot();
    assert_eq!(snapshot.rows.len(), 1);
    return snapshot.rows[0].bricks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().concat();
}

#[test]
fn parentheses_follow_associativity() {
    // Left associative: (1 - 2) - 3 needs no parentheses, 1 - (2 - 3) does.
    let left = sub(0, &sub(1, &number(2, 1.), &number(3, 2.)), &number(4, 3.));
    assert_eq!(render_module(&left), "1 - 2 - 3");
    let right = sub(0, &number(1, 1.), &sub(2, &number(3, 2.), &number(4, 3.)));
    assert_eq!(render_module(&right), "1 - (2 - 3)");
}
