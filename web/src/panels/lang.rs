use {
    crate::{
        lang::{
            Lang,
            lang_source_key,
        },
        panels::{
            Panel,
            PanelChange,
            PanelResult,
            list::{
                List,
                ListRow,
            },
        },
    },
    merman_core::keys::{
        Action,
        Keymap,
    },
    merman_langserver::{
        CompileError,
        Location,
        Source,
    },
    rooting::El,
    std::{
        cell::RefCell,
        rc::Rc,
    },
    wasm_bindgen::JsValue,
    web_sys::{
        KeyboardEvent,
        MouseEvent,
    },
};

pub struct LangPanel(Rc<RefCell<State>>);

impl LangPanel {
    pub fn lang_new(keys: Keymap, lang: Rc<Lang>, path: String) -> LangPanel {
        let mut state = State {
            focused: false,
            lang: lang,
            list: List::list_new(keys, "merman_panel_lang", "No compiles yet", vec![], None),
            path: path,
            targets: vec![],
        };
        lang_fill(&mut state);
        return LangPanel(Rc::new(RefCell::new(state)));
    }
}

impl Panel for LangPanel {
    fn panel_attach(&self) -> El {
        return self.0.borrow_mut().list.list_attach();
    }

    fn panel_changed(&self, _path: &str) -> Option<PanelChange> {
        return None;
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_detach(&self) {
        self.0.borrow_mut().list.list_detach();
        return;
    }

    fn panel_editable(&self, _editable: bool) { }

    fn panel_focusable(&self) -> bool {
        return true;
    }

    fn panel_focused(&self, focused: bool) {
        let mut s = self.0.borrow_mut();
        s.focused = focused;
        s.list.list_focused(focused);
        return;
    }

    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult {
        let result = self.0.borrow_mut().list.list_key(e);
        match result {
            PanelResult::Unused(Action::Enter) => return lang_activate(&self.0.borrow()),
            result => return result,
        }
    }

    fn panel_lang_errors(&self, path: &str, _errors: &[CompileError]) {
        let mut s = self.0.borrow_mut();
        if path != s.path {
            return;
        }
        lang_fill(&mut s);
        s.list.list_draw();
        return;
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        let result = self.0.borrow_mut().list.list_mouse(e);
        match result {
            PanelResult::Selected | PanelResult::Used => return lang_activate(&self.0.borrow()),
            result => return result,
        }
    }

    fn panel_parent(&self) -> Option<String> {
        return None;
    }

    fn panel_path(&self) -> String {
        return self.0.borrow().path.clone();
    }

    fn panel_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_select(&self, _location: &str) { }

    fn panel_selection(&self) -> Option<(bool, String)> {
        return None;
    }

    fn panel_size(&self) -> f64 {
        return 30.;
    }
}

fn lang_activate(s: &State) -> PanelResult {
    let Some(selected) = s.list.selected else {
        return PanelResult::Unused(Action::Enter);
    };
    match &s.targets[selected] {
        Target::None => return PanelResult::Used,
        Target::Local(path, location) => return PanelResult::Jump(path.clone(), location.clone()),
        Target::Remote(server, source, location) => return PanelResult::JumpRemote(
            server.clone(),
            source.clone(),
            location.clone(),
        ),
    }
}

fn lang_fill(s: &mut State) {
    let mut rows = vec![];
    let mut targets = vec![];
    let Some(file) = s.lang.lang_file(&s.path) else {
        s.list.rows = rows;
        s.targets = targets;
        return;
    };
    if !file.dirty.is_empty() {
        rows.push(ListRow {
            icon: Some("\u{e863}"),
            spans: vec![],
            text: format!("Compiling ({})", file.dirty.iter().cloned().collect::<Vec<_>>().join(", ")),
        });
        targets.push(Target::None);
    }
    for ((server, import_state), compiled) in &file.compiles {
        let started =
            js_sys::Date::new(&JsValue::from_f64(compiled.started_ms as f64))
                .to_locale_time_string("default")
                .as_string()
                .unwrap_or_default();
        rows.push(ListRow {
            icon: None,
            spans: vec![],
            text: format!(
                "{} \u{2014} import state {} \u{2014} {} ({} ms)",
                server,
                import_state,
                started,
                compiled.ended_ms - compiled.started_ms
            ),
        });
        targets.push(Target::None);
        for log in &compiled.logs {
            rows.push(ListRow {
                icon: Some("\u{e0ee}"),
                spans: vec![],
                text: log.message.clone(),
            });
            targets.push(Target::None);
        }
        for error in &compiled.errors {
            rows.push(ListRow {
                icon: Some("\u{e000}"),
                spans: vec![],
                text: format!("{} {}", lang_location_text(&error.location, &s.path), error.message),
            });
            targets.push(lang_target(server, &error.location));
            for related in &error.related {
                rows.push(ListRow {
                    icon: None,
                    spans: vec![],
                    text: format!("    {}: {}", related.description, lang_location_text(&related.location, &s.path)),
                });
                targets.push(lang_target(server, &related.location));
            }
        }
    }
    s.list.rows = rows;
    s.targets = targets;
    return;
}

fn lang_location_text(location: &Location, here: &str) -> String {
    let key = lang_source_key(&location.source);
    let name = if key == here {
        ""
    } else {
        key.rsplit('/').next().unwrap_or(key)
    };
    return match location.expr {
        Some(expr) => format!("{}#{}", name, expr),
        None => name.to_string(),
    };
}

fn lang_target(server: &str, location: &Location) -> Target {
    let reference = location.expr.map(|expr| format!("#{}", expr));
    match &location.source {
        Source::Local { path } => return Target::Local(path.clone(), reference),
        Source::Remote { .. } => return Target::Remote(server.to_string(), location.source.clone(), reference),
    }
}

struct State {
    focused: bool,
    lang: Rc<Lang>,
    list: List,
    path: String,
    targets: Vec<Target>,
}

enum Target {
    None,
    Local(String, Option<String>),
    Remote(String, Source, Option<String>),
}
