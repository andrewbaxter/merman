use {
    crate::{
        ai::Ai,
        client::client_send,
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
    merman_api::{
        AiMessage,
        AiRole,
        ReqAiResume,
        ReqAiSessions,
    },
    merman_core::keys::{
        Action,
        Keymap,
    },
    rooting::El,
    std::{
        cell::RefCell,
        rc::{
            Rc,
            Weak,
        },
    },
    wasm_bindgen::JsValue,
    web_sys::{
        KeyboardEvent,
        MouseEvent,
    },
};

pub struct SessionsPanel(Rc<RefCell<State>>);

impl SessionsPanel {
    pub fn sessions_new(keys: Keymap, ai: Weak<Ai>) -> SessionsPanel {
        let state = Rc::new(RefCell::new(State {
            ai: ai.clone(),
            focused: false,
            ids: vec![],
            list: List::list_new(keys, "merman_panel_sessions", "Loading...", vec![], None),
        }));
        wasm_bindgen_futures::spawn_local({
            let state = Rc::downgrade(&state);
            async move {
                let result = client_send(ReqAiSessions {}).await;
                let Some(state) = state.upgrade() else {
                    return;
                };
                let sessions = match result {
                    Ok(v) => v.sessions,
                    Err(e) => {
                        if let Some(ai) = ai.upgrade() {
                            ai.ai_message(AiMessage {
                                role: AiRole::System,
                                text: e,
                                time: js_sys::Date::now() as u64,
                            });
                        }
                        vec![]
                    },
                };
                let mut s = state.borrow_mut();
                s.ids = sessions.iter().map(|session| session.id.clone()).collect();
                s.list.rows = sessions.iter().map(|session| {
                    let first: String = session.first.chars().take(80).map(|c| if c == '\n' {
                        ' '
                    } else {
                        c
                    }).collect();
                    let time =
                        js_sys::Date::new(&JsValue::from_f64(session.last_time as f64))
                            .to_locale_string("default", &JsValue::UNDEFINED)
                            .as_string()
                            .unwrap_or_default();
                    return ListRow {
                        spans: vec![],
                        icon: None,
                        text: format!("{} \u{2014} {} ({} messages)", time, first, session.messages),
                    };
                }).collect();
                s.list.empty = "No earlier sessions";
                s.list.list_draw();
                if s.focused {
                    s.list.list_focused(true);
                }
            }
        });
        return SessionsPanel(state);
    }
}

impl Panel for SessionsPanel {
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

    fn panel_lang_errors(&self, _path: &str, _errors: &[merman_langserver::CompileError]) { }

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
            PanelResult::Unused(Action::Enter) => return sessions_resume(&self.0.borrow()),
            result => return result,
        }
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        let result = self.0.borrow_mut().list.list_mouse(e);
        match result {
            PanelResult::Selected | PanelResult::Used => return sessions_resume(&self.0.borrow()),
            result => return result,
        }
    }

    fn panel_parent(&self) -> Option<String> {
        return None;
    }

    fn panel_path(&self) -> String {
        return String::new();
    }

    fn panel_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_select(&self, _location: &str) { }

    fn panel_selection(&self) -> Option<(bool, String)> {
        return None;
    }

    fn panel_size(&self) -> f64 {
        return 15.;
    }
}

fn sessions_resume(s: &State) -> PanelResult {
    let Some(selected) = s.list.selected else {
        return PanelResult::Unused(Action::Enter);
    };
    let id = s.ids[selected].clone();
    let ai = s.ai.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let result = client_send(ReqAiResume { id: id }).await;
        let Some(ai) = ai.upgrade() else {
            return;
        };
        match result {
            Ok(_) => ai.ai_load(),
            Err(e) => ai.ai_message(AiMessage {
                role: AiRole::System,
                text: e,
                time: js_sys::Date::now() as u64,
            }),
        }
    });
    return PanelResult::Unused(Action::Exit);
}

struct State {
    ai: Weak<Ai>,
    focused: bool,
    ids: Vec<String>,
    list: List,
}
