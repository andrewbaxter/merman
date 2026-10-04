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
        keys::{
            Action,
            KeyName,
            KeyResolve,
            KeyStroke,
        },
        matcher::match_document,
        patch::{
            Patch,
            PatchApplied,
            patch_apply_json,
            patch_merge,
        },
        serialize::serialize_atom,
        syntax::Syntax,
    },
    serde_json::Value,
    std::rc::Rc,
};

const ARITH_SYNTAX: &str =
    r##"{
  "groups": [{"id": "expr", "members": ["num", "add", "neg", "word"]}],
  "root": {
    "back": {"fixed_record": [{"key": "body", "value": {"array": {"id": "body", "element": "expr"}}}]},
    "front": [{"array": {"field": "body", "suffix": [{"text": {"text": ";"}}]}}]
  },
  "types": [
    {"id": "num", "back": {"fixed_record": [{"key": "num", "value": {"number": {"id": "value"}}}]}, "front": [{"primitive": {"field": "value"}}]},
    {"id": "word", "back": {"fixed_record": [{"key": "word", "value": {"string": {"id": "text"}}}]}, "front": [{"symbol": {"text": {"text": "'"}}}, {"primitive": {"field": "text"}}, {"symbol": {"text": {"text": "'"}}}]},
    {"id": "neg", "back": {"fixed_record": [{"key": "neg", "value": {"atom": {"id": "value", "type": "expr"}}}]}, "front": [{"symbol": {"text": {"text": "-"}}}, {"atom": {"field": "value"}}]},
    {"id": "add", "precedence": 10, "back": {"fixed_record": [{"key": "add", "value": {"fixed_record": [{"key": "a", "value": {"atom": {"id": "a", "type": "expr"}}}, {"key": "b", "value": {"atom": {"id": "b", "type": "expr"}}}]}}]}, "front": [{"atom": {"field": "a"}}, {"symbol": {"text": {"text": "+"}}}, {"atom": {"field": "b"}}]}
  ]
}"##;
const JSON_SYNTAX: &str = include_str!("../../syntaxes/json.json");
const UNIT: f64 = 4. * PIXELS_PER_MM * 0.6;

struct Editor {
    ctx: Context,
    display: DisplayTest,
    environment: EnvironmentTest,
    json: Value,
    levels: Vec<Vec<(Patch, Patch)>>,
    redo: Vec<Vec<(Patch, Patch)>>,
    syntax: Rc<Syntax>,
}

impl Editor {
    fn new(syntax_text: &str, source: &str) -> Editor {
        let syntax = load_syntax(syntax_text);
        let document = load_document(&syntax, source);
        let display = DisplayTest::default();
        let environment = EnvironmentTest::default();
        let mut ctx = Context::context_new(syntax.clone(), document, ContextConfig {
            lay_beyond_view: f64::INFINITY,
            editable: true,
            ..ContextConfig::default()
        }, Box::new(display.clone()), Box::new(environment.clone()), 2000., 2000.);
        settle(&mut ctx);
        return Editor {
            ctx: ctx,
            display: display,
            environment: environment,
            json: serde_json::from_str(source).unwrap(),
            levels: vec![],
            redo: vec![],
            syntax: syntax,
        };
    }

    fn check(&mut self) {
        settle(&mut self.ctx);
        let written = serialize_atom(&self.syntax, &self.ctx.document, self.ctx.document.root);
        assert_eq!(written, self.json, "the patches didn't make the same change as the edit");
        let document =
            match_document(&self.syntax, &self.json).unwrap_or_else(|e| panic!("{}", e.mismatch_format()));
        let (mut fresh, fresh_display, _) = build(self.syntax.clone(), document, 2000., 2000.);
        settle(&mut fresh);
        assert_eq!(self.rendered(), render_text(&fresh_display, UNIT), "the edited view differs from a fresh one");
    }

    fn json_text(&self) -> String {
        return serde_json::to_string(&self.json).unwrap();
    }

    fn key(&mut self, key: KeyName, ctrl: bool, shift: bool) -> bool {
        let mut stroke = KeyStroke::key_stroke_new(key);
        stroke.ctrl = ctrl;
        stroke.shift = shift;
        let handled = match self.ctx.key_resolve(stroke) {
            KeyResolve::Unbound | KeyResolve::Pending => false,
            KeyResolve::Type => {
                let KeyName::Char(c) = key else {
                    panic!("typed a key without text");
                };
                let text = if shift {
                    c.to_uppercase().to_string()
                } else {
                    c.to_string()
                };
                self.ctx.edit_type(&text)
            },
            KeyResolve::Action(action) => self.ctx.key_action(action),
        };
        self.ctx.input_flush();
        self.sync();
        self.check();
        return handled;
    }

    fn press(&mut self, c: char) -> bool {
        return self.key(KeyName::Char(c), false, false);
    }

    fn redo(&mut self) {
        let level = self.redo.pop().expect("nothing to redo");
        for (forward, _) in &level {
            patch_apply_json(&mut self.json, forward).unwrap();
            self.patch(forward);
        }
        self.levels.push(level);
        self.ctx.edit_break();
        self.check();
    }

    fn patch(&mut self, patch: &Patch) {
        match self.ctx.patch_apply(patch).unwrap() {
            PatchApplied::Done => { },
            PatchApplied::Reload(_) => panic!("unexpected reload"),
        }
    }

    fn rendered(&self) -> Vec<String> {
        return render_text(&self.display, UNIT).into_iter().filter(|l| !l.is_empty()).collect();
    }

    fn sync(&mut self) {
        for batch in self.ctx.edit_take() {
            if batch.new_level || self.levels.is_empty() {
                self.levels.push(vec![]);
            }
            self.redo.clear();
            let level = self.levels.last_mut().unwrap();
            for patch in batch.patches {
                let reverse = patch_apply_json(&mut self.json, &patch).unwrap();
                let step = (patch, reverse);
                if let Some(last) = level.last_mut() {
                    if patch_merge(last, &step) {
                        continue;
                    }
                }
                level.push(step);
            }
        }
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            assert!(self.ctx.edit_type(&c.to_string()), "couldn't type {:?}", c);
            self.ctx.input_flush();
            self.sync();
            self.check();
        }
    }

    fn undo(&mut self) {
        let level = self.levels.pop().expect("nothing to undo");
        for (_, reverse) in level.iter().rev() {
            patch_apply_json(&mut self.json, reverse).unwrap();
            self.patch(reverse);
        }
        self.redo.push(level);
        self.ctx.edit_break();
        self.check();
    }
}

#[test]
fn a_record_key_can_not_be_typed_into_one_another_entry_has() {
    let mut e = Editor::new(JSON_SYNTAX, r#"{"ab": 1, "a": 2}"#);
    for key in ['l', 'l', 'j', 'l', 'l'] {
        e.press(key);
    }
    assert_eq!(e.ctx.cursor_reference().unwrap().reference_format(), "#.a[1:1]");
    assert!(!e.ctx.edit_type("b"), "the key would be taken");
    assert_eq!(e.json_text(), r#"{"ab":1,"a":2}"#);
    e.type_text("c");
    assert_eq!(e.json_text(), r#"{"ab":1,"ac":2}"#);
    e.undo();
    assert_eq!(e.json_text(), r#"{"ab":1,"a":2}"#);
}

#[test]
fn deleting_every_root_element_leaves_a_gap() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}, {"num": 2}]}"#);
    e.press('j');
    assert!(e.key(KeyName::Next, false, true));
    assert!(e.press('x'));
    assert_eq!(e.json_text(), r#"{"body":[{"__gap":""}]}"#);
    e.undo();
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"num":2}]}"#);
}

#[test]
fn elements_move_and_the_move_undoes() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}, {"num": 2}, {"num": 3}]}"#);
    e.press('j');
    assert!(e.key(KeyName::Char('j'), true, false));
    assert_eq!(e.json_text(), r#"{"body":[{"num":2},{"num":1},{"num":3}]}"#);
    assert!(e.key(KeyName::Char('j'), true, false));
    assert_eq!(e.json_text(), r#"{"body":[{"num":2},{"num":3},{"num":1}]}"#);
    assert_eq!(e.rendered(), vec!["2;3;1;"]);
    e.undo();
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"num":2},{"num":3}]}"#);
    e.redo();
    assert_eq!(e.json_text(), r#"{"body":[{"num":2},{"num":3},{"num":1}]}"#);
}

#[test]
fn inserting_an_element_makes_a_gap_and_typing_chooses_its_type() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}]}"#);
    e.press('j');
    assert!(e.press('a'));
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"__gap":""}]}"#);
    assert!(e.ctx.gap_cursor().is_some());
    e.type_text("4");
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"num":4}]}"#);
    e.type_text("2");
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"num":42}]}"#);
    assert_eq!(e.rendered(), vec!["1;42;"]);
    e.undo();
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"num":4}]}"#, "typed text undoes together, after the choice");
    e.undo();
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"__gap":""}]}"#);
}

#[test]
fn a_number_being_typed_is_kept_as_text_until_it_is_one() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}]}"#);
    e.press('j');
    e.press('l');
    e.key(KeyName::Backspace, false, false);
    assert_eq!(e.json_text(), r#"{"body":[{"num":"invalid_json_dec:"}]}"#);
    e.type_text("-");
    assert_eq!(e.json_text(), r#"{"body":[{"num":"invalid_json_dec:-"}]}"#);
    e.type_text("5");
    assert_eq!(e.json_text(), r#"{"body":[{"num":-5}]}"#);
}

#[test]
fn a_suffix_gap_takes_the_atom_before_it() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}]}"#);
    e.press('j');
    assert!(e.press('s'));
    assert_eq!(e.json_text(), r#"{"body":[{"__suffix_gap":{"text":"","preceding":[{"num":1}]}}]}"#);
    e.type_text("+");
    assert_eq!(e.json_text(), r#"{"body":[{"add":{"a":{"num":1},"b":{"__gap":""}}}]}"#);
    e.type_text("2");
    assert_eq!(e.json_text(), r#"{"body":[{"add":{"a":{"num":1},"b":{"num":2}}}]}"#);
    assert_eq!(e.rendered(), vec!["1+2;"]);
    e.undo();
    e.undo();
    assert_eq!(e.json_text(), r#"{"body":[{"__suffix_gap":{"text":"","preceding":[{"num":1}]}}]}"#);
    e.undo();
    assert_eq!(e.json_text(), r#"{"body":[{"num":1}]}"#);
}

#[test]
fn a_gap_with_several_candidates_waits_for_a_choice() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}]}"#);
    e.press('j');
    e.press('a');
    e.type_text("'");
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"word":""}]}"#, "only a word starts with a quote");
    e.type_text("hi");
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"word":"hi"}]}"#);
}

#[test]
fn gap_choices_are_listed_and_chosen_from() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}]}"#);
    e.press('j');
    e.press('a');
    e.ctx.gap_choices_sync();
    let names: Vec<String> = e.ctx.gap_choices.as_ref().unwrap().choices.iter().map(|c| c.name.clone()).collect();
    assert_eq!(names, vec!["num", "neg", "word"]);
    assert!(e.key(KeyName::Next, false, false));
    assert!(e.key(KeyName::Enter, false, false));
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"neg":{"__gap":""}}]}"#);
    assert!(e.ctx.gap_cursor().is_some(), "the cursor moves into the new atom's gap");
}

#[test]
fn leaving_an_empty_suffix_gap_puts_its_atoms_back() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}]}"#);
    e.press('j');
    e.press('s');
    assert!(e.key(KeyName::Escape, false, false));
    assert_eq!(e.json_text(), r#"{"body":[{"num":1}]}"#);
}

#[test]
fn pasting_replaces_the_selection() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}, {"num": 2}]}"#);
    e.press('j');
    assert!(e.ctx.edit_paste(r#"[{"word": "x"}, {"num": 7}]"#));
    e.sync();
    e.check();
    assert_eq!(e.json_text(), r#"{"body":[{"word":"x"},{"num":7},{"num":2}]}"#);
    e.undo();
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"num":2}]}"#);
    let _ = &e.environment;
}

#[test]
fn typing_in_a_string_changes_the_text_and_undoes_as_a_word() {
    let mut e = Editor::new(JSON_SYNTAX, r#"{"a": "hi", "b": [true, null]}"#);
    for key in ['l', 'l', 'l', 'j', 'l'] {
        e.press(key);
    }
    assert_eq!(e.ctx.cursor_reference().unwrap().reference_format(), "#.a[2:2]");
    e.type_text(" there");
    assert_eq!(e.json_text(), r#"{"a":"hi there","b":[true,null]}"#);
    assert_eq!(e.levels.len(), 1, "typing joins one undo level");
    e.key(KeyName::Backspace, false, false);
    assert_eq!(e.json_text(), r#"{"a":"hi ther","b":[true,null]}"#);
    e.undo();
    assert_eq!(e.json_text(), r#"{"a":"hi","b":[true,null]}"#);
    e.redo();
    assert_eq!(e.json_text(), r#"{"a":"hi ther","b":[true,null]}"#);
}

#[test]
fn typing_a_newline_splits_the_line() {
    let mut e = Editor::new(JSON_SYNTAX, r#"{"a": "hi"}"#);
    for key in ['l', 'l', 'l', 'j', 'l'] {
        e.press(key);
    }
    e.key(KeyName::Surface, false, false);
    assert!(e.key(KeyName::Enter, false, false));
    assert_eq!(e.json_text(), r#"{"a":"h\ni"}"#);
    e.type_text("o");
    assert_eq!(e.json_text(), r#"{"a":"h\noi"}"#);
    e.key(KeyName::Surface, false, false);
    e.key(KeyName::Backspace, false, false);
    assert_eq!(e.json_text(), r#"{"a":"hoi"}"#);
    let _ = Action::Undo;
}

const PATTERN_SYNTAX: &str =
    r##"{
  "groups": [
    {"id": "expr", "members": ["num", "var", "add"]},
    {"id": "named", "members": ["ident", "text"]}
  ],
  "root": {
    "back": {"fixed_record": [
      {"key": "body", "value": {"array": {"id": "body", "element": "expr"}}},
      {"key": "names", "value": {"array": {"id": "names", "element": "named"}}}
    ]},
    "front": [{"array": {"field": "body", "suffix": [{"text": {"text": ";"}}]}}, {"array": {"field": "names", "suffix": [{"text": {"text": ","}}]}}]
  },
  "types": [
    {"id": "num", "back": {"fixed_record": [{"key": "num", "value": {"number": {"id": "value"}}}]}, "front": [{"primitive": {"field": "value"}}]},
    {"id": "var", "suffix_on_pattern_mismatch": true, "back": {"fixed_record": [{"key": "var", "value": {"string": {"id": "name", "pattern": {"repeat1": "letters"}}}}]}, "front": [{"primitive": {"field": "name"}}]},
    {"id": "add", "precedence": 10, "back": {"fixed_record": [{"key": "add", "value": {"fixed_record": [{"key": "a", "value": {"atom": {"id": "a", "type": "expr"}}}, {"key": "b", "value": {"atom": {"id": "b", "type": "expr"}}}]}}]}, "front": [{"atom": {"field": "a"}}, {"symbol": {"text": {"text": "+"}}}, {"atom": {"field": "b"}}]},
    {"id": "ident", "back": {"fixed_record": [{"key": "name", "value": {"string": {"id": "name", "pattern": {"repeat1": "letters"}}}}]}, "front": [{"primitive": {"field": "name"}}]},
    {"id": "text", "back": {"fixed_record": [{"key": "name", "value": {"string": {"id": "name"}}}]}, "front": [{"symbol": {"text": {"text": "'"}}}, {"primitive": {"field": "name"}}]}
  ]
}"##;

#[test]
fn a_pattern_picks_the_type_and_typing_past_it_starts_a_suffix_gap() {
    let mut e = Editor::new(PATTERN_SYNTAX, r#"{"body": [{"num": 1}], "names": []}"#);
    e.press('j');
    e.press('l');
    e.press('a');
    e.type_text("x");
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"var":"x"}],"names":[]}"#, "only a var's pattern takes a letter");
    e.type_text("y+");
    assert_eq!(
        e.json_text(),
        r#"{"body":[{"num":1},{"add":{"a":{"var":"xy"},"b":{"__gap":""}}}],"names":[]}"#,
        "a `+` doesn't fit the var, so it goes to a suffix gap after it"
    );
    e.type_text("2");
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"add":{"a":{"var":"xy"},"b":{"num":2}}}],"names":[]}"#);
    e.undo();
    e.undo();
    assert_eq!(
        e.json_text(),
        r#"{"body":[{"num":1},{"var":"xy"}],"names":[]}"#,
        "the suffix and its choice undo together"
    );
}

#[test]
fn patterns_decide_types_when_reading_but_never_stop_a_file_loading() {
    let syntax = load_syntax(PATTERN_SYNTAX);
    let document = load_document(&syntax, r#"{"body": [], "names": [{"name": "ab"}, {"name": "a b"}]}"#);
    let names: Vec<String> = match document.document_atom(document.root).fields.get("names") {
        Some(merman_core::document::Field::Array(e)) => e
            .iter()
            .map(|a| syntax.syntax_type(document.document_atom(*a).type_).id.clone())
            .collect(),
        _ => panic!(),
    };
    assert_eq!(names, vec!["ident", "text"]);
    let document = load_document(&syntax, r#"{"body": [{"var": "not a var"}], "names": []}"#);
    let written = serialize_atom(&syntax, &document, document.root);
    assert_eq!(serde_json::to_string(&written).unwrap(), r#"{"body":[{"var":"not a var"}],"names":[]}"#);
    let var = syntax.types.iter().position(|t| t.id == "var").unwrap();
    assert!(syntax.syntax_primitive_valid(var, "name", "ab"));
    assert!(!syntax.syntax_primitive_valid(var, "name", "not a var"));
    let num = syntax.types.iter().position(|t| t.id == "num").unwrap();
    assert!(!syntax.syntax_primitive_valid(num, "value", "1."), "a prefix of a number isn't one");
}

#[test]
fn number_text_is_kept_as_written() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1.50}]}"#);
    assert_eq!(e.json_text(), r#"{"body":[{"num":1.50}]}"#);
    e.press('j');
    e.press('l');
    e.type_text("0");
    assert_eq!(e.json_text(), r#"{"body":[{"num":1.500}]}"#);
}

#[test]
fn what_the_syntax_does_not_match_is_shown_and_edited_as_json() {
    let mut e = Editor::new(ARITH_SYNTAX, r#"{"body": [{"num": 1}, {"mul": [2, "x"]}]}"#);
    assert_eq!(e.rendered(), vec![r#"1;{mul: [2, "x"]};"#]);
    for key in ['j', 'j', 'l', 'l', 'j', 'l'] {
        e.press(key);
    }
    assert_eq!(e.ctx.cursor_reference().unwrap().reference_format(), "#.body[1].mul[0]");
    assert!(e.press('a'));
    assert!(e.ctx.gap_cursor().is_some(), "a JSON array takes any JSON value, so a gap");
    e.type_text("t");
    assert_eq!(
        e.json_text(),
        r#"{"body":[{"num":1},{"mul":[2,{"__suffix_gap":{"text":"","preceding":[true]}},"x"]}]}"#,
        "only JSON true starts with t, and with nothing of its own to type, what follows goes in a suffix gap"
    );
    assert!(e.key(KeyName::Escape, false, false));
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"mul":[2,true,"x"]}]}"#);
    e.undo();
    e.undo();
    e.undo();
    assert_eq!(e.json_text(), r#"{"body":[{"num":1},{"mul":[2,"x"]}]}"#);
}

#[test]
fn json_fallback_text_all_uses_the_invalid_style() {
    let syntax = load_syntax(ARITH_SYNTAX);
    let invalid = syntax.types.iter().find(|t| t.id == "__json_string").map(|t| match &t.front[1] {
        merman_core::syntax::Front::Primitive(p) => p.style,
        _ => panic!(),
    }).unwrap();
    assert_eq!(
        syntax.syntax_style(invalid).color,
        merman_core::spec::default_invalid_style().color,
        "without an `invalid` theme style, the default one"
    );
    for t in syntax.types.iter().filter(|t| t.id.starts_with("__json")) {
        let symbol = |s: &merman_core::syntax::Symbol| match &s.kind {
            merman_core::syntax::SymbolKind::Text { style, .. } => assert_eq!(*style, invalid, "{}", t.id),
            merman_core::syntax::SymbolKind::Space { .. } => { },
        };
        for front in &t.front {
            match front {
                merman_core::syntax::Front::Symbol(s) => symbol(s),
                merman_core::syntax::Front::Primitive(p) => {
                    assert_eq!(p.style, invalid, "{}", t.id);
                    assert_eq!(p.invalid_style, invalid, "{}", t.id);
                },
                merman_core::syntax::Front::Array(a) => {
                    a.prefix.iter().chain(&a.separator).chain(&a.suffix).for_each(symbol);
                },
                merman_core::syntax::Front::Atom(_) => { },
            }
        }
    }
}
