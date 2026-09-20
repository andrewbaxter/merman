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
use merman3_core::direction::{
    DirectionConvert,
    DirectionKey,
};
use merman3_core::keys::{
    KeyName,
    SpecBinding,
    KeyStroke,
    Keymap,
    SpecKeys,
};
use merman3_core::spec::SpecDirection;
use std::collections::HashMap;

const SOURCE: &str = r#"{"a": 1, "b": [true, null]}"#;
const UNIT: f64 = 12. * 0.6;

fn json_context(keys: Keymap) -> (Context, Clock) {
    let syntax = load_syntax(include_str!("../../syntaxes/json.json"));
    let document = load_document(&syntax, SOURCE);
    let mut ctx = build(syntax, document, 600., 600.);
    ctx.config.keys = keys;
    let mut clock = Clock(0.);
    settle(&mut ctx, &mut clock);
    return (ctx, clock);
}

fn press(ctx: &mut Context, clock: &mut Clock, key: KeyName, shift: bool) -> bool {
    let mut stroke = KeyStroke::key_stroke_new(key);
    stroke.shift = shift;
    let handled = ctx.key_press(stroke, &mut || clock.now());
    settle(ctx, clock);
    return handled;
}

fn press_char(ctx: &mut Context, clock: &mut Clock, c: char) -> bool {
    return press(ctx, clock, KeyName::Char(c), false);
}

fn parse_binding(json: &str) -> SpecBinding {
    return serde_json::from_str(json).unwrap();
}

fn path(ctx: &Context) -> Vec<String> {
    return ctx.cursor_syntax_path(ctx.cursor.expect("nothing is selected"));
}

#[test]
fn arrows_follow_the_layout_direction() {
    let convert = DirectionConvert::new(SpecDirection::Right, SpecDirection::Down);
    assert_eq!(convert.direction_convert_cardinal(SpecDirection::Right), DirectionKey::Dive);
    assert_eq!(convert.direction_convert_cardinal(SpecDirection::Left), DirectionKey::Surface);
    assert_eq!(convert.direction_convert_cardinal(SpecDirection::Down), DirectionKey::Next);
    assert_eq!(convert.direction_convert_cardinal(SpecDirection::Up), DirectionKey::Previous);
    let convert = DirectionConvert::new(SpecDirection::Down, SpecDirection::Left);
    assert_eq!(convert.direction_convert_cardinal(SpecDirection::Down), DirectionKey::Dive);
    assert_eq!(convert.direction_convert_cardinal(SpecDirection::Left), DirectionKey::Next);
    assert_eq!(convert.direction_convert_cardinal(SpecDirection::Right), DirectionKey::Previous);
}

#[test]
fn first_press_selects_the_root() {
    let (mut ctx, mut clock) = json_context(Keymap::default());
    assert!(ctx.cursor.is_none());
    assert!(press(&mut ctx, &mut clock, KeyName::Next, false));
    assert_eq!(path(&ctx), vec!["named", "value"]);
}

#[test]
fn moves_through_an_array_and_into_atoms() {
    let (mut ctx, mut clock) = json_context(Keymap::default());
    press_char(&mut ctx, &mut clock, 'j');
    assert_eq!(path(&ctx), vec!["named", "value"]);
    press_char(&mut ctx, &mut clock, 'l');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
    press_char(&mut ctx, &mut clock, 'j');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "1"]);
    press_char(&mut ctx, &mut clock, 'k');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
    press_char(&mut ctx, &mut clock, 'u');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "1"]);
    press_char(&mut ctx, &mut clock, 'i');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
    press_char(&mut ctx, &mut clock, 'l');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key"]);
    press_char(&mut ctx, &mut clock, 'j');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "value"]);
    press_char(&mut ctx, &mut clock, 'h');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
    press_char(&mut ctx, &mut clock, 'h');
    assert_eq!(path(&ctx), vec!["named", "value"]);
    assert!(!press_char(&mut ctx, &mut clock, 'h'), "the root has nothing to exit to");
}

#[test]
fn selects_a_range_of_elements_and_copies_it() {
    let (mut ctx, mut clock) = json_context(Keymap::default());
    for key in ['j', 'l', 'j', 'l', 'j', 'l'] {
        press_char(&mut ctx, &mut clock, key);
    }
    assert_eq!(
        path(&ctx),
        vec!["named", "value", "named", "entries", "1", "named", "value", "named", "elements", "0"]
    );
    assert!(press(&mut ctx, &mut clock, KeyName::Char('j'), true));
    press_char(&mut ctx, &mut clock, 'y');
    assert_eq!(ctx.clipboard.take().unwrap(), "[\n  true,\n  null\n]");
    assert!(press(&mut ctx, &mut clock, KeyName::Char('k'), true));
    press_char(&mut ctx, &mut clock, 'y');
    assert_eq!(ctx.clipboard.take().unwrap(), "[\n  true\n]");
}

#[test]
fn moves_through_text_by_glyph_and_word() {
    let (mut ctx, mut clock) = json_context(Keymap::default());
    for key in ['j', 'l', 'l', 'l'] {
        press_char(&mut ctx, &mut clock, key);
    }
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key", "1"]);
    assert!(press_char(&mut ctx, &mut clock, 'h'));
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key", "0"]);
    assert!(!press_char(&mut ctx, &mut clock, 'h'), "already at the start of the text");
    assert!(press_char(&mut ctx, &mut clock, 'w'));
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key", "1"]);
    assert!(press_char(&mut ctx, &mut clock, 'b'));
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key", "0"]);
    assert!(press(&mut ctx, &mut clock, KeyName::Char('l'), true));
    press_char(&mut ctx, &mut clock, 'y');
    assert_eq!(ctx.clipboard.take().unwrap(), "a");
    assert!(press(&mut ctx, &mut clock, KeyName::Escape, false));
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key"]);
}

#[test]
fn configured_bindings_replace_the_defaults() {
    let keys =
        SpecKeys(
            HashMap::from(
                [
                    ("next".to_string(), vec![parse_binding(r#"{"key": "n"}"#)]),
                    ("enter".to_string(), vec![parse_binding(r#"[{"key": "g"}, {"key": "l"}]"#)]),
                ],
            ),
        );
    let (mut ctx, mut clock) = json_context(Keymap::keymap_resolve(&keys).unwrap());
    assert!(!press_char(&mut ctx, &mut clock, 'j'), "`j` was replaced by `n`");
    assert!(press_char(&mut ctx, &mut clock, 'n'));
    assert_eq!(path(&ctx), vec!["named", "value"]);
    assert!(press_char(&mut ctx, &mut clock, 'g'));
    assert_eq!(path(&ctx), vec!["named", "value"]);
    assert!(press_char(&mut ctx, &mut clock, 'l'));
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
}

#[test]
fn unusable_bindings_are_reported() {
    let bad =
        SpecKeys(
            HashMap::from(
                [
                    ("nxt".to_string(), vec![parse_binding(r#"{"key": "n"}"#)]),
                    ("previous".to_string(), vec![parse_binding(r#"{"key": "escape"}"#)]),
                    ("copy".to_string(), vec![parse_binding(r#"{"key": "g"}"#)]),
                    ("first".to_string(), vec![parse_binding(r#"[{"key": "g"}, {"key": "g"}]"#)]),
                ],
            ),
        );
    let errors = Keymap::keymap_resolve(&bad).err().expect("should fail");
    assert_eq!(errors.len(), 3, "{:?}", errors);
    assert!(errors.iter().any(|e| e.contains("Unknown action `nxt`")), "{:?}", errors);
    assert!(errors.iter().any(|e| e.contains("bound to both")), "{:?}", errors);
    assert!(errors.iter().any(|e| e.contains("is the start of")), "{:?}", errors);
}

#[test]
fn bad_bindings_are_rejected_when_read() {
    for (
        bad,
        message,
    ) in [
        (r#"{"key": "nope"}"#, "unknown key `nope`"),
        (r#"{"key": "j", "control": true}"#, "unknown field `control`"),
        (r#"{"ctrl": true}"#, "missing field `key`"),
    ] {
        let e = serde_json::from_str::<SpecBinding>(bad).err().expect("should fail");
        assert!(e.to_string().contains(message), "{} -> {}", bad, e);
    }
}

#[test]
fn hover_resolves_during_the_move() {
    let (mut ctx, mut clock) = json_context(Keymap::default());
    let snapshot = ctx.render_snapshot();
    assert_eq!(render_text(&snapshot, UNIT), vec!["{a: 1, b: [true, null]}".to_string()]);
    let row = &snapshot.rows[0];
    let brick = row.bricks.iter().find(|b| b.text == "true").unwrap();
    ctx.mouse_moved(Vector::new(brick.converse + 1., row.transverse + 1.), &mut || clock.now());
    assert!(ctx.hover.is_some());
    ctx.mouse_exited();
    assert!(ctx.hover.is_none());
}
