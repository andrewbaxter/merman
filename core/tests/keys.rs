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
        direction::{
            DirectionConvert,
            DirectionKey,
        },
        display::DisplayTest,
        environment::EnvironmentTest,
        error::ErrorKind,
        keys::{
            Action,
            KeyName,
            KeyResolve,
            KeyStroke,
            Keymap,
            SpecBinding,
            SpecKeys,
        },
        reference::Reference,
        spec::SpecDirection,
    },
    std::collections::HashMap,
};

const SOURCE: &str = r#"{"a": 1, "b": [true, null]}"#;
const UNIT: f64 = 4. * PIXELS_PER_MM * 0.6;

#[test]
fn a_reference_is_selected_or_its_nearest_ancestor() {
    let (mut ctx, _display, _environment) = json_context(Keymap::default());
    for key in ['j', 'l', 'l', 'l'] {
        press_char(&mut ctx, key);
    }
    let text = ctx.cursor_reference().unwrap();
    assert_eq!(text.reference_format(), "#.a[1:1]");
    ctx.clear_cursor();
    assert!(ctx.cursor_select_reference(&text), "a key has no path of its own, so its entry's value is selected");
    settle(&mut ctx);
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "value", "named", "value", "1"]);
    let element = Reference::reference_parse("#.b[1]").unwrap();
    assert!(ctx.cursor_select_reference(&element));
    settle(&mut ctx);
    assert_eq!(
        path(&ctx),
        vec!["named", "value", "named", "entries", "1", "named", "value", "named", "elements", "1"]
    );
    assert_eq!(ctx.cursor_reference().unwrap().reference_format(), "#.b[1]");
    let missing = Reference::reference_parse("#.b[7].nothing").unwrap();
    assert!(ctx.cursor_select_reference(&missing));
    settle(&mut ctx);
    assert_eq!(ctx.cursor_reference().unwrap().reference_format(), "#.b");
    let both = Reference::reference_parse("#.b[0:2]").unwrap();
    assert!(ctx.cursor_select_reference(&both));
    settle(&mut ctx);
    assert_eq!(ctx.cursor_reference().unwrap().reference_format(), "#.b[0:2]");
    assert!(ctx.cursor_select_reference(&Reference::reference_parse("#.").unwrap()));
    settle(&mut ctx);
    assert_eq!(ctx.cursor_reference().unwrap().reference_format(), "#.");
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
fn configured_bindings_replace_the_defaults() {
    let keys = SpecKeys {
        atom: HashMap::from(
            [
                ("next_element".to_string(), vec![parse_binding(r#"{"key": "n"}"#)]),
                ("enter".to_string(), vec![parse_binding(r#"[{"key": "g"}, {"key": "l"}]"#)]),
            ],
        ),
        ..SpecKeys::default()
    };
    let (mut ctx, _display, _environment) = json_context(Keymap::keymap_resolve(&keys).unwrap());
    assert!(press_char(&mut ctx, 'n'));
    assert_eq!(path(&ctx), vec!["named", "value"]);
    assert!(!press_char(&mut ctx, 'j'), "`j` was replaced by `n`");
    assert!(press_char(&mut ctx, 'g'));
    assert_eq!(path(&ctx), vec!["named", "value"]);
    assert!(press_char(&mut ctx, 'l'));
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
}

#[test]
fn first_press_selects_the_root() {
    let (mut ctx, _display, _environment) = json_context(Keymap::default());
    assert!(ctx.cursor.is_none());
    assert!(press(&mut ctx, KeyName::Next, false));
    assert_eq!(path(&ctx), vec!["named", "value"]);
}

#[test]
fn function_keys_and_insert_can_be_bound() {
    for (
        text,
        want,
    ) in [("f1", KeyName::Function(1)), ("f24", KeyName::Function(24)), ("insert", KeyName::Insert)] {
        let binding = parse_binding(&format!(r#"{{"key": {:?}}}"#, text));
        let SpecBinding::Stroke(stroke) = binding else {
            panic!("expected a single stroke");
        };
        assert_eq!(stroke.key, want);
        assert_eq!(serde_json::to_string(&stroke.key).unwrap(), format!("{:?}", text));
    }
}

#[test]
fn hover_resolves_during_the_move() {
    let (mut ctx, display, _environment) = json_context(Keymap::default());
    assert_eq!(render_text(&display, UNIT), vec!["{a: 1, b: [true, null]}".to_string()]);
    let rows = display.display_test_rows();
    let row = &rows[0];
    let brick = row.bricks.iter().find(|b| b.text == "true").unwrap();
    let pad = ctx.syntax.spec_root.pad.converse_start;
    ctx.mouse_moved(Vector::new(brick.converse + 1. + pad, row.transverse + 1.));
    settle(&mut ctx);
    assert!(ctx.hover.is_some());
    ctx.mouse_exited();
    assert!(ctx.hover.is_none());
}

fn json_context(keys: Keymap) -> (Context, DisplayTest, EnvironmentTest) {
    let syntax = load_syntax(include_str!("../../syntaxes/json.json"));
    let document = load_document(&syntax, SOURCE);
    let (mut ctx, display, environment) = build(syntax, document, 600., 600.);
    ctx.config.keys = keys;
    settle(&mut ctx);
    return (ctx, display, environment);
}

#[test]
fn moves_through_an_array_and_into_atoms() {
    let (mut ctx, _display, _environment) = json_context(Keymap::default());
    press_char(&mut ctx, 'j');
    assert_eq!(path(&ctx), vec!["named", "value"]);
    press_char(&mut ctx, 'l');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
    press_char(&mut ctx, 'j');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "1"]);
    press_char(&mut ctx, 'k');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
    press_char(&mut ctx, 'u');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "1"]);
    press_char(&mut ctx, 'i');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
    press_char(&mut ctx, 'l');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key"]);
    press_char(&mut ctx, 'j');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "value"]);
    press_char(&mut ctx, 'h');
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0"]);
    press_char(&mut ctx, 'h');
    assert_eq!(path(&ctx), vec!["named", "value"]);
    assert!(!press_char(&mut ctx, 'h'), "the root has nothing to exit to");
}

#[test]
fn moves_through_text_by_glyph_and_word() {
    let (mut ctx, _display, environment) = json_context(Keymap::default());
    for key in ['j', 'l', 'l', 'l'] {
        press_char(&mut ctx, key);
    }
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key", "1"]);
    assert!(press(&mut ctx, KeyName::Surface, false), "previous_glyph");
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key", "0"]);
    assert!(!press(&mut ctx, KeyName::Surface, false), "already at the start of the text");
    assert!(press_all(&mut ctx, KeyName::Dive, true, false, false), "next_word");
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key", "1"]);
    assert!(press_all(&mut ctx, KeyName::Surface, true, false, false), "previous_word");
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key", "0"]);
    assert!(press_all(&mut ctx, KeyName::Dive, true, false, false), "next_word");
    assert!(press(&mut ctx, KeyName::Surface, true), "select_previous_glyph grows from the lead");
    assert!(press_all(&mut ctx, KeyName::Char('c'), true, false, false), "copy");
    assert_eq!(environment.0.borrow_mut().clipboard.take().unwrap(), "a");
    assert!(press(&mut ctx, KeyName::Escape, false));
    assert_eq!(path(&ctx), vec!["named", "value", "named", "entries", "0", "named", "key"]);
}

fn parse_binding(json: &str) -> SpecBinding {
    return serde_json::from_str(json).unwrap();
}

fn path(ctx: &Context) -> Vec<String> {
    return ctx.cursor_syntax_path(ctx.cursor.expect("nothing is selected"));
}

fn press(ctx: &mut Context, key: KeyName, shift: bool) -> bool {
    return press_mods(ctx, key, shift, false);
}

fn press_all(ctx: &mut Context, key: KeyName, ctrl: bool, shift: bool, alt: bool) -> bool {
    let mut stroke = KeyStroke::key_stroke_new(key);
    stroke.ctrl = ctrl;
    stroke.shift = shift;
    stroke.alt = alt;
    let handled = match ctx.key_resolve(stroke) {
        KeyResolve::Unbound => false,
        KeyResolve::Pending => true,
        KeyResolve::Action(action) => {
            let handled = ctx.key_action(action);
            if handled {
                ctx.input_flush();
            }
            handled
        },
    };
    settle(ctx);
    return handled;
}

fn press_char(ctx: &mut Context, c: char) -> bool {
    return press(ctx, KeyName::Char(c), false);
}

fn press_mods(ctx: &mut Context, key: KeyName, shift: bool, alt: bool) -> bool {
    return press_all(ctx, key, false, shift, alt);
}

#[test]
fn selects_a_range_of_elements_and_copies_it() {
    let (mut ctx, _display, environment) = json_context(Keymap::default());
    for key in ['j', 'l', 'j', 'l', 'j', 'l'] {
        press_char(&mut ctx, key);
    }
    assert_eq!(
        path(&ctx),
        vec!["named", "value", "named", "entries", "1", "named", "value", "named", "elements", "0"]
    );
    assert!(press(&mut ctx, KeyName::Char('j'), true), "select_next");
    press_char(&mut ctx, 'c');
    assert_eq!(environment.0.borrow_mut().clipboard.take().unwrap(), "[\n  true,\n  null\n]");
    assert!(press(&mut ctx, KeyName::Char('k'), true), "select_previous shrinks the far end");
    press_char(&mut ctx, 'c');
    assert_eq!(environment.0.borrow_mut().clipboard.take().unwrap(), "[\n  true\n]");
}

#[test]
fn selects_text_by_word_from_either_end() {
    let syntax = load_syntax(include_str!("../../syntaxes/json.json"));
    let document = load_document(&syntax, r#"{"one two": 1}"#);
    let (mut ctx, _display, environment) = build(syntax, document, 600., 600.);
    settle(&mut ctx);
    for key in ['j', 'l', 'l', 'l'] {
        press_char(&mut ctx, key);
    }
    assert_eq!(
        path(&ctx),
        vec!["named", "value", "named", "entries", "0", "named", "key", "7"],
        "entering text puts the lead at the end, as merman2's `FieldPrimitive.selectInto` does"
    );
    let copied = |ctx: &mut Context, environment: &EnvironmentTest| -> String {
        press_all(ctx, KeyName::Char('c'), true, false, false);
        return environment.0.borrow_mut().clipboard.take().unwrap();
    };
    assert!(press_all(&mut ctx, KeyName::Surface, true, true, false), "select_previous_word");
    assert_eq!(copied(&mut ctx, &environment), "two");
    assert!(press_all(&mut ctx, KeyName::Surface, true, true, false), "select_previous_word again");
    assert_eq!(copied(&mut ctx, &environment), "one two");
    assert!(
        press_all(&mut ctx, KeyName::Dive, true, true, false),
        "the same stroke gives ground back because the lead is at the front"
    );
    assert_eq!(copied(&mut ctx, &environment), "two");
    assert!(ctx.key_action(Action::GatherFirst), "gather_first has no binding of its own");
    ctx.input_flush();
    settle(&mut ctx);
    assert_eq!(copied(&mut ctx, &environment), "one two");
    assert!(ctx.key_action(Action::ReleaseAll));
    ctx.input_flush();
    settle(&mut ctx);
    assert_eq!(
        path(&ctx),
        vec!["named", "value", "named", "entries", "0", "named", "key", "0"],
        "releasing collapses onto the lead"
    );
}

#[test]
fn the_common_section_can_bind_the_context_actions() {
    let keys = SpecKeys {
        common: HashMap::from(
            [
                ("scroll_next".to_string(), vec![parse_binding(r#"{"key": "page_down"}"#)]),
                ("scroll_previous_alot".to_string(), vec![parse_binding(r#"{"key": "page_up"}"#)]),
            ],
        ),
        ..SpecKeys::default()
    };
    let (mut ctx, _display, _environment) = json_context(Keymap::keymap_resolve(&keys).unwrap());
    let start = ctx.scroll;
    assert!(press(&mut ctx, KeyName::PageDown, false), "scroll_next");
    assert!(ctx.scroll < start, "scroll_next moves forward, got {}", ctx.scroll);
    assert!(ctx.cursor.is_none(), "scrolling does not select anything");
    assert!(press(&mut ctx, KeyName::PageUp, false), "scroll_previous_alot");
    assert!(ctx.scroll > start, "a page back overshoots the start, got {}", ctx.scroll);
}

#[test]
fn unusable_bindings_are_reported() {
    let bad = SpecKeys {
        array: HashMap::from(
            [
                ("nxt".to_string(), vec![parse_binding(r#"{"key": "n"}"#)]),
                ("next_element".to_string(), vec![parse_binding(r#"{"key": "escape"}"#)]),
                ("previous_element".to_string(), vec![parse_binding(r#"{"key": "escape"}"#)]),
                ("copy".to_string(), vec![parse_binding(r#"{"key": "g"}"#)]),
                ("first_element".to_string(), vec![parse_binding(r#"[{"key": "g"}, {"key": "g"}]"#)]),
            ],
        ),
        ..SpecKeys::default()
    };
    let errors = Keymap::keymap_resolve(&bad).err().expect("should fail");
    assert_eq!(errors.0.len(), 3, "{}", errors);
    let kinds = errors.0.iter().map(|e| &e.kind).collect::<Vec<_>>();
    assert!(
        kinds.iter().any(|k| matches!(k, ErrorKind::UnknownAction { action, .. } if action == "nxt")),
        "{}",
        errors
    );
    assert!(kinds.iter().any(|k| matches!(k, ErrorKind::AmbiguousKeyBinding { .. })), "{}", errors);
    assert!(kinds.iter().any(|k| matches!(k, ErrorKind::ShadowedKeyBinding { .. })), "{}", errors);
}
