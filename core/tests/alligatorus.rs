mod common;

use common::{
    build,
    load_document,
    load_syntax,
    render_text,
    settle,
};

#[test]
fn lays_out_synth_module() {
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let doc = load_document(&syntax, include_str!("data/synth-midi-sine.at"));
    let unit = syntax.syntax_style(0).font.size * 0.6;
    let edge_chars = 100.;
    let pad = syntax.spec_root.pad.converse_start + syntax.spec_root.pad.converse_end;
    let (mut ctx, display, _environment) = build(syntax, doc, edge_chars * unit + pad, 800.);
    settle(&mut ctx);
    let rows = render_text(&display, unit);
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
    return format!(r#"{{"id":{{"value":{}}},"variant":{{"{}":{}}}}}"#, id, variant, body);
}

fn number(id: u64, n: f64) -> String {
    return expr(id, "literal", &format!(r#"{{"value":{{"number":{{"value":{}}}}}}}"#, n));
}

fn sub(id: u64, base: &str, reference: &str) -> String {
    return expr(id, "operator_binary", &format!(r#"{{"op":"sub","base":{},"reference":{}}}"#, base, reference));
}

fn render_module(expr: &str) -> String {
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let doc = load_document(&syntax, &format!(r#"{{"v1":{{"expr":{}}}}}"#, expr));
    let (mut ctx, display, _environment) = build(syntax, doc, 2000., 800.);
    settle(&mut ctx);
    let rows = display.display_test_rows();
    assert_eq!(rows.len(), 1);
    return rows[0].bricks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().concat();
}

#[test]
fn parentheses_follow_associativity() {
    // Left associative: (1 - 2) - 3 needs no parentheses, 1 - (2 - 3) does.
    let left = sub(0, &sub(1, &number(2, 1.), &number(3, 2.)), &number(4, 3.));
    assert_eq!(render_module(&left), "1 - 2 - 3");
    let right = sub(0, &number(1, 1.), &sub(2, &number(3, 2.), &number(4, 3.)));
    assert_eq!(render_module(&right), "1 - (2 - 3)");
}

#[test]
fn ids_round_trip() {
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let source = include_str!("data/synth-midi-sine.at");
    let document = load_document(&syntax, source);
    let written = merman3_core::serialize::serialize_atom(&syntax, &document, document.root);
    let want: serde_json::Value = serde_json::from_str(source).unwrap();
    let mut diffs = vec![];
    first_difference(&written, &want, "", &mut diffs);
    for d in diffs.iter().take(8) {
        println!("{}", d);
    }
    assert!(diffs.is_empty(), "{} differences, first shown above", diffs.len());
}

fn first_difference(got: &serde_json::Value, want: &serde_json::Value, path: &str, out: &mut Vec<String>) -> bool {
    match (got, want) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for (k, v) in b {
                let Some(av) = a.get(k) else {
                    out.push(format!("{}/{} missing from output", path, k));
                    continue;
                };
                first_difference(av, v, &format!("{}/{}", path, k), out);
            }
            for k in a.keys() {
                if !b.contains_key(k) {
                    out.push(format!("{}/{} unexpected in output", path, k));
                }
            }
            return true;
        },
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{} length {} but wanted {}", path, a.len(), b.len()));
                return false;
            }
            for (i, (av, bv)) in a.iter().zip(b.iter()).enumerate() {
                first_difference(av, bv, &format!("{}/{}", path, i), out);
            }
            return true;
        },
        _ => {
            if got != want {
                out.push(format!("{} is {} but wanted {}", path, got, want));
                return false;
            }
            return true;
        },
    }
}
