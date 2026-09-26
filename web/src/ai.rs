use {
    crate::client::client_send,
    gloo_utils::window,
    merman_api::{
        AiMessage,
        AiRole,
        AiStatus,
        ReqAiClear,
        ReqAiHistory,
        ReqAiResume,
        ReqAiSend,
        ReqAiSessions,
        RespAiHistory,
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
    },
};

pub struct Ai {
    pub element: El,
    history: Cell<bool>,
    icon: El,
    input: El,
    log: El,
    messages: RefCell<Vec<AiMessage>>,
    messages_el: El,
    older: El,
    sessions: El,
    pub status: El,
}

impl Ai {
    pub fn ai_append(&self, text: &str) {
        let input = html(&self.input);
        input.set_inner_text(&format!("{}{}", input.inner_text(), text));
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
                        ai.ai_status(status);
                        ai.ai_render();
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
        self.ai_render();
        return;
    }

    pub fn ai_new() -> Rc<Ai> {
        let icon = el("span").classes(&["merman_status_icon"]).attr("hidden", "");
        let status = el("div").classes(&["merman_status"]).push(icon.clone());
        let older = el("button").classes(&["merman_ai_older"]).attr("hidden", "");
        let messages_el = el("div").classes(&["merman_ai_messages"]);
        let log = el("div").classes(&["merman_ai_log"]).push(older.clone()).push(messages_el.clone());
        let sessions = el("div").classes(&["merman_ai_sessions"]).attr("hidden", "");
        let input = el("div").classes(&["merman_ai_input"]).attr("contenteditable", "true");
        let send = el("button").text("Send");
        let clear = el("button").text("Clear");
        let close = el("button").text("Close");
        let element =
            el("div")
                .classes(&["merman_ai"])
                .attr("hidden", "")
                .push(
                    el("div")
                        .classes(&["merman_ai_header"])
                        .push(el("span").classes(&["merman_ai_title"]).text("Claude"))
                        .push(clear.clone())
                        .push(close.clone()),
                )
                .push(sessions.clone())
                .push(log.clone())
                .push(el("div").classes(&["merman_ai_compose"]).push(input.clone()).push(send.clone()));
        let ai = Rc::new(Ai {
            element: element.clone(),
            status: status,
            icon: icon.clone(),
            log: log,
            older: older.clone(),
            messages_el: messages_el,
            sessions: sessions,
            input: input.clone(),
            messages: RefCell::new(vec![]),
            history: Cell::new(false),
        });
        icon.ref_on("click", with_ai(&ai, |ai| ai.ai_open()));
        close.ref_on("click", with_ai(&ai, |ai| {
            ai.element.ref_attr("hidden", "");
        }));
        clear.ref_on("click", with_ai(&ai, |ai| {
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
                    ai.history.set(false);
                    ai.ai_render();
                }
            });
        }));
        older.ref_on("click", with_ai(&ai, |ai| {
            ai.history.set(!ai.history.get());
            ai.ai_render();
        }));
        send.ref_on("click", with_ai(&ai, |ai| ai.ai_send()));
        input.ref_on("keydown", {
            let ai: Weak<Ai> = Rc::downgrade(&ai);
            move |e| {
                let e: &KeyboardEvent = e.dyn_ref().unwrap();
                if e.key() != "Enter" || e.shift_key() {
                    return;
                }
                e.prevent_default();
                if let Some(ai) = ai.upgrade() {
                    ai.ai_send();
                }
            }
        });
        ai.ai_load();
        return ai;
    }

    pub fn ai_open(&self) {
        self.element.ref_remove_attr("hidden");
        _ = html(&self.input).focus();
        return;
    }

    fn ai_render(&self) {
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
        }
        if self.history.get() {
            self.older.ref_remove_attr("hidden").ref_text("Show recent messages only");
        } else if start > 0 {
            self.older.ref_remove_attr("hidden").ref_text(&format!("View {} older messages", start));
        } else {
            self.older.ref_attr("hidden", "");
        }
        let rows = messages[start..].iter().map(|m| {
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
            return el("div")
                .classes(&["merman_ai_msg", role])
                .push(el("span").classes(&["merman_ai_time"]).text(&time))
                .push(el("span").classes(&["merman_ai_text"]).text(&m.text));
        }).collect::<Vec<_>>();
        self.messages_el.ref_clear();
        self.messages_el.ref_extend(rows);
        let log = self.log.raw();
        log.set_scroll_top(log.scroll_height());
        return;
    }

    fn ai_send(self: &Rc<Self>) {
        let input = html(&self.input);
        let text = input.inner_text().trim().to_string();
        if text.is_empty() {
            return;
        }
        input.set_inner_text("");
        if text == "/resume" {
            wasm_bindgen_futures::spawn_local({
                let ai = self.clone();
                async move {
                    let sessions = match client_send(ReqAiSessions {}).await {
                        Ok(v) => v.sessions,
                        Err(e) => {
                            ai.ai_message(AiMessage {
                                role: AiRole::System,
                                text: e,
                                time: js_sys::Date::now() as u64,
                            });
                            return;
                        },
                    };
                    let rows = sessions.iter().map(|session| {
                        let first: String = session.first.chars().take(80).collect();
                        let time =
                            js_sys::Date::new(&JsValue::from_f64(session.last_time as f64))
                                .to_locale_string("default", &JsValue::UNDEFINED)
                                .as_string()
                                .unwrap_or_default();
                        let row =
                            el("button")
                                .classes(&["merman_ai_session"])
                                .text(&format!("{} \u{2014} {} ({} messages)", time, first, session.messages));
                        row.ref_on("click", {
                            let ai: Weak<Ai> = Rc::downgrade(&ai);
                            let id = session.id.clone();
                            move |_| {
                                let Some(ai) = ai.upgrade() else {
                                    return;
                                };
                                ai.sessions.ref_attr("hidden", "");
                                wasm_bindgen_futures::spawn_local({
                                    let ai = ai.clone();
                                    let id = id.clone();
                                    async move {
                                        match client_send(ReqAiResume { id: id }).await {
                                            Ok(_) => ai.ai_load(),
                                            Err(e) => ai.ai_message(AiMessage {
                                                role: AiRole::System,
                                                text: e,
                                                time: js_sys::Date::now() as u64,
                                            }),
                                        }
                                    }
                                });
                            }
                        });
                        return row;
                    }).collect::<Vec<_>>();
                    let cancel = el("button").text("Cancel");
                    cancel.ref_on("click", with_ai(&ai, |ai| {
                        ai.sessions.ref_attr("hidden", "");
                    }));
                    ai.sessions.ref_clear();
                    ai.sessions.ref_push(el("div").classes(&["merman_ai_sessions_title"]).text(if rows.is_empty() {
                        "No earlier sessions"
                    } else {
                        "Resume a session"
                    }));
                    ai.sessions.ref_extend(rows);
                    ai.sessions.ref_push(cancel);
                    ai.sessions.ref_remove_attr("hidden");
                }
            });
            return;
        }
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

fn html(e: &El) -> HtmlElement {
    return e.raw().dyn_into().unwrap();
}

fn with_ai(ai: &Rc<Ai>, f: impl Fn(&Rc<Ai>) + 'static) -> impl FnMut(&web_sys::Event) + 'static {
    let ai: Weak<Ai> = Rc::downgrade(ai);
    return move |_| {
        if let Some(ai) = ai.upgrade() {
            f(&ai);
        }
    };
}
