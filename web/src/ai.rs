use {
    crate::{
        client::client_send,
        panels::{
            Panel,
            PanelResult,
            panel_key_stroke,
            sessions::SessionsPanel,
        },
    },
    gloo_utils::{
        document,
        window,
    },
    merman_api::{
        AiMessage,
        AiRole,
        AiStatus,
        ReqAiClear,
        ReqAiHistory,
        ReqAiSend,
        RespAiHistory,
    },
    merman_core::{
        cursor::CursorKind,
        direction::DirectionConvert,
        keys::{
            Action,
            KeyResolve,
            Keymap,
        },
        spec::SpecDirection,
    },
    rooting::{
        El,
        el,
    },
    std::{
        cell::{
            Cell,
            RefCell,
        },
        collections::HashSet,
        rc::{
            Rc,
            Weak,
        },
    },
    wasm_bindgen::{
        JsCast,
        JsValue,
    },
    web_sys::{
        HtmlElement,
        KeyboardEvent,
        MouseEvent,
    },
};

pub struct Ai {
    element: El,
    focused: Cell<bool>,
    history: Cell<bool>,
    pub icon: El,
    input: El,
    keys: Keymap,
    log: El,
    messages: RefCell<Vec<AiMessage>>,
    messages_el: El,
    older: El,
    open: RefCell<HashSet<usize>>,
    start: Cell<usize>,
    pub status: El,
    stick: Cell<bool>,
    this: Weak<Ai>,
}

impl Ai {
    pub fn ai_append(&self, text: &str) {
        let input = html(&self.input);
        input.set_inner_text(&format!("{}{}", input.inner_text(), text));
        self.ai_focus();
        return;
    }

    fn ai_focus(&self) {
        let input = html(&self.input);
        if document().active_element().is_some_and(|a| &a == input.as_ref() as &web_sys::Element) {
            return;
        }
        _ = input.focus();
        if let Ok(Some(selection)) = window().get_selection() {
            _ = selection.select_all_children(&input);
            _ = selection.collapse_to_end();
        }
        return;
    }

    pub fn ai_load(self: &Rc<Self>) {
        wasm_bindgen_futures::spawn_local({
            let ai = self.clone();
            async move {
                match client_send(ReqAiHistory {}).await {
                    Ok(RespAiHistory { status, messages }) => {
                        *ai.messages.borrow_mut() = messages;
                        ai.open.borrow_mut().clear();
                        ai.ai_status(status);
                        ai.ai_render(false);
                    },
                    Err(e) => ai.ai_message(AiMessage {
                        role: AiRole::System,
                        text: e,
                        time: js_sys::Date::now() as u64,
                    }),
                }
            }
        });
        return;
    }

    pub fn ai_message(&self, message: AiMessage) {
        self.messages.borrow_mut().push(message);
        self.ai_render(true);
        return;
    }

    pub fn ai_new(keys: Keymap) -> Rc<Ai> {
        let icon = el("span").classes(&["merman_status_icon"]).attr("hidden", "");
        let status = el("div").classes(&["merman_status"]).push(icon.clone());
        let older = el("div").classes(&["merman_ai_older"]).attr("hidden", "");
        let messages_el = el("div").classes(&["merman_ai_messages"]);
        let log = el("div").classes(&["merman_ai_log"]).push(older.clone()).push(messages_el.clone());
        let input = el("div").classes(&["merman_ai_input"]).attr("contenteditable", "true");
        let element = el("div").classes(&["merman_ai"]).push(log.clone()).push(input.clone());
        let ai = Rc::new_cyclic(|this| Ai {
            element: element,
            focused: Cell::new(false),
            start: Cell::new(0),
            stick: Cell::new(true),
            status: status,
            icon: icon.clone(),
            log: log.clone(),
            older: older.clone(),
            open: RefCell::new(HashSet::new()),
            messages_el: messages_el,
            input: input.clone(),
            keys: keys,
            messages: RefCell::new(vec![]),
            history: Cell::new(false),
            this: this.clone(),
        });
        older.ref_on("click", {
            let ai: Weak<Ai> = Rc::downgrade(&ai);
            move |_| {
                let Some(ai) = ai.upgrade() else {
                    return;
                };
                ai.history.set(!ai.history.get());
                ai.ai_render(false);
            }
        });
        log.ref_on("scroll", {
            let ai: Weak<Ai> = Rc::downgrade(&ai);
            move |_| {
                let Some(ai) = ai.upgrade() else {
                    return;
                };
                let log = ai.log.raw();
                ai.stick.set(log.scroll_top() + log.client_height() >= log.scroll_height() - 1);
            }
        });
        log.ref_on_resize({
            let ai: Weak<Ai> = Rc::downgrade(&ai);
            move |_, _, block_size| {
                let Some(ai) = ai.upgrade() else {
                    return;
                };
                if block_size == 0. {
                    return;
                }
                ai.ai_scroll();
                if ai.focused.get() {
                    ai.ai_focus();
                }
            }
        });
        ai.ai_load();
        return ai;
    }

    fn ai_render(&self, keep: bool) {
        let messages = self.messages.borrow();
        let mut start = 0;
        if !self.history.get() {
            let mut turns = 0;
            for (i, m) in messages.iter().enumerate().rev() {
                if m.role == AiRole::User {
                    turns += 1;
                    if turns == 5 {
                        start = i;
                        break;
                    }
                }
            }
            if keep && !self.stick.get() {
                start = start.min(self.start.get());
            }
        }
        self.start.set(start);
        if self.history.get() {
            self.older.ref_remove_attr("hidden").ref_text("Show recent messages only");
        } else if start > 0 {
            self.older.ref_remove_attr("hidden").ref_text(&format!("View {} older messages", start));
        } else {
            self.older.ref_attr("hidden", "");
        }
        let rows = messages[start..].iter().enumerate().map(|(i, m)| {
            let role = match m.role {
                AiRole::User => "merman_ai_user",
                AiRole::Assistant => "merman_ai_assistant",
                AiRole::Tool => "merman_ai_tool",
                AiRole::System => "merman_ai_system",
            };
            let time =
                js_sys::Date::new(&JsValue::from_f64(m.time as f64))
                    .to_locale_time_string("default")
                    .as_string()
                    .unwrap_or_default();
            let time = el("span").classes(&["merman_ai_time"]).text(&time);
            if m.role == AiRole::Tool {
                let (name, input) = m.text.split_once(' ').unwrap_or((&m.text, ""));
                let input_json = serde_json::from_str::<serde_json::Value>(input).unwrap_or_default();
                let gist =
                    input_json
                        .get("description")
                        .and_then(|d| d.as_str())
                        .or_else(|| input_json.as_object().and_then(|o| o.values().find_map(|v| v.as_str())))
                        .unwrap_or_default()
                        .lines()
                        .next()
                        .unwrap_or_default();
                let index = start + i;
                let details =
                    el("details")
                        .classes(&["merman_ai_msg", role])
                        .push(
                            el("summary")
                                .push(time)
                                .push(el("span").classes(&["merman_ai_text"]).text(&format!("{} {}", name, gist))),
                        )
                        .push(el("div").classes(&["merman_ai_text"]).text(input));
                if self.open.borrow().contains(&index) {
                    details.ref_attr("open", "");
                }
                details.ref_on("toggle", {
                    let ai = self.this.clone();
                    let details = details.weak();
                    move |_| {
                        let (Some(ai), Some(details)) = (ai.upgrade(), details.upgrade()) else {
                            return;
                        };
                        if details.raw().has_attribute("open") {
                            ai.open.borrow_mut().insert(index);
                        } else {
                            ai.open.borrow_mut().remove(&index);
                        }
                    }
                });
                return details;
            }
            return el("div")
                .classes(&["merman_ai_msg", role])
                .push(time)
                .push(el("span").classes(&["merman_ai_text"]).text(&m.text));
        }).collect::<Vec<_>>();
        self.messages_el.ref_clear();
        self.messages_el.ref_extend(rows);
        self.ai_scroll();
        return;
    }

    fn ai_scroll(&self) {
        if self.stick.get() {
            let log = self.log.raw();
            log.set_scroll_top(log.scroll_height());
        }
        return;
    }

    pub fn ai_status(&self, status: AiStatus) {
        match status {
            AiStatus::Off => {
                self.icon.ref_attr("hidden", "");
            },
            AiStatus::Thinking => {
                self.icon.ref_remove_attr("hidden").ref_text("\u{e88b}").ref_attr("title", "Claude is thinking");
            },
            AiStatus::Waiting => {
                self
                    .icon
                    .ref_remove_attr("hidden")
                    .ref_text("\u{e0b7}")
                    .ref_attr("title", "Claude is waiting for you");
            },
        }
        return;
    }
}

impl Panel for Ai {
    fn panel_attach(&self) -> El {
        return el("div").classes(&["merman_panel", "merman_panel_ai"]).push(self.element.clone());
    }

    fn panel_changed(&self, _path: &str) -> Option<bool> {
        return None;
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_detach(&self) { }

    fn panel_focusable(&self) -> bool {
        return true;
    }

    fn panel_focused(&self, focused: bool) {
        self.focused.set(focused);
        if focused {
            self.ai_focus();
        } else {
            _ = html(&self.input).blur();
        }
        return;
    }

    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult {
        if e.key() == "Enter" && !e.shift_key() {
            let input = html(&self.input);
            let text = input.inner_text().trim().to_string();
            if text.is_empty() {
                return PanelResult::Used;
            }
            input.set_inner_text("");
            let Some(ai) = self.this.upgrade() else {
                return PanelResult::Used;
            };
            if text == "/clear" {
                wasm_bindgen_futures::spawn_local({
                    let ai = ai.clone();
                    async move {
                        if let Err(e) = client_send(ReqAiClear {}).await {
                            ai.ai_message(AiMessage {
                                role: AiRole::System,
                                text: e,
                                time: js_sys::Date::now() as u64,
                            });
                            return;
                        }
                        ai.messages.borrow_mut().clear();
                        ai.open.borrow_mut().clear();
                        ai.history.set(false);
                        ai.ai_render(false);
                    }
                });
                return PanelResult::Used;
            }
            if text == "/resume" {
                return PanelResult::Open(
                    Rc::new(SessionsPanel::sessions_new(self.keys.clone(), self.this.clone())),
                );
            }
            wasm_bindgen_futures::spawn_local({
                let ai = ai.clone();
                async move {
                    if let Err(e) = client_send(ReqAiSend { text: text }).await {
                        ai.ai_message(AiMessage {
                            role: AiRole::System,
                            text: e,
                            time: js_sys::Date::now() as u64,
                        });
                    }
                }
            });
            return PanelResult::Used;
        }
        let Some(stroke) =
            panel_key_stroke(e, DirectionConvert::new(SpecDirection::Right, SpecDirection::Down)) else {
                return PanelResult::Ignored;
            };
        let KeyResolve::Action(Action::Exit) =
            self.keys.keymap_read(&mut vec![], stroke, Some(CursorKind::Primitive)) else {
                return PanelResult::Ignored;
            };
        return PanelResult::Unused(Action::Exit);
    }

    fn panel_mouse(&self, _e: &MouseEvent) -> PanelResult {
        return PanelResult::Ignored;
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

    fn panel_selection(&self) -> Option<(bool, String)> {
        return None;
    }

    fn panel_size(&self) -> f64 {
        return 3.;
    }
}

fn html(e: &El) -> HtmlElement {
    return e.raw().dyn_into().unwrap();
}
