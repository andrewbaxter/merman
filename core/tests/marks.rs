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
        display::{
            DisplayTest,
            PIXELS_PER_MM,
        },
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

fn nest(depth: usize) -> String {
    if depth == 0 {
        return r#"{"leaf": "x"}"#.to_string();
    }
    return format!(r#"{{"nest": {}}}"#, nest(depth - 1));
}

fn assert_mark_on_brick(ctx: &Context, display: &DisplayTest, m: usize) {
    let mark = ctx.marks[m].as_ref().unwrap();
    let brick = mark.brick.expect("the mark has a brick");
    let course = &ctx.courses[ctx.bricks[brick].course.expect("the brick is in a course")];
    let style = ctx.stylist.style_mark();
    let drawn =
        display
            .display_test_drawn()
            .into_iter()
            .find(|d| d.size.converse == style.length && d.size.transverse == style.thickness)
            .expect("the mark was drawn");
    assert_eq!(drawn.converse, ctx.bricks[brick].converse.round());
    assert_eq!(drawn.transverse, (course.transverse_start + course.ascent + course.descent - style.thickness).round());
}

#[test]
fn a_mark_attaches_to_an_existing_brick() {
    let syntax = load_syntax(SYNTAX);
    let document = load_document(&syntax, &format!(r#"{{"v": {}}}"#, nest(2)));
    let leaf = document.atoms.iter().position(|a| syntax.syntax_type(a.type_).id == "leaf").unwrap();
    let innermost = document.atoms[leaf].parent.as_ref().unwrap().atom;
    let (mut ctx, display, _environment) = build(syntax, document, 600., 600.);
    settle(&mut ctx);
    assert_eq!(render_text(&display, UNIT), vec!["((x))".to_string()]);
    let style = ctx.stylist.style_mark();
    let m = ctx.mark_new(innermost, style);
    let first = ctx.atom_visual[innermost].and_then(|v| ctx.visual_get_first_brick(v)).unwrap();
    assert_eq!(ctx.marks[m].as_ref().unwrap().brick, Some(first), "the mark attached on creation");
    assert_mark_on_brick(&ctx, &display, m);
}

#[test]
fn a_mark_follows_the_first_brick_through_windowing() {
    let syntax = load_syntax(SYNTAX);
    let document = load_document(&syntax, &format!(r#"{{"v": {}}}"#, nest(4)));
    let leaf = document.atoms.iter().position(|a| syntax.syntax_type(a.type_).id == "leaf").unwrap();
    let innermost = document.atoms[leaf].parent.as_ref().unwrap().atom;
    let display = DisplayTest::default();
    let mut ctx = Context::context_new(syntax, document, ContextConfig {
        ellipsize_threshold: 2,
        start_windowed: true,
        ..ContextConfig::default()
    }, Box::new(display.clone()), Box::new(EnvironmentTest::default()), 600., 600.);
    settle(&mut ctx);
    let style = ctx.stylist.style_mark();
    let m = ctx.mark_new(innermost, style);
    assert_eq!(ctx.mark_atom(m), innermost);
    assert!(ctx.marks[m].as_ref().unwrap().brick.is_none(), "the atom is beyond the window, no brick yet");
    assert_eq!(display.display_test_drawings(), 0, "nothing drawn without a brick");
    for _ in 0 .. 4 {
        ctx.key_action(Action::Enter);
        settle(&mut ctx);
    }
    let first =
        ctx.atom_visual[innermost]
            .and_then(|v| ctx.visual_get_first_brick(v))
            .expect("the window reached the atom");
    assert_eq!(
        ctx.marks[m].as_ref().unwrap().brick,
        Some(first),
        "the mark attached to the brick when it was created"
    );
    assert_mark_on_brick(&ctx, &display, m);
    assert!(ctx.key_action(Action::WindowTowardsRoot));
    settle(&mut ctx);
    assert!(ctx.key_action(Action::WindowTowardsRoot));
    settle(&mut ctx);
    assert!(ctx.marks[m].as_ref().unwrap().brick.is_none(), "the brick was destroyed when the window moved out");
    assert_eq!(display.display_test_drawings(), 0, "the drawing was cleared with the brick");
    assert!(ctx.key_action(Action::ClearWindow));
    settle(&mut ctx);
    let first =
        ctx.atom_visual[innermost]
            .and_then(|v| ctx.visual_get_first_brick(v))
            .expect("the atom is visible again");
    assert_eq!(ctx.marks[m].as_ref().unwrap().brick, Some(first), "the mark reattached to the recreated brick");
    assert_mark_on_brick(&ctx, &display, m);
    ctx.mark_destroy(m);
    assert!(ctx.marks[m].is_none());
    assert!(
        !ctx.bricks[first].attachments.iter().any(|a| matches!(a, merman_core::attachment::AttachmentRef::Mark(_)))
    );
    assert_eq!(display.display_test_drawings(), 0, "the drawing went with the mark");
}
