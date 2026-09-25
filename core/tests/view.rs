mod common;

use common::{
    load_document,
    load_syntax,
    settle,
};
use merman3_core::context::{
    Context,
    ContextConfig,
};
use merman3_core::cursor::{
    Cursor,
    CursorKind,
};
use merman3_core::display::DisplayTest;
use merman3_core::environment::EnvironmentTest;
use merman3_core::keys::Action;

fn view_context(entries: usize, transverse: f64) -> (Context, DisplayTest) {
    let syntax = load_syntax(include_str!("../../syntaxes/json.json"));
    let mut text = String::from("{");
    for i in 0 .. entries {
        if i > 0 {
            text.push_str(", ");
        }
        text.push_str(&format!("\"key{}\": {}", i, i));
    }
    text.push('}');
    let doc = load_document(&syntax, &text);
    let display = DisplayTest::default();
    let mut ctx =
        Context::context_new(
            syntax,
            doc,
            ContextConfig::default(),
            Box::new(display.clone()),
            Box::new(EnvironmentTest::default()),
            40. * 12. * 0.6,
            transverse,
        );
    settle(&mut ctx);
    return (ctx, display);
}

fn rows(display: &DisplayTest) -> usize {
    return display.display_test_rows().len();
}

#[test]
fn lays_only_around_the_view() {
    let (mut ctx, display) = view_context(3000, 600.);
    ctx.visual_select_into_any_child(ctx.root_visual);
    settle(&mut ctx);
    let laid = rows(&display);
    assert!(laid > 40, "too few courses laid: {}", laid);
    assert!(laid < 400, "the whole document was laid: {}", laid);
    let (first_before, _) = ctx.wall_usage().unwrap();
    ctx.context_scroll_by(3000.);
    settle(&mut ctx);
    let (first_after, last_after) = ctx.wall_usage().unwrap();
    assert!(rows(&display) < 400, "scrolling laid too much: {}", rows(&display));
    assert!(first_after > first_before, "courses above the view were kept: {} -> {}", first_before, first_after);
    assert!(last_after >= ctx.scroll + 600., "courses weren't laid down to the view: {} < {}", last_after, ctx.scroll);
    let border = match ctx.cursor_get(ctx.cursor.unwrap()) {
        Cursor::Atom(ca) => ca.border,
        _ => panic!("root cursor isn't an atom cursor"),
    };
    let first_course = *ctx.wall.children.first().unwrap();
    let last_course = *ctx.wall.children.last().unwrap();
    assert_eq!(ctx.borders[border].as_ref().unwrap().first, Some(ctx.courses[first_course].children[0]));
    assert_eq!(ctx.borders[border].as_ref().unwrap().last, ctx.courses[last_course].children.last().copied());
}

#[test]
fn jump_rebuilds_from_the_cursor() {
    let (mut ctx, display) = view_context(3000, 600.);
    ctx.visual_select_into_any_child(ctx.root_visual);
    while ctx.cursor.map(|c| ctx.cursor_get(c).cursor_kind()) != Some(CursorKind::Array) {
        assert!(ctx.key_action(Action::Enter));
    }
    settle(&mut ctx);
    assert!(ctx.key_action(Action::LastElement));
    let mut ticks = 0;
    while ctx.take_timer_request() || !ctx.iteration_idle() {
        ctx.handle_timer();
        ticks += 1;
        assert!(ticks < 200, "a jump to the end took too many ticks to settle");
    }
    let laid = rows(&display);
    assert!(laid > 40 && laid < 400, "unexpected course count after the jump: {}", laid);
    let (first, _) = ctx.wall_usage().unwrap();
    assert!(first < 0., "the wall wasn't rebuilt from the cornerstone: {}", first);
    assert!(ctx.scroll < 600. && ctx.scroll > -600., "the view didn't follow the cursor: {}", ctx.scroll);
}
