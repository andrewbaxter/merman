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
            ContextConfig,
        },
        display::DisplayTest,
        environment::EnvironmentTest,
        keys::Action,
    },
};

const SYNTAX: &str = r##"{
  "groups": [{"id": "any", "members": ["nest", "leaf"]}],
  "root": {
    "back": {"fixed_record": [{"key": "v", "value": {"atom": {"id": "value", "type": "any"}}}]},
    "front": [{"atom": {"field": "value"}}]
  },
  "types": [
    {
      "id": "leaf",
      "back": {"fixed_record": [{"key": "leaf", "value": {"string": {"id": "text"}}}]},
      "front": [{"primitive": {"field": "text"}}]
    },
    {
      "id": "nest",
      "depth_score": 1,
      "back": {"fixed_record": [{"key": "nest", "value": {"atom": {"id": "child", "type": "any"}}}]},
      "front": [
        {"symbol": {"text": {"text": "("}}},
        {"atom": {"field": "child", "ellipsis": {"text": {"text": "..."}}}},
        {"symbol": {"text": {"text": ")"}}}
      ]
    }
  ]
}"##;
const UNIT: f64 = 4. * PIXELS_PER_MM * 0.6;

#[test]
fn ellipsize_threshold_bounds_the_window() {
    assert_eq!(windowed(2), vec!["((...))".to_string()]);
    assert_eq!(windowed(3), vec!["(((...)))".to_string()]);
}

fn nest(depth: usize) -> String {
    if depth == 0 {
        return r#"{"leaf": "x"}"#.to_string();
    }
    return format!(r#"{{"nest": {}}}"#, nest(depth - 1));
}

#[test]
fn the_window_actions_move_it_out_and_clear_it() {
    let syntax = load_syntax(SYNTAX);
    let document = load_document(&syntax, &format!(r#"{{"v": {}}}"#, nest(4)));
    let display = DisplayTest::default();
    let mut ctx = Context::context_new(syntax, document, ContextConfig {
        ellipsize_threshold: 2,
        start_windowed: true,
        ..ContextConfig::default()
    }, Box::new(display.clone()), Box::new(EnvironmentTest::default()), 600., 600.);
    settle(&mut ctx);
    for _ in 0 .. 4 {
        ctx.key_action(Action::Enter);
        settle(&mut ctx);
    }
    let inner = ctx.window_atom;
    assert!(inner > 0, "diving moved the window in");
    assert!(ctx.key_action(Action::WindowTowardsRoot));
    settle(&mut ctx);
    assert!(ctx.window_atom < inner, "the window moved back out");
    assert!(ctx.key_action(Action::ClearWindow));
    settle(&mut ctx);
    assert_eq!(render_text(&display, UNIT), vec!["((((x))))".to_string()]);
    assert!(!ctx.key_action(Action::ClearWindow), "there is no window left to clear");
}

#[test]
fn the_window_follows_the_cursor() {
    let syntax = load_syntax(SYNTAX);
    let document = load_document(&syntax, &format!(r#"{{"v": {}}}"#, nest(6)));
    let display = DisplayTest::default();
    let mut ctx = Context::context_new(syntax, document, ContextConfig {
        ellipsize_threshold: 2,
        start_windowed: true,
        ..ContextConfig::default()
    }, Box::new(display.clone()), Box::new(EnvironmentTest::default()), 600., 600.);
    settle(&mut ctx);
    let mut windows = vec![];
    let mut renders = vec![];
    for _ in 0 .. 7 {
        ctx.key_action(Action::Enter);
        settle(&mut ctx);
        windows.push(ctx.window_atom);
        renders.push(render_text(&display, UNIT).concat());
    }
    for render in &renders {
        assert!(
            render.chars().filter(|c| *c == '(').count() <= 3,
            "the window stays bounded while diving, got {:?}",
            renders
        );
    }
    assert!(windows.windows(2).all(|w| w[1] >= w[0]), "the window only moves inward, got {:?}", windows);
    assert!(windows.last() > windows.first(), "the window follows the cursor, got {:?}", windows);
    assert_eq!(renders.last().unwrap(), "((x))", "diving to the leaf reveals it");
}

#[test]
fn unwindowed_shows_the_whole_document() {
    let syntax = load_syntax(SYNTAX);
    let document = load_document(&syntax, &format!(r#"{{"v": {}}}"#, nest(4)));
    let (mut ctx, display, _environment) = build(syntax, document, 600., 600.);
    settle(&mut ctx);
    assert_eq!(render_text(&display, UNIT), vec!["((((x))))".to_string()]);
}

fn windowed(threshold: i64) -> Vec<String> {
    let syntax = load_syntax(SYNTAX);
    let document = load_document(&syntax, &format!(r#"{{"v": {}}}"#, nest(4)));
    let display = DisplayTest::default();
    let mut ctx = Context::context_new(syntax, document, ContextConfig {
        ellipsize_threshold: threshold,
        start_windowed: true,
        ..ContextConfig::default()
    }, Box::new(display.clone()), Box::new(EnvironmentTest::default()), 600., 600.);
    settle(&mut ctx);
    return render_text(&display, UNIT);
}
