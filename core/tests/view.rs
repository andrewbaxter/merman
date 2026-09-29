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
        context::{
            Context,
            ContextConfig,
        },
        cursor::{
            Cursor,
            CursorKind,
        },
        display::DisplayTest,
        keys::Action,
    },
};

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
    let laid = render_text(&display, 1.).len();
    assert!(laid > 40 && laid < 400, "unexpected course count after the jump: {}", laid);
    let (first, _) = ctx.wall_usage().unwrap();
    assert!(first < 0., "the wall wasn't rebuilt from the cornerstone: {}", first);
    assert!(ctx.scroll < 600. && ctx.scroll > -600., "the view didn't follow the cursor: {}", ctx.scroll);
}

#[test]
fn lays_only_around_the_view() {
    let (mut ctx, display) = view_context(3000, 600.);
    ctx.visual_select_into_any_child(ctx.root_visual);
    settle(&mut ctx);
    let laid = render_text(&display, 1.).len();
    assert!(laid > 40, "too few courses laid: {}", laid);
    assert!(laid < 400, "the whole document was laid: {}", laid);
    let (first_before, _) = ctx.wall_usage().unwrap();
    ctx.context_scroll_by(3000.);
    settle(&mut ctx);
    let (first_after, last_after) = ctx.wall_usage().unwrap();
    assert!(render_text(&display, 1.).len() < 400, "scrolling laid too much: {}", render_text(&display, 1.).len());
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
fn scrolling_away_isnt_pulled_back() {
    let (mut ctx, _display) = view_context(3000, 600.);
    ctx.visual_select_into_any_child(ctx.root_visual);
    settle(&mut ctx);
    for _ in 0 .. 60 {
        ctx.context_scroll_by(100.);
        let scrolled = ctx.scroll;
        settle(&mut ctx);
        assert_eq!(ctx.scroll, scrolled, "the view was pulled back after scrolling");
    }
    assert!(ctx.key_action(Action::ScrollReset));
    assert!(ctx.scroll < 600., "scroll reset didn't return to the cursor: {}", ctx.scroll);
}

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
    let (mut ctx, display, _environment) = build(syntax, doc, 40. * 12. * 0.6, transverse);
    ctx.config.lay_beyond_view = ContextConfig::default().lay_beyond_view;
    settle(&mut ctx);
    return (ctx, display);
}
