//! Smoke test: the alligatorus example syntax lays out a real module.
use merman3_core::layout::{Layout, LayoutConfig};
use merman3_core::matcher::match_document;
use merman3_core::measure::MeasureFixed;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use merman3_core::visual::Visual;
use std::rc::Rc;

#[test]
fn lays_out_synth_module() {
    let spec: SpecSyntax =
        serde_json::from_str(include_str!("../../syntaxes/alligatorus.json")).expect("syntax json");
    let syntax = Rc::new(Syntax::syntax_resolve(spec).unwrap_or_else(|e| panic!("{}", e.join("\n"))));
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../../../ecosystem/synth-midi-sine.at")).unwrap();
    let doc = match_document(&syntax, &value).unwrap_or_else(|e| panic!("{}", e.mismatch_format()));
    let mut measure = MeasureFixed;
    let visual = Rc::new(Visual::visual_build(&syntax, &doc, &mut measure));
    let unit = syntax.syntax_style(0).font.size * 0.6;
    let edge_chars = 100.;
    let layout = Layout::layout_build(
        syntax.clone(),
        visual,
        LayoutConfig::default(),
        edge_chars * unit,
        &mut measure,
    );
    let rows = layout.layout_rows();
    let mut over = 0;
    for (i, row) in rows.rows.iter().enumerate() {
        let mut line = String::new();
        for b in &row.bricks {
            let col = (b.converse / unit).round() as usize;
            while line.chars().count() < col {
                line.push(' ');
            }
            line.push_str(&b.text);
        }
        // Wrapped lines may hang half a character over (nearest-index split).
        if line.trim_end().chars().count() as f64 > edge_chars + 1. {
            over += 1;
        }
        if i < 60 {
            println!("{}", line);
        }
    }
    println!("rows: {}, over edge: {}", rows.rows.len(), over);
    assert!(rows.rows.len() > 100);
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
    let spec: SpecSyntax =
        serde_json::from_str(include_str!("../../syntaxes/alligatorus.json")).expect("syntax json");
    let syntax = Rc::new(Syntax::syntax_resolve(spec).unwrap_or_else(|e| panic!("{}", e.join("\n"))));
    let value: serde_json::Value =
        serde_json::from_str(&format!(r#"{{"v1":{{"expr":{}}}}}"#, expr)).unwrap();
    let doc = match_document(&syntax, &value).unwrap_or_else(|e| panic!("{}", e.mismatch_format()));
    let mut measure = MeasureFixed;
    let visual = Rc::new(Visual::visual_build(&syntax, &doc, &mut measure));
    let layout = Layout::layout_build(syntax, visual, LayoutConfig::default(), 1000., &mut measure);
    let rows = layout.layout_rows();
    assert_eq!(rows.rows.len(), 1);
    return rows.rows[0].bricks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().concat();
}

#[test]
fn parentheses_follow_associativity() {
    // Left associative: (1 - 2) - 3 needs no parentheses, 1 - (2 - 3) does.
    let left = sub(0, &sub(1, &number(2, 1.), &number(3, 2.)), &number(4, 3.));
    assert_eq!(render_module(&left), "1 - 2 - 3");
    let right = sub(0, &number(1, 1.), &sub(2, &number(3, 2.), &number(4, 3.)));
    assert_eq!(render_module(&right), "1 - (2 - 3)");
}
