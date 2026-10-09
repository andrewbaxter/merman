use {
    crate::{
        client::client_send,
        panels::{
            Panel,
            PanelChange,
            PanelResult,
            models::ModelsPanel,
            panel_key_stroke,
            sessions::SessionsPanel,
            toolbar::{
                toolbar_action,
                toolbar_new,
            },
        },
    },
    gloo_render::{
        AnimationFrame,
        request_animation_frame,
    },
    gloo_utils::window,
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
    pulldown_cmark::{
        Event as MdEvent,
        Options,
        Parser,
        Tag,
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
    attach_frame: RefCell<Option<AnimationFrame>>,
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

    fn ai_clear(self: &Rc<Self>) {
        wasm_bindgen_futures::spawn_local({
            let ai = self.clone();
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
        return;
    }

    fn ai_focus(&self) {
        let input = html(&self.input);
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
        let icon = el("span").classes(&["merman_status_icon"]);
        let toolbar =
            toolbar_new(
                &[
                    ("clear", "\u{e0b8}", "Clear: start a new session"),
                    ("compact", "\u{e94d}", "Compact: summarize the session so far to free up context"),
                    ("sessions", "\u{e889}", "Session: resume an earlier session"),
                    ("model", "\u{ea4a}", "Model: choose the model"),
                ],
            );
        let older = el("div").classes(&["merman_ai_older"]).attr("hidden", "");
        let messages_el = el("div").classes(&["merman_ai_messages"]);
        let log = el("div").classes(&["merman_ai_log"]).push(older.clone()).push(messages_el.clone());
        let input = el("div").classes(&["merman_ai_input"]).attr("contenteditable", "true");
        let element = el("div").classes(&["merman_ai"]).push(toolbar).push(log.clone()).push(input.clone());
        let ai = Rc::new_cyclic(|this| Ai {
            attach_frame: RefCell::new(None),
            element: element,
            focused: Cell::new(false),
            start: Cell::new(0),
            stick: Cell::new(true),
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
            }
        });
        ai.ai_status(AiStatus::Off);
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
            let text = el("span").classes(&["merman_ai_text"]);
            if m.role == AiRole::System {
                text.ref_text(&m.text);
                return el("div").classes(&["merman_ai_msg", role]).push(time).push(text);
            }
            let mut marks = vec![];
            for (event, range) in Parser::new_ext(&m.text, Options::ENABLE_STRIKETHROUGH).into_offset_iter() {
                let class = match event {
                    MdEvent::Start(Tag::Heading { .. }) => "merman_md_heading",
                    MdEvent::Start(Tag::CodeBlock(_)) | MdEvent::Code(_) => "merman_md_code",
                    MdEvent::Start(Tag::Link { .. }) => "merman_md_link",
                    MdEvent::Start(Tag::Emphasis) => "merman_md_emphasis",
                    MdEvent::Start(Tag::Strong) => "merman_md_strong",
                    _ => continue,
                };
                marks.push((range, class));
            }
            let mut cuts = vec![0, m.text.len()];
            for (range, _) in &marks {
                cuts.push(range.start);
                cuts.push(range.end);
            }
            cuts.sort();
            cuts.dedup();
            for cut in cuts.windows(2) {
                let classes =
                    marks
                        .iter()
                        .filter(|(range, _)| range.start <= cut[0] && cut[1] <= range.end)
                        .map(|(_, class)| *class)
                        .collect::<Vec<_>>();
                text.ref_push(el("span").classes(&classes).text(&m.text[cut[0] .. cut[1]]));
            }
            return el("div").classes(&["merman_ai_msg", role]).push(time).push(text);
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

    pub fn ai_send(self: &Rc<Self>, text: String) {
        wasm_bindgen_futures::spawn_local({
            let ai = self.clone();
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
        return;
    }

    fn ai_sessions(&self) -> PanelResult {
        return PanelResult::Open(Rc::new(SessionsPanel::sessions_new(self.keys.clone(), self.this.clone())));
    }

    pub fn ai_status(&self, status: AiStatus) {
        let (glyph, title) = match status {
            AiStatus::Off => ("\u{e0ca}", "No Claude session"),
            AiStatus::Thinking => ("\u{e88b}", "Claude is thinking"),
            AiStatus::Waiting => ("\u{e0b7}", "Claude is waiting for you"),
        };
        self
            .icon
            .ref_text(glyph)
            .ref_attr("title", title)
            .ref_modify_classes(&[("merman_status_off", status == AiStatus::Off)]);
        return;
    }
}

impl Panel for Ai {
    fn panel_attach(&self) -> El {
        *self.attach_frame.borrow_mut() = Some(request_animation_frame({
            let ai = self.this.clone();
            move |_| {
                let Some(ai) = ai.upgrade() else {
                    return;
                };
                ai.ai_scroll();
                if ai.focused.get() {
                    ai.ai_focus();
                }
            }
        }));
        return el("div").classes(&["merman_panel", "merman_panel_ai"]).push(self.element.clone());
    }

    fn panel_changed(&self, _path: &str) -> Option<PanelChange> {
        return None;
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_detach(&self) { }

    fn panel_lang_errors(&self, _path: &str, _errors: &[merman_langserver::CompileError]) { }

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
                ai.ai_clear();
                return PanelResult::Used;
            }
            if text == "/resume" {
                return self.ai_sessions();
            }
            ai.ai_send(text);
            return PanelResult::Used;
        }
        let Some(stroke) =
            panel_key_stroke(e, DirectionConvert::new(SpecDirection::Right, SpecDirection::Down)) else {
                return PanelResult::Ignored;
            };
        let KeyResolve::Action(action @ (Action::Exit | Action::HistoryBack | Action::HistoryForward)) =
            self.keys.keymap_read(&mut vec![], stroke, Some(CursorKind::Primitive), false) else {
                return PanelResult::Ignored;
            };
        return PanelResult::Unused(action);
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        let Some(ai) = self.this.upgrade() else {
            return PanelResult::Ignored;
        };
        match toolbar_action(e).as_deref() {
            Some("clear") => ai.ai_clear(),
            Some("compact") => ai.ai_send("/compact".to_string()),
            Some("sessions") => return self.ai_sessions(),
            Some("model") => return PanelResult::Open(
                Rc::new(ModelsPanel::models_new(self.keys.clone(), self.this.clone())),
            ),
            _ => return PanelResult::Ignored,
        }
        return PanelResult::Used;
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
        return 30.;
    }
}

fn html(e: &El) -> HtmlElement {
    return e.raw().dyn_into().unwrap();
}
