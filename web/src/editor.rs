use {
    crate::{
        ai::Ai,
        client::client_send,
        lang::{
            Lang,
            lang_source_key,
        },
        panels::{
            Panel,
            PanelChange,
            PanelHost,
            PanelResult,
            code::{
                CodeEdit,
                CodePanel,
            },
            error::ErrorPanel,
            filesystem::FilesystemPanel,
            lang::LangPanel,
            panel_theme_apply,
        },
    },
    futures::channel::oneshot::Receiver,
    gloo_render::{
        AnimationFrame,
        request_animation_frame,
    },
    gloo_events::{
        EventListener,
        EventListenerOptions,
    },
    gloo_timers::callback::Timeout,
    gloo_utils::{
        document,
        window,
    },
    merman_api::{
        Event,
        ReqLangSourceRead,
        ReqList,
        ReqLocationSet,
        ReqOpen,
        ReqStart,
        WS_PATH,
        WsClient,
        WsServer,
    },
    merman_core::{
        keys::{
            Action,
            Keymap,
            SpecKeys,
        },
        matcher::{
            match_document,
            source_parse,
        },
        spec::{
            SpecSyntax,
            SpecTheme,
        },
        syntax::Syntax,
    },
    rooting::{
        El,
        el,
        set_root,
        set_root_non_dom,
        spawn_rooted,
    },
    std::{
        cell::{
            Cell,
            RefCell,
        },
        collections::HashMap,
        path::Path,
        rc::{
            Rc,
            Weak,
        },
    },
    wasm_bindgen::{
        JsCast,
        prelude::*,
    },
    web_sys::{
        HtmlElement,
        KeyboardEvent,
        MessageEvent,
        MouseEvent,
        PopStateEvent,
        WebSocket,
    },
};

struct Editor {
    ai: Rc<Ai>,
    animation: RefCell<Option<AnimationFrame>>,
    animation_start: Cell<f64>,
    changed: RefCell<Vec<String>>,
    changed_timer: RefCell<Option<Timeout>>,
    child_request: RefCell<Option<Receiver<()>>>,
    dir: String,
    element: El,
    focus: Cell<usize>,
    focus_next: Cell<bool>,
    history_navigating: Cell<bool>,
    history_select: RefCell<Option<(String, String)>>,
    keys: Keymap,
    lang: Rc<Lang>,
    lang_panel: RefCell<Option<(String, Rc<dyn Panel>)>>,
    last_seq: Cell<Option<u64>>,
    location_timer: RefCell<Option<Timeout>>,
    panels: RefCell<Vec<Rc<dyn Panel>>>,
    parent_request: RefCell<Option<Receiver<()>>>,
    reconnect: RefCell<Option<Timeout>>,
    reloads: RefCell<HashMap<String, Receiver<()>>>,
    selections: RefCell<HashMap<String, String>>,
    shown: RefCell<Vec<Shown>>,
    socket: RefCell<Option<(WebSocket, Vec<EventListener>)>>,
    start_focus: RefCell<Option<String>>,
    status_panel: El,
    theme: Rc<SpecTheme>,
}

struct Shown {
    element: El,
    from: f64,
    leaving: bool,
    panel: Rc<dyn Panel>,
    to: f64,
}

fn editor_connect(editor: &Rc<Editor>) {
    let location = window().location();
    let scheme = if location.protocol().unwrap() == "https:" {
        "wss"
    } else {
        "ws"
    };
    let socket = WebSocket::new(&format!("{}://{}{}", scheme, location.host().unwrap(), WS_PATH)).unwrap();
    let open = EventListener::new(&socket, "open", {
        let editor = editor.clone();
        let socket = socket.clone();
        move |_| {
            socket
                .send_with_str(&serde_json::to_string(&WsClient { since: editor.last_seq.get() }).unwrap())
                .unwrap();
        }
    });
    let message = EventListener::new(&socket, "message", {
        let editor = editor.clone();
        move |e| {
            let e: &MessageEvent = e.dyn_ref().unwrap();
            let data = e.data().as_string().unwrap();
            let event = match serde_json::from_str::<WsServer>(&data).unwrap() {
                WsServer::Gap => {
                    editor.ai.ai_load();
                    let paths = editor.panels.borrow().iter().map(|p| p.panel_path()).collect::<Vec<_>>();
                    for path in &paths {
                        if Lang::lang_path_relevant(path) {
                            editor.lang.lang_load(path.clone());
                        }
                    }
                    editor.changed.borrow_mut().extend(paths);
                    editor_schedule_reload(&editor);
                    return;
                },
                WsServer::Event { seq, event } => {
                    if editor.last_seq.get().is_some_and(|last| seq <= last) {
                        return;
                    }
                    editor.last_seq.set(Some(seq));
                    event
                },
            };
            match event {
                Event::FileChanged { path } => {
                    editor.changed.borrow_mut().push(path);
                    editor_schedule_reload(&editor);
                },
                Event::Ai { message } => editor.ai.ai_message(message),
                Event::AiStatus { status } => editor.ai.ai_status(status),
                Event::LangServer { server, announce } => editor.lang.lang_announce(&server, announce),
                Event::LangServerStatus { server, running } => {
                    editor.lang.lang_status(&server, running);
                    editor_status_refresh(&editor);
                },
            }
        }
    });
    let close = EventListener::new(&socket, "close", {
        let editor = editor.clone();
        move |_| {
            *editor.reconnect.borrow_mut() = Some(Timeout::new(1000, {
                let editor = editor.clone();
                move || editor_connect(&editor)
            }));
        }
    });
    *editor.socket.borrow_mut() = Some((socket, vec![open, message, close]));
    return;
}

fn editor_animate(editor: &Rc<Editor>, now: f64) {
    let progress = editor_progress(editor, now);
    let mut shown = editor.shown.borrow_mut();
    for entry in shown.iter() {
        _ =
            entry
                .element
                .raw()
                .dyn_into::<HtmlElement>()
                .unwrap()
                .style()
                .set_property("left", &format!("{}px", (entry.from + (entry.to - entry.from) * progress).round()));
    }
    if progress < 1. {
        *editor.animation.borrow_mut() = Some(request_animation_frame({
            let editor: Weak<Editor> = Rc::downgrade(editor);
            move |_| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                editor_animate(&editor, js_sys::Date::now());
            }
        }));
        return;
    }
    let mut i = 0;
    while i < shown.len() {
        if !shown[i].leaving {
            shown[i].from = shown[i].to;
            i += 1;
            continue;
        }
        editor.element.ref_splice(i, 1, vec![]);
        shown.remove(i).panel.panel_detach();
    }
    return;
}

fn editor_ai_open(editor: &Rc<Editor>, index: usize) {
    let ai: Rc<dyn Panel> = editor.ai.clone();
    if let Some(existing) = editor_index(editor, &ai) {
        editor_focus(editor, existing);
        return;
    }
    editor_open_child(editor, index, ai);
    return;
}

fn editor_open_child(editor: &Rc<Editor>, index: usize, child: Rc<dyn Panel>) {
    *editor.child_request.borrow_mut() = None;
    let count = editor.panels.borrow().len();
    editor_splice(editor, index + 1, count - index - 1, vec![child]);
    editor_focus(editor, index + 1);
    return;
}

fn editor_focus(editor: &Rc<Editor>, index: usize) {
    if let Some(active) = document().active_element() {
        _ = active.dyn_into::<HtmlElement>().map(|active| active.blur());
    }
    editor.focus.set(index);
    editor.focus_next.set(false);
    let count = editor.panels.borrow().len();
    editor_splice(editor, (index + 2).min(count), count.saturating_sub(index + 2), vec![]);
    let (panel, child) = {
        let panels = editor.panels.borrow();
        (panels[index].clone(), panels.get(index + 1).cloned())
    };
    if let Some(child) = child {
        child.panel_focused(false);
    }
    panel.panel_focused(true);
    editor_show(editor);
    editor_sync(editor, index);
    editor_history_record(editor);
    editor_status_refresh(editor);
    return;
}

fn editor_jump(editor: &Rc<Editor>, path: String, location: Option<String>) {
    *editor.location_timer.borrow_mut() = None;
    let panels = editor.panels.borrow().clone();
    let Some(index) = panels.iter().rposition(|p| {
        let panel_path = p.panel_path();
        return !panel_path.is_empty() && Path::new(&path).starts_with(&panel_path);
    }) else {
        return;
    };
    let panel = &panels[index];
    if panel.panel_path() == path {
        if let Some(location) = &location {
            panel.panel_select(location);
        }
    } else {
        let mut child = Path::new(&path);
        {
            let mut selections = editor.selections.borrow_mut();
            while let Some(parent) = child.parent().filter(|p| *p != Path::new(&panel.panel_path())) {
                selections.insert(parent.to_string_lossy().into_owned(), child.to_string_lossy().into_owned());
                child = parent;
            }
            if let Some(location) = &location {
                selections.insert(path.clone(), location.clone());
                *editor.history_select.borrow_mut() = Some((path.clone(), location.clone()));
            }
        }
        *editor.start_focus.borrow_mut() = Some(path.clone());
        panel.panel_select(&child.to_string_lossy());
    }
    editor_focus(editor, index);
    return;
}

fn editor_status_refresh(editor: &Rc<Editor>) {
    editor.status_panel.ref_clear();
    let panel = editor.panels.borrow().get(editor.focus.get()).cloned();
    let Some(panel) = panel else {
        return;
    };
    let path = panel.panel_path();
    let Some(file) = editor.lang.lang_file(&path) else {
        return;
    };
    if !file.dirty.is_empty() {
        editor
            .status_panel
            .ref_push(
                el("span")
                    .classes(&["merman_status_icon", "merman_status_spin"])
                    .attr("title", "Compiling")
                    .text("\u{e863}"),
            );
    }
    let errors = editor.lang.lang_errors(&path).len();
    let logs =
        el("span")
            .classes(&["merman_status_icon", "merman_status_badge_holder"])
            .attr("title", "Compile logs and errors")
            .text("\u{e0ee}");
    if errors > 0 {
        logs.ref_push(el("span").classes(&["merman_status_badge"]).text(&errors.to_string()));
    }
    logs.ref_modify_classes(&[("merman_status_off", !editor.lang.lang_running())]);
    logs.ref_on("click", {
        let editor: Weak<Editor> = Rc::downgrade(editor);
        move |_| {
            let Some(editor) = editor.upgrade() else {
                return;
            };
            if let Some((shown, panel)) = editor.lang_panel.borrow().as_ref() {
                if *shown == path {
                    if let Some(existing) = editor_index(&editor, panel) {
                        editor_focus(&editor, existing);
                        return;
                    }
                }
            }
            let panel: Rc<dyn Panel> =
                Rc::new(LangPanel::lang_new(editor.keys.clone(), editor.lang.clone(), path.clone()));
            *editor.lang_panel.borrow_mut() = Some((path.clone(), panel.clone()));
            editor_open_child(&editor, editor.focus.get(), panel);
        }
    });
    editor.status_panel.ref_push(logs);
    return;
}

fn editor_host(editor: &Rc<Editor>) -> PanelHost {
    let editor: Weak<Editor> = Rc::downgrade(editor);
    return PanelHost(Rc::new(move |panel, result| {
        let Some(editor) = editor.upgrade() else {
            return;
        };
        let Some(index) = editor_index(&editor, &panel) else {
            return;
        };
        editor_result(&editor, index, result);
    }));
}

fn editor_history_record(editor: &Editor) {
    if editor.history_navigating.get() || editor.start_focus.borrow().is_some() {
        return;
    }
    let Some(panel) = editor.panels.borrow().get(editor.focus.get()).cloned() else {
        return;
    };
    let path = panel.panel_path();
    if path.is_empty() {
        return;
    }
    let location = panel.panel_cursor_reference().or_else(|| panel.panel_selection().map(|(_, p)| p));
    let state = serde_json::to_string(&(path, location)).unwrap();
    let history = window().history().unwrap();
    let current = history.state().ok().and_then(|s| s.as_string());
    if current.as_deref() == Some(state.as_str()) {
        return;
    }
    if current.is_none() {
        _ = history.replace_state(&JsValue::from_str(&state), "");
    } else {
        _ = history.push_state(&JsValue::from_str(&state), "");
    }
    return;
}

fn editor_index(editor: &Editor, panel: &Rc<dyn Panel>) -> Option<usize> {
    return editor.panels.borrow().iter().position(|p| panel_same(p, panel));
}

async fn editor_list(editor: &Editor, dir: String, select: Option<String>) -> Rc<dyn Panel> {
    let keys = editor.keys.clone();
    let select = select.or_else(|| editor.selections.borrow().get(&dir).cloned());
    match client_send(ReqList { dir: dir.clone() }).await {
        Ok(listing) => {
            let select = select.or_else(|| listing.location.clone());
            return Rc::new(FilesystemPanel::filesystem_new(keys, listing, select));
        },
        Err(e) => return Rc::new(ErrorPanel::error_new(dir, true, &e)),
    }
}

fn editor_location_touch(editor: &Rc<Editor>) {
    *editor.location_timer.borrow_mut() = Some(Timeout::new(1000, {
        let editor = editor.clone();
        move || {
            *editor.location_timer.borrow_mut() = None;
            editor_history_record(&editor);
            let panel = editor.panels.borrow().get(editor.focus.get()).cloned();
            let Some(panel) = panel else {
                return;
            };
            let Some(location) = panel.panel_cursor_reference() else {
                return;
            };
            let path = panel.panel_path();
            wasm_bindgen_futures::spawn_local(async move {
                _ = client_send(ReqLocationSet {
                    path: path,
                    location: location,
                }).await;
            });
        }
    }));
    return;
}

async fn editor_open(
    keys: Keymap,
    theme: Rc<SpecTheme>,
    host: PanelHost,
    path: String,
    select: Option<String>,
) -> Rc<dyn Panel> {
    let opened = match client_send(ReqOpen { path: path.clone() }).await {
        Ok(opened) => opened,
        Err(e) => return Rc::new(ErrorPanel::error_new(path, false, &e)),
    };
    let built = (|| -> Result<_, String> {
        let spec: SpecSyntax =
            serde_json::from_str(&opened.syntax).map_err(|e| format!("Error parsing syntax JSON: {}", e))?;
        let syntax = Rc::new(Syntax::syntax_resolve(spec, &theme).map_err(|e| format!("Syntax errors:\n{}", e))?);
        let value: serde_json::Value =
            source_parse(&opened.source).map_err(|e| format!("Error parsing source JSON: {}", e))?;
        let document =
            match_document(
                &syntax,
                &value,
            ).map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?;
        return Ok((syntax, document));
    })();
    match built {
        Ok((syntax, document)) => {
            return CodePanel::code_new(keys, path, syntax, document, select.or(opened.location), Some(CodeEdit {
                host: host,
                revision: opened.revision,
            }));
        },
        Err(e) => return Rc::new(ErrorPanel::error_new(path, false, &e)),
    }
}

fn editor_progress(editor: &Editor, now: f64) -> f64 {
    return ezing::quad_out(((now - editor.animation_start.get()) / 250.).clamp(0., 1.));
}

fn editor_result(editor: &Rc<Editor>, index: usize, result: PanelResult) -> bool {
    let action = match result {
        PanelResult::Ignored => return false,
        PanelResult::Used => return true,
        PanelResult::Detail(child) => {
            *editor.child_request.borrow_mut() = None;
            let count = editor.panels.borrow().len();
            editor_splice(editor, index + 1, count - index - 1, vec![child]);
            return true;
        },
        PanelResult::Replace(replacement) => {
            let focused = editor.focus.get() == index;
            editor_splice(editor, index, 1, vec![replacement.clone()]);
            if focused {
                if replacement.panel_focusable() || index == 0 {
                    editor_focus(editor, index);
                } else {
                    editor_focus(editor, index - 1);
                }
            }
            return true;
        },
        PanelResult::Selected => {
            editor_sync(editor, index);
            return true;
        },
        PanelResult::Open(child) => {
            editor_open_child(editor, index, child);
            return true;
        },
        PanelResult::Jump(path, location) => {
            editor_jump(editor, path, location);
            return true;
        },
        PanelResult::JumpRemote(server, source, select) => {
            let request = spawn_rooted({
                let editor: Weak<Editor> = Rc::downgrade(editor);
                let name = lang_source_key(&source).to_string();
                async move {
                    let response = client_send(ReqLangSourceRead {
                        server: server,
                        source: source,
                    }).await;
                    let Some(editor) = editor.upgrade() else {
                        return;
                    };
                    let built = (|| -> Result<_, String> {
                        let response = response?;
                        let Some(text) = response.text else {
                            return Err("The language server has no text for this source".to_string());
                        };
                        let spec: SpecSyntax =
                            serde_json::from_str(
                                &response.syntax,
                            ).map_err(|e| format!("Error parsing syntax JSON: {}", e))?;
                        let syntax =
                            Rc::new(
                                Syntax::syntax_resolve(
                                    spec,
                                    &editor.theme,
                                ).map_err(|e| format!("Syntax errors:\n{}", e))?,
                            );
                        let value: serde_json::Value =
                            source_parse(&text).map_err(|e| format!("Error parsing source JSON: {}", e))?;
                        let document =
                            match_document(
                                &syntax,
                                &value,
                            ).map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?;
                        return Ok((syntax, document));
                    })();
                    let panel: Rc<dyn Panel> = match built {
                        Ok((syntax, document)) => CodePanel::code_new(
                            editor.keys.clone(),
                            name,
                            syntax,
                            document,
                            select,
                            None,
                        ),
                        Err(e) => Rc::new(ErrorPanel::error_new(name, false, &e)),
                    };
                    if index >= editor.panels.borrow().len() {
                        return;
                    }
                    editor_open_child(&editor, index, panel);
                }
            });
            *editor.child_request.borrow_mut() = Some(request);
            return true;
        },
        PanelResult::Unused(action) => action,
    };
    match action {
        Action::Enter => {
            let child = editor.panels.borrow().get(index + 1).cloned();
            match child {
                Some(child) => {
                    if child.panel_focusable() {
                        editor_focus(editor, index + 1);
                    }
                },
                None => editor.focus_next.set(true),
            }
        },
        Action::AiOpen => editor_ai_open(editor, index),
        Action::AiOpenReference => {
            let reference = editor.panels.borrow()[index].panel_reference();
            editor_ai_open(editor, index);
            if let Some(reference) = reference {
                let prefix = format!("{}/", editor.dir);
                editor.ai.ai_append(&format!("{} ", reference.strip_prefix(&prefix).unwrap_or(&reference)));
            }
        },
        Action::HistoryBack => _ = window().history().unwrap().back(),
        Action::HistoryForward => _ = window().history().unwrap().forward(),
        Action::Exit | Action::PanelExit => {
            if index > 0 {
                editor_focus(editor, index - 1);
                return true;
            }
            let panel = editor.panels.borrow()[0].clone();
            let Some(parent) = panel.panel_parent() else {
                return true;
            };
            let request = spawn_rooted({
                let editor: Weak<Editor> = Rc::downgrade(editor);
                async move {
                    let Some(editor) = editor.upgrade() else {
                        return;
                    };
                    let parent = editor_list(&editor, parent, Some(panel.panel_path())).await;
                    if editor_index(&editor, &panel) != Some(0) {
                        return;
                    }
                    editor_splice(&editor, 0, 0, vec![parent]);
                    editor_focus(&editor, 0);
                }
            });
            *editor.parent_request.borrow_mut() = Some(request);
        },
        _ => { },
    }
    return true;
}

fn editor_schedule_reload(editor: &Rc<Editor>) {
    let timer = Timeout::new(100, {
        let editor = editor.clone();
        move || {
            *editor.changed_timer.borrow_mut() = None;
            let paths = std::mem::take(&mut *editor.changed.borrow_mut());
            let panels = editor.panels.borrow().clone();
            for panel in panels {
                let Some(dir) = paths.iter().find_map(|p| match panel.panel_changed(p) {
                    Some(PanelChange::Reload(dir)) => Some(dir),
                    Some(PanelChange::Handled) | None => None,
                }) else {
                    continue;
                };
                let path = panel.panel_path();
                let request = spawn_rooted({
                    let editor: Weak<Editor> = Rc::downgrade(&editor);
                    let path = path.clone();
                    async move {
                        let Some(editor) = editor.upgrade() else {
                            return;
                        };
                        let replacement = if dir {
                            let select = panel.panel_selection().map(|(_, p)| p);
                            editor_list(&editor, path, select).await
                        } else {
                            editor_open(
                                editor.keys.clone(),
                                editor.theme.clone(),
                                editor_host(&editor),
                                path,
                                panel.panel_cursor_reference(),
                            ).await
                        };
                        let Some(index) = editor_index(&editor, &panel) else {
                            return;
                        };
                        let focused = editor.focus.get() == index;
                        editor_splice(&editor, index, 1, vec![replacement.clone()]);
                        if !focused {
                            return;
                        }
                        if replacement.panel_focusable() || index == 0 {
                            editor_focus(&editor, index);
                        } else {
                            editor_focus(&editor, index - 1);
                        }
                    }
                });
                editor.reloads.borrow_mut().insert(path, request);
            }
        }
    });
    *editor.changed_timer.borrow_mut() = Some(timer);
    return;
}

fn editor_show(editor: &Rc<Editor>) {
    let panels = editor.panels.borrow();
    let focus = editor.focus.get();
    let width = editor.element.raw().client_width() as f64;
    let mut wanted = vec![];
    if focus < panels.len() {
        let min_width = |i: usize| panels[i].panel_size() * 96. / 2.54;
        let mut start = focus;
        let mut end = focus + 1;
        let mut used = min_width(focus);
        while end < panels.len() && used + min_width(end) <= width {
            used += min_width(end);
            end += 1;
        }
        while start > 0 && used + min_width(start - 1) <= width {
            start -= 1;
            used += min_width(start);
        }
        let mut edge = 0.;
        for i in start .. end {
            let left = edge;
            edge += min_width(i) * width / used;
            wanted.push((i, left.round(), edge.round() - left.round()));
        }
    }
    let progress = editor_progress(editor, js_sys::Date::now());
    let mut shown = editor.shown.borrow_mut();
    for entry in shown.iter_mut() {
        entry.from = entry.from + (entry.to - entry.from) * progress;
        if wanted.iter().any(|(i, _, _)| panel_same(&panels[*i], &entry.panel)) {
            continue;
        }
        entry.leaving = true;
        let off_left = wanted.first().is_some_and(|(start, _, _)| {
            return panels[..*start].iter().any(|p| panel_same(p, &entry.panel));
        });
        entry.to = if off_left {
            -(entry.element.raw().client_width() as f64)
        } else {
            width
        };
    }
    for (i, left, panel_width) in wanted {
        let panel = &panels[i];
        let entry = match shown.iter_mut().position(|e| panel_same(&e.panel, panel)) {
            Some(existing) => &mut shown[existing],
            None => {
                let element = panel.panel_attach();
                element.ref_own(
                    |e| EventListener::new_with_options(
                        &e.raw(),
                        "mousedown",
                        EventListenerOptions::enable_prevent_default(),
                        {
                            let editor: Weak<Editor> = Rc::downgrade(editor);
                            let panel = panel.clone();
                            move |e| {
                                let e: &MouseEvent = e.dyn_ref().unwrap();
                                let Some(editor) = editor.upgrade() else {
                                    return;
                                };
                                let Some(index) = editor_index(&editor, &panel) else {
                                    return;
                                };
                                if e.button() == 0 && panel.panel_focusable() {
                                    editor_focus(&editor, index);
                                }
                                if editor_result(&editor, index, panel.panel_mouse(e)) {
                                    e.prevent_default();
                                }
                                editor_location_touch(&editor);
                            }
                        },
                    ),
                );
                editor.element.ref_push(element.clone());
                shown.push(Shown {
                    element: element,
                    from: if i < focus {
                        -panel_width
                    } else {
                        width
                    },
                    leaving: false,
                    panel: panel.clone(),
                    to: 0.,
                });
                shown.last_mut().unwrap()
            },
        };
        entry.leaving = false;
        entry.to = left;
        let style = entry.element.raw().dyn_into::<HtmlElement>().unwrap().style();
        _ = style.set_property("width", &format!("{}px", panel_width));
        _ = style.set_property("left", &format!("{}px", entry.from.round()));
        entry.element.ref_modify_classes(&[("merman_panel_focus", i == focus)]);
    }
    drop(shown);
    drop(panels);
    editor.animation_start.set(js_sys::Date::now());
    editor_animate(editor, js_sys::Date::now());
    return;
}

fn editor_splice(editor: &Rc<Editor>, offset: usize, remove: usize, add: Vec<Rc<dyn Panel>>) {
    let focus = editor.focus.get();
    if focus >= offset + remove {
        editor.focus.set(focus - remove + add.len());
    }
    for panel in &add {
        let path = panel.panel_path();
        if !Lang::lang_path_relevant(&path) {
            continue;
        }
        panel.panel_lang_errors(&path, &editor.lang.lang_errors(&path));
        editor.lang.lang_load(path);
    }
    editor.panels.borrow_mut().splice(offset .. offset + remove, add);
    editor_show(editor);
    return;
}

fn editor_sync(editor: &Rc<Editor>, index: usize) {
    let panel = editor.panels.borrow()[index].clone();
    let count = editor.panels.borrow().len();
    let Some((dir, path)) = panel.panel_selection() else {
        editor_splice(editor, index + 1, count - index - 1, vec![]);
        return;
    };
    *editor.child_request.borrow_mut() = None;
    let current = editor.panels.borrow().get(index + 1).map(|p| p.panel_path());
    if current.as_deref() == Some(path.as_str()) {
        return;
    }
    editor_splice(editor, index + 1, count - index - 1, vec![]);
    editor.selections.borrow_mut().insert(panel.panel_path(), path.clone());
    wasm_bindgen_futures::spawn_local({
        let request = ReqLocationSet {
            path: panel.panel_path(),
            location: path.clone(),
        };
        async move {
            _ = client_send(request).await;
        }
    });
    let select =
        editor.history_select.borrow_mut().take_if(|(file, _)| *file == path).map(|(_, location)| location);
    let request = spawn_rooted({
        let keys = editor.keys.clone();
        let theme = editor.theme.clone();
        let host = editor_host(editor);
        let editor: Weak<Editor> = Rc::downgrade(editor);
        async move {
            let child: Rc<dyn Panel> = if dir {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                editor_list(&editor, path, None).await
            } else {
                editor_open(keys, theme, host, path, select).await
            };
            let Some(editor) = editor.upgrade() else {
                return;
            };
            let Some(index) = editor_index(&editor, &panel) else {
                return;
            };
            let child_path = child.panel_path();
            let start =
                editor.start_focus.borrow().as_ref().is_some_and(|file| Path::new(file).starts_with(&child_path));
            if editor.start_focus.borrow().as_deref() == Some(child_path.as_str()) {
                *editor.start_focus.borrow_mut() = None;
            }
            let jump = start || (editor.focus.get() == index && editor.focus_next.get() && child.panel_focusable());
            let count = editor.panels.borrow().len();
            editor_splice(&editor, index + 1, count - index - 1, vec![child]);
            if jump {
                editor_focus(&editor, index + 1);
            }
        }
    });
    *editor.child_request.borrow_mut() = Some(request);
    return;
}

fn panel_same(a: &Rc<dyn Panel>, b: &Rc<dyn Panel>) -> bool {
    return std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b));
}

#[wasm_bindgen]
pub fn start_editor() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        let start = match client_send(ReqStart {}).await {
            Ok(start) => start,
            Err(e) => {
                set_root(vec![el("pre").classes(&["merman_error"]).text(&e)]);
                return;
            },
        };
        let resolved = (|| -> Result<(Keymap, SpecTheme), String> {
            let spec =
                serde_json::from_str::<SpecKeys>(
                    &start.keys,
                ).map_err(|e| format!("Error parsing keys JSON: {}", e))?;
            let keys = Keymap::keymap_resolve(&spec).map_err(|e| format!("Errors in key bindings:\n{}", e))?;
            let theme =
                serde_json::from_str::<SpecTheme>(
                    &start.theme,
                ).map_err(|e| format!("Error parsing theme JSON: {}", e))?;
            return Ok((keys, theme));
        })();
        let (keys, theme) = match resolved {
            Ok(resolved) => resolved,
            Err(e) => {
                set_root(vec![el("pre").classes(&["merman_error"]).text(&e)]);
                return;
            },
        };
        panel_theme_apply(&theme);
        let ai = Ai::ai_new(keys.clone());
        let lang = Lang::lang_new();
        let editor = Rc::new(Editor {
            animation: RefCell::new(None),
            animation_start: Cell::new(0.),
            keys: keys,
            dir: start.dir.clone(),
            ai: ai.clone(),
            lang: lang.clone(),
            lang_panel: RefCell::new(None),
            status_panel: el("div").classes(&["merman_status_panel"]),
            element: el("div").classes(&["merman_panels"]),
            panels: RefCell::new(vec![]),
            shown: RefCell::new(vec![]),
            focus: Cell::new(0),
            focus_next: Cell::new(false),
            history_navigating: Cell::new(false),
            history_select: RefCell::new(None),
            selections: RefCell::new(HashMap::new()),
            child_request: RefCell::new(None),
            parent_request: RefCell::new(None),
            changed: RefCell::new(vec![]),
            changed_timer: RefCell::new(None),
            reloads: RefCell::new(HashMap::new()),
            socket: RefCell::new(None),
            last_seq: Cell::new(None),
            reconnect: RefCell::new(None),
            location_timer: RefCell::new(None),
            start_focus: RefCell::new(start.file.clone()),
            theme: Rc::new(theme),
        });
        *lang.changed.borrow_mut() = Some(Rc::new({
            let editor: Weak<Editor> = Rc::downgrade(&editor);
            move |path: &str| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                let errors = editor.lang.lang_errors(path);
                let panels = editor.panels.borrow().clone();
                for panel in panels {
                    panel.panel_lang_errors(path, &errors);
                }
                editor_status_refresh(&editor);
            }
        }));
        if let Some(file) = &start.file {
            let mut selections = editor.selections.borrow_mut();
            let mut child = Path::new(file);
            while let Some(parent) = child.parent().filter(|p| p.starts_with(&start.dir)) {
                selections.insert(parent.to_string_lossy().into_owned(), child.to_string_lossy().into_owned());
                child = parent;
            }
        }
        let root = editor_list(&editor, start.dir, None).await;
        editor
            .element
            .ref_own(
                |_| EventListener::new_with_options(
                    &document(),
                    "keydown",
                    EventListenerOptions::enable_prevent_default(),
                    {
                        let editor = editor.clone();
                        move |e| {
                            let e: &KeyboardEvent = e.dyn_ref().unwrap();
                            let index = editor.focus.get();
                            let panel = editor.panels.borrow().get(index).cloned();
                            let Some(panel) = panel else {
                                return;
                            };
                            if editor_result(&editor, index, panel.panel_key(e)) {
                                e.prevent_default();
                                editor_location_touch(&editor);
                            }
                        }
                    },
                ),
            );
        ai.icon.ref_on("click", {
            let editor: Weak<Editor> = Rc::downgrade(&editor);
            move |_| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                editor_ai_open(&editor, editor.focus.get());
            }
        });
        let back =
            el("span")
                .classes(&["merman_status_icon", "merman_status_back"])
                .attr("title", "Back")
                .text("\u{e5c4}");
        back.ref_on("click", {
            let editor: Weak<Editor> = Rc::downgrade(&editor);
            move |_| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                editor_result(&editor, editor.focus.get(), PanelResult::Unused(Action::PanelExit));
            }
        });
        editor.element.ref_own(|_| EventListener::new(&window(), "popstate", {
            let editor: Weak<Editor> = Rc::downgrade(&editor);
            move |e| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                let e: &PopStateEvent = e.dyn_ref().unwrap();
                let Some(state) = e.state().as_string() else {
                    return;
                };
                let Ok((path, location)) = serde_json::from_str::<(String, Option<String>)>(&state) else {
                    return;
                };
                editor.history_navigating.set(true);
                editor_jump(&editor, path, location);
                editor.history_navigating.set(false);
            }
        }));
        if js_sys::Reflect::get(&window(), &JsValue::from_str("mermanWebview")).ok().and_then(|v| v.as_bool()) ==
            Some(true) {
            editor
                .element
                .ref_own(
                    |_| EventListener::new_with_options(
                        &document(),
                        "mouseup",
                        EventListenerOptions::enable_prevent_default(),
                        {
                            let editor: Weak<Editor> = Rc::downgrade(&editor);
                            move |e| {
                                let Some(editor) = editor.upgrade() else {
                                    return;
                                };
                                let e: &MouseEvent = e.dyn_ref().unwrap();
                                let action = match e.button() {
                                    3 => Action::HistoryBack,
                                    4 => Action::HistoryForward,
                                    _ => return,
                                };
                                e.prevent_default();
                                editor_result(&editor, editor.focus.get(), PanelResult::Unused(action));
                            }
                        },
                    ),
                );
        }
        editor.element.ref_on_resize({
            let editor: Weak<Editor> = Rc::downgrade(&editor);
            move |_, _, _| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                editor_show(&editor);
            }
        });
        editor_connect(&editor);
        set_root(
            vec![
                el("div")
                    .classes(&["merman_root"])
                    .push(
                        el("div")
                            .classes(&["merman_status"])
                            .push(back)
                            .push(ai.icon.clone())
                            .push(editor.status_panel.clone()),
                    )
                    .push(editor.element.clone()),
            ],
        );
        editor_splice(&editor, 0, 0, vec![root]);
        editor_focus(&editor, 0);
        set_root_non_dom(editor);
    });
}
