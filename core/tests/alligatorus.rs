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
        context::Vector,
        keys::Action,
        reference::Reference,
    },
    std::time::Instant,
};

fn bind(id: u64, name: &str, value: &str) -> String {
    return expr(id, "bind", &format!(r#"{{"name":{},"value":{}}}"#, name, value));
}

#[test]
fn brackets_mark_tuples_records_and_sequences() {
    assert_eq!(render_module(&expr(0, "tuple", &format!(r#"[{},{}]"#, number(1, 1.), number(2, 2.)))), "(1, 2)");
    assert_eq!(
        render_module(&expr(0, "record", &format!(r#"[{{"key":{},"value":{}}}]"#, string(1, "x"), number(2, 1.)))),
        "{x: 1}"
    );
    assert_eq!(render_module(&expr(0, "seq", &format!(r#"[{},{}]"#, number(1, 1.), number(2, 2.)))), "[ 1; 2; ]");
    assert_eq!(render_module(&expr(0, "stage", &scope_use(1, &string(2, "x")))), "#x");
}

fn call(id: u64, func: &str, arg: &str) -> String {
    return expr(id, "call", &format!(r#"{{"func":{},"arg":{}}}"#, func, arg));
}

#[test]
fn calls_are_juxtaposed() {
    let f = |id: u64| scope_use(id, &string(id + 1, "f"));
    let x = |id: u64| scope_use(id, &string(id + 1, "x"));
    assert_eq!(render_module(&call(0, &f(1), &x(3))), "f x");
    assert_eq!(render_module(&sub(0, &call(1, &f(2), &x(4)), &number(6, 1.))), "f x - 1");
    assert_eq!(render_module(&call(0, &f(1), &sub(3, &number(4, 1.), &number(5, 2.)))), "f (1 - 2)");
    assert_eq!(render_module(&call(0, &f(1), &call(3, &f(4), &x(6)))), "f (f x)");
    assert_eq!(render_module(&call(0, &call(1, &f(2), &x(4)), &x(6))), "(f x) x");
}

fn expr(id: u64, variant: &str, body: &str) -> String {
    return format!(r#"{{"id":{},"variant":{{"{}":{}}}}}"#, id, variant, body);
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

#[test]
fn identifiers_are_bare_only_when_unambiguous() {
    assert_eq!(render_module(&bind(0, &string(1, "x"), &number(2, 1.))), "x := 1");
    assert_eq!(render_module(&bind(0, &string(1, "a b"), &number(2, 1.))), "expr \"a b\" := 1");
    assert_eq!(render_module(&scope_use(0, &string(1, "x"))), "x");
    assert_eq!(render_module(&scope_use(0, &string(1, ""))), "expr \"\"");
    assert_eq!(render_module(&scope_use(0, &scope_use(1, &string(2, "k")))), "expr k");
    assert_eq!(render_module(&scope_use(0, &sub(1, &number(2, 1.), &number(3, 2.)))), "expr (1 - 2)");
}

#[test]
fn ids_round_trip() {
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let source = include_str!("data/synth-midi-sine.at");
    let document = load_document(&syntax, source);
    let written = merman_core::serialize::serialize_atom(&syntax, &document, document.root);
    let want: serde_json::Value = serde_json::from_str(source).unwrap();
    let mut diffs = vec![];
    first_difference(&written, &want, "", &mut diffs);
    for d in diffs.iter().take(8) {
        println!("{}", d);
    }
    assert!(diffs.is_empty(), "{} differences, first shown above", diffs.len());
}

#[test]
fn a_selected_expression_is_referenced_by_its_own_id() {
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let doc = load_document(&syntax, &format!(r#"{{"v1":[{}]}}"#, sub(5, &number(6, 1.), &number(7, 2.))));
    let (mut ctx, _display, _environment) = build(syntax, doc, 2000., 800.);
    settle(&mut ctx);
    assert!(ctx.cursor_select_reference(&Reference::reference_parse("#.v1[0]").unwrap()));
    settle(&mut ctx);
    assert_eq!(ctx.cursor_reference().unwrap().reference_format(), "#5");
    assert!(ctx.cursor_select_reference(&Reference::reference_parse("#5.variant.operator_binary.base").unwrap()));
    settle(&mut ctx);
    assert_eq!(ctx.cursor_reference().unwrap().reference_format(), "#6");
}

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
    assert!(rows.len() > 50);
    assert_eq!(over, 0, "{} rows exceed the edge", over);
}

fn number(id: u64, n: f64) -> String {
    return expr(id, "literal", &format!(r#"{{"number":{}}}"#, n));
}

#[test]
fn parentheses_follow_associativity() {
    // Left associative: (1 - 2) - 3 needs no parentheses, 1 - (2 - 3) does.
    let left = sub(0, &sub(1, &number(2, 1.), &number(3, 2.)), &number(4, 3.));
    assert_eq!(render_module(&left), "1 - 2 - 3");
    let right = sub(0, &number(1, 1.), &sub(2, &number(3, 2.), &number(4, 3.)));
    assert_eq!(render_module(&right), "1 - (2 - 3)");
}

fn profile(name: &str, source: &str) {
    println!("\n--- {} ({} bytes)", name, source.len());
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let document = load_document(&syntax, source);
    let start = Instant::now();
    let (mut ctx, display, _environment) = build(syntax, document.clone(), 1200., 800.);
    let build = start.elapsed();
    let lay = Instant::now();
    settle(&mut ctx);
    let lay = lay.elapsed();
    println!("atoms {} | context_new {:?} | settle {:?}", document.atoms.len(), build, lay);
    println!(
        "visuals {} | bricks {} | courses {} | wall transverse {:.0} vs 800 viewport",
        ctx.visuals.len(),
        ctx.bricks.iter().filter(|b| b.alive).count(),
        ctx.wall.children.len(),
        ctx.courses.last().map(|c| c.transverse_start).unwrap_or(0.)
    );
    let mut worst = std::time::Duration::ZERO;
    let mut total = std::time::Duration::ZERO;
    let mut dives = 0;
    loop {
        let press = Instant::now();
        let handled = ctx.key_action(Action::Enter);
        if handled {
            ctx.input_flush();
        }
        settle(&mut ctx);
        let press = press.elapsed();
        if !handled {
            break;
        }
        total += press;
        worst = worst.max(press);
        dives += 1;
        assert!(dives < 1000, "diving never bottomed out");
    }
    println!("enter x{} to the deepest leaf: total {:?} | worst {:?}", dives, total, worst);
    let rows = display.display_test_rows();
    let mut worst = std::time::Duration::ZERO;
    for row in [rows.len() - 1, 0, rows.len() / 2] {
        let row = &rows[row];
        let hover = Instant::now();
        ctx.mouse_moved(Vector::new(row.bricks[0].converse + 1., row.transverse + 1.));
        settle(&mut ctx);
        worst = worst.max(hover.elapsed());
    }
    println!("hover across the wall ({} rows): worst {:?}", rows.len(), worst);
}

#[test]
#[ignore]
fn profile_large_document() {
    let source = include_str!("data/synth-midi-sine.at");
    profile("synth-midi-sine", source);
    profile("x4", &repeated(source, 4));
    profile("x16", &repeated(source, 16));
}

fn render_module(expr: &str) -> String {
    let syntax = load_syntax(include_str!("../../syntaxes/alligatorus.json"));
    let doc = load_document(&syntax, &format!(r#"{{"v1":[{}]}}"#, expr));
    let (mut ctx, display, _environment) = build(syntax, doc, 2000., 800.);
    settle(&mut ctx);
    let rows = display.display_test_rows();
    assert_eq!(rows.len(), 1);
    let text = rows[0].bricks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().concat();
    return text.strip_suffix("; ").unwrap_or(&text).to_string();
}

fn repeated(source: &str, times: usize) -> String {
    let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
    let exprs = value["v1"].as_array().unwrap().clone();
    let mut grown = vec![];
    for _ in 0 .. times {
        grown.extend(exprs.iter().cloned());
    }
    value["v1"] = serde_json::Value::Array(grown);
    return serde_json::to_string(&value).unwrap();
}

fn scope_use(id: u64, key: &str) -> String {
    return expr(id, "scope_use", key);
}

fn string(id: u64, s: &str) -> String {
    return expr(id, "literal", &format!(r#"{{"str":{}}}"#, serde_json::to_string(s).unwrap()));
}

fn sub(id: u64, base: &str, reference: &str) -> String {
    return expr(id, "operator_binary", &format!(r#"{{"op":"sub","base":{},"reference":{}}}"#, base, reference));
}
