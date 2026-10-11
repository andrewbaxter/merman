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
            editor::EditorPanel,
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
        AiStatus,
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

struct Ide {
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
    editor_panel: RefCell<Option<(String, Rc<dyn Panel>)>>,
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

fn ide_connect(ide: &Rc<Ide>) {
    let location = window().location();
    let scheme = if location.protocol().unwrap() == "https:" {
        "wss"
    } else {
        "ws"
    };
    let socket = WebSocket::new(&format!("{}://{}{}", scheme, location.host().unwrap(), WS_PATH)).unwrap();
    let open = EventListener::new(&socket, "open", {
        let ide = ide.clone();
        let socket = socket.clone();
        move |_| {
            socket
                .send_with_str(&serde_json::to_string(&WsClient { since: ide.last_seq.get() }).unwrap())
                .unwrap();
        }
    });
    let message = EventListener::new(&socket, "message", {
        let ide = ide.clone();
        move |e| {
            let e: &MessageEvent = e.dyn_ref().unwrap();
            let data = e.data().as_string().unwrap();
            let event = match serde_json::from_str::<WsServer>(&data).unwrap() {
                WsServer::Gap => {
                    ide.ai.ai_load();
                    let paths = ide.panels.borrow().iter().map(|p| p.panel_path()).collect::<Vec<_>>();
                    for path in &paths {
                        if Lang::lang_path_relevant(path) {
                            ide.lang.lang_load(path.clone());
                        }
                    }
                    ide.changed.borrow_mut().extend(paths);
                    ide_schedule_reload(&ide);
                    return;
                },
                WsServer::Event { seq, event } => {
                    if ide.last_seq.get().is_some_and(|last| seq <= last) {
                        return;
                    }
                    ide.last_seq.set(Some(seq));
                    event
                },
            };
            match event {
                Event::FileChanged { path } => {
                    ide.changed.borrow_mut().push(path);
                    ide_schedule_reload(&ide);
                },
                Event::Ai { message } => ide.ai.ai_message(message),
                Event::AiStatus { status } => ide.ai.ai_status(status),
                Event::LangServer { server, announce } => ide.lang.lang_announce(&server, announce),
                Event::LangServerStatus { server, running } => {
                    ide.lang.lang_status(&server, running);
                    ide_status_refresh(&ide);
                },
            }
        }
    });
    let close = EventListener::new(&socket, "close", {
        let ide = ide.clone();
        move |_| {
            *ide.reconnect.borrow_mut() = Some(Timeout::new(1000, {
                let ide = ide.clone();
                move || ide_connect(&ide)
            }));
        }
    });
    *ide.socket.borrow_mut() = Some((socket, vec![open, message, close]));
    return;
}

fn ide_animate(ide: &Rc<Ide>, now: f64) {
    let progress = ide_progress(ide, now);
    let mut shown = ide.shown.borrow_mut();
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
        *ide.animation.borrow_mut() = Some(request_animation_frame({
            let ide: Weak<Ide> = Rc::downgrade(ide);
            move |_| {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                ide_animate(&ide, js_sys::Date::now());
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
        ide.element.ref_splice(i, 1, vec![]);
        shown.remove(i).panel.panel_detach();
    }
    return;
}

fn ide_ai_open(ide: &Rc<Ide>, index: usize) {
    let ai: Rc<dyn Panel> = ide.ai.clone();
    if let Some(existing) = ide_index(ide, &ai) {
        ide_focus(ide, existing);
        return;
    }
    ide_open_child(ide, index, ai);
    return;
}

fn ide_open_child(ide: &Rc<Ide>, index: usize, child: Rc<dyn Panel>) {
    *ide.child_request.borrow_mut() = None;
    let count = ide.panels.borrow().len();
    ide_splice(ide, index + 1, count - index - 1, vec![child]);
    ide_focus(ide, index + 1);
    return;
}

fn ide_focus(ide: &Rc<Ide>, index: usize) {
    if let Some(active) = document().active_element() {
        _ = active.dyn_into::<HtmlElement>().map(|active| active.blur());
    }
    ide.focus.set(index);
    ide.focus_next.set(false);
    let count = ide.panels.borrow().len();
    ide_splice(ide, (index + 2).min(count), count.saturating_sub(index + 2), vec![]);
    let (panel, child) = {
        let panels = ide.panels.borrow();
        (panels[index].clone(), panels.get(index + 1).cloned())
    };
    if let Some(child) = child {
        child.panel_focused(false);
    }
    panel.panel_focused(true);
    ide_show(ide);
    ide_sync(ide, index);
    ide_history_record(ide);
    ide_status_refresh(ide);
    return;
}

fn ide_jump(ide: &Rc<Ide>, path: String, location: Option<String>) {
    *ide.location_timer.borrow_mut() = None;
    let panels = ide.panels.borrow().clone();
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
            let mut selections = ide.selections.borrow_mut();
            while let Some(parent) = child.parent().filter(|p| *p != Path::new(&panel.panel_path())) {
                selections.insert(parent.to_string_lossy().into_owned(), child.to_string_lossy().into_owned());
                child = parent;
            }
            if let Some(location) = &location {
                selections.insert(path.clone(), location.clone());
                *ide.history_select.borrow_mut() = Some((path.clone(), location.clone()));
            }
        }
        *ide.start_focus.borrow_mut() = Some(path.clone());
        panel.panel_select(&child.to_string_lossy());
    }
    ide_focus(ide, index);
    return;
}

fn ide_status_refresh(ide: &Rc<Ide>) {
    ide.status_panel.ref_clear();
    let panel = ide.panels.borrow().get(ide.focus.get()).cloned();
    let Some(panel) = panel else {
        return;
    };
    let path = panel.panel_path();
    let Some(file) = ide.lang.lang_file(&path) else {
        return;
    };
    if !file.dirty.is_empty() {
        ide
            .status_panel
            .ref_push(
                el("span")
                    .classes(&["merman_status_icon", "merman_status_spin"])
                    .attr("title", "Compiling")
                    .text("\u{e863}"),
            );
    }
    let errors = ide.lang.lang_errors(&path).len();
    let logs =
        el("span")
            .classes(&["merman_status_icon", "merman_status_badge_holder"])
            .attr("title", "Compile logs and errors")
            .text("\u{e0ee}");
    if errors > 0 {
        logs.ref_push(el("span").classes(&["merman_status_badge"]).text(&errors.to_string()));
    }
    logs.ref_modify_classes(&[("merman_status_off", !ide.lang.lang_running())]);
    logs.ref_on("click", {
        let ide: Weak<Ide> = Rc::downgrade(ide);
        let path = path.clone();
        move |_| {
            let Some(ide) = ide.upgrade() else {
                return;
            };
            if let Some((shown, panel)) = ide.lang_panel.borrow().as_ref() {
                if *shown == path {
                    if let Some(existing) = ide_index(&ide, panel) {
                        ide_focus(&ide, existing);
                        return;
                    }
                }
            }
            let panel: Rc<dyn Panel> =
                Rc::new(LangPanel::lang_new(ide.keys.clone(), ide.lang.clone(), path.clone()));
            *ide.lang_panel.borrow_mut() = Some((path.clone(), panel.clone()));
            ide_open_child(&ide, ide.focus.get(), panel);
        }
    });
    ide.status_panel.ref_push(logs);
    if ide.lang.lang_editor(&path).is_some() {
        let button =
            el("span").classes(&["merman_status_icon"]).attr("title", "Open the file's editor").text("\u{e87b}");
        button.ref_on("click", {
            let ide: Weak<Ide> = Rc::downgrade(ide);
            move |_| {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                ide_editor_open(&ide, ide.focus.get());
            }
        });
        ide.status_panel.ref_push(button);
    }
    return;
}

/// Opens the editor the file at `index` exports, or returns to it if it's already
/// open.
fn ide_editor_open(ide: &Rc<Ide>, index: usize) {
    let path = ide.panels.borrow()[index].panel_path();
    let Some(export) = ide.lang.lang_editor(&path) else {
        return;
    };
    let existing = ide.editor_panel.borrow().clone();
    if let Some((shown, panel)) = existing {
        if shown == path {
            if let Some(existing) = ide_index(ide, &panel) {
                ide_focus(ide, existing);
                return;
            }
        }
    }
    let panel: Rc<dyn Panel> =
        Rc::new(
            EditorPanel::editor_new(
                ide.keys.clone(),
                ide.lang.clone(),
                path.clone(),
                export.server,
                export.source,
                export.import_state,
            ),
        );
    *ide.editor_panel.borrow_mut() = Some((path, panel.clone()));
    ide_open_child(ide, index, panel);
    return;
}

fn ide_host(ide: &Rc<Ide>) -> PanelHost {
    let ide: Weak<Ide> = Rc::downgrade(ide);
    return PanelHost(Rc::new(move |panel, result| {
        let Some(ide) = ide.upgrade() else {
            return;
        };
        let Some(index) = ide_index(&ide, &panel) else {
            return;
        };
        ide_result(&ide, index, result);
    }));
}

fn ide_history_record(ide: &Ide) {
    if ide.history_navigating.get() || ide.start_focus.borrow().is_some() {
        return;
    }
    let Some(panel) = ide.panels.borrow().get(ide.focus.get()).cloned() else {
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

fn ide_index(ide: &Ide, panel: &Rc<dyn Panel>) -> Option<usize> {
    return ide.panels.borrow().iter().position(|p| panel_same(p, panel));
}

async fn ide_list(ide: &Ide, dir: String, select: Option<String>) -> Rc<dyn Panel> {
    let keys = ide.keys.clone();
    let select = select.or_else(|| ide.selections.borrow().get(&dir).cloned());
    match client_send(ReqList { dir: dir.clone() }).await {
        Ok(listing) => {
            let select = select.or_else(|| listing.location.clone());
            return Rc::new(FilesystemPanel::filesystem_new(keys, listing, select));
        },
        Err(e) => return Rc::new(ErrorPanel::error_new(dir, true, &e)),
    }
}

fn ide_location_touch(ide: &Rc<Ide>) {
    *ide.location_timer.borrow_mut() = Some(Timeout::new(1000, {
        let ide = ide.clone();
        move || {
            *ide.location_timer.borrow_mut() = None;
            ide_history_record(&ide);
            let panel = ide.panels.borrow().get(ide.focus.get()).cloned();
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

async fn ide_open(
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

fn ide_progress(ide: &Ide, now: f64) -> f64 {
    return ezing::quad_out(((now - ide.animation_start.get()) / 250.).clamp(0., 1.));
}

fn ide_result(ide: &Rc<Ide>, index: usize, result: PanelResult) -> bool {
    let action = match result {
        PanelResult::Ignored => return false,
        PanelResult::Used => return true,
        PanelResult::Detail(child) => {
            *ide.child_request.borrow_mut() = None;
            let count = ide.panels.borrow().len();
            ide_splice(ide, index + 1, count - index - 1, vec![child]);
            return true;
        },
        PanelResult::Replace(replacement) => {
            let focused = ide.focus.get() == index;
            ide_splice(ide, index, 1, vec![replacement.clone()]);
            if focused {
                if replacement.panel_focusable() || index == 0 {
                    ide_focus(ide, index);
                } else {
                    ide_focus(ide, index - 1);
                }
            }
            return true;
        },
        PanelResult::Selected => {
            ide_sync(ide, index);
            return true;
        },
        PanelResult::Open(child) => {
            ide_open_child(ide, index, child);
            return true;
        },
        PanelResult::Jump(path, location) => {
            ide_jump(ide, path, location);
            return true;
        },
        PanelResult::JumpRemote(server, source, select) => {
            let request = spawn_rooted({
                let ide: Weak<Ide> = Rc::downgrade(ide);
                let name = lang_source_key(&source).to_string();
                async move {
                    let response = client_send(ReqLangSourceRead {
                        server: server,
                        source: source,
                    }).await;
                    let Some(ide) = ide.upgrade() else {
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
                                    &ide.theme,
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
                            ide.keys.clone(),
                            name,
                            syntax,
                            document,
                            select,
                            None,
                        ),
                        Err(e) => Rc::new(ErrorPanel::error_new(name, false, &e)),
                    };
                    if index >= ide.panels.borrow().len() {
                        return;
                    }
                    ide_open_child(&ide, index, panel);
                }
            });
            *ide.child_request.borrow_mut() = Some(request);
            return true;
        },
        PanelResult::Unused(action) => action,
    };
    match action {
        Action::Enter => {
            let child = ide.panels.borrow().get(index + 1).cloned();
            match child {
                Some(child) => {
                    if child.panel_focusable() {
                        ide_focus(ide, index + 1);
                    }
                },
                None => ide.focus_next.set(true),
            }
        },
        Action::AiOpen => ide_ai_open(ide, index),
        Action::EditorOpen => ide_editor_open(ide, index),
        Action::AiOpenReference => {
            let reference = ide.panels.borrow()[index].panel_reference();
            ide_ai_open(ide, index);
            if let Some(reference) = reference {
                let prefix = format!("{}/", ide.dir);
                ide.ai.ai_append(&format!("{} ", reference.strip_prefix(&prefix).unwrap_or(&reference)));
            }
        },
        Action::HistoryBack => _ = window().history().unwrap().back(),
        Action::HistoryForward => _ = window().history().unwrap().forward(),
        Action::Exit | Action::PanelExit => {
            if index > 0 {
                ide_focus(ide, index - 1);
                return true;
            }
            let panel = ide.panels.borrow()[0].clone();
            let Some(parent) = panel.panel_parent() else {
                return true;
            };
            let request = spawn_rooted({
                let ide: Weak<Ide> = Rc::downgrade(ide);
                async move {
                    let Some(ide) = ide.upgrade() else {
                        return;
                    };
                    let parent = ide_list(&ide, parent, Some(panel.panel_path())).await;
                    if ide_index(&ide, &panel) != Some(0) {
                        return;
                    }
                    ide_splice(&ide, 0, 0, vec![parent]);
                    ide_focus(&ide, 0);
                }
            });
            *ide.parent_request.borrow_mut() = Some(request);
        },
        _ => { },
    }
    return true;
}

fn ide_schedule_reload(ide: &Rc<Ide>) {
    let timer = Timeout::new(100, {
        let ide = ide.clone();
        move || {
            *ide.changed_timer.borrow_mut() = None;
            let paths = std::mem::take(&mut *ide.changed.borrow_mut());
            let panels = ide.panels.borrow().clone();
            for panel in panels {
                let Some(dir) = paths.iter().find_map(|p| match panel.panel_changed(p) {
                    Some(PanelChange::Reload(dir)) => Some(dir),
                    Some(PanelChange::Handled) | None => None,
                }) else {
                    continue;
                };
                let path = panel.panel_path();
                let request = spawn_rooted({
                    let ide: Weak<Ide> = Rc::downgrade(&ide);
                    let path = path.clone();
                    async move {
                        let Some(ide) = ide.upgrade() else {
                            return;
                        };
                        let replacement = if dir {
                            let select = panel.panel_selection().map(|(_, p)| p);
                            ide_list(&ide, path, select).await
                        } else {
                            ide_open(
                                ide.keys.clone(),
                                ide.theme.clone(),
                                ide_host(&ide),
                                path,
                                panel.panel_cursor_reference(),
                            ).await
                        };
                        let Some(index) = ide_index(&ide, &panel) else {
                            return;
                        };
                        let focused = ide.focus.get() == index;
                        ide_splice(&ide, index, 1, vec![replacement.clone()]);
                        if !focused {
                            return;
                        }
                        if replacement.panel_focusable() || index == 0 {
                            ide_focus(&ide, index);
                        } else {
                            ide_focus(&ide, index - 1);
                        }
                    }
                });
                ide.reloads.borrow_mut().insert(path, request);
            }
        }
    });
    *ide.changed_timer.borrow_mut() = Some(timer);
    return;
}

fn ide_show(ide: &Rc<Ide>) {
    let panels = ide.panels.borrow();
    let focus = ide.focus.get();
    let width = ide.element.raw().client_width() as f64;
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
    let progress = ide_progress(ide, js_sys::Date::now());
    let mut shown = ide.shown.borrow_mut();
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
                            let ide: Weak<Ide> = Rc::downgrade(ide);
                            let panel = panel.clone();
                            move |e| {
                                let e: &MouseEvent = e.dyn_ref().unwrap();
                                let Some(ide) = ide.upgrade() else {
                                    return;
                                };
                                let Some(index) = ide_index(&ide, &panel) else {
                                    return;
                                };
                                if e.button() == 0 && panel.panel_focusable() {
                                    ide_focus(&ide, index);
                                }
                                if ide_result(&ide, index, panel.panel_mouse(e)) {
                                    e.prevent_default();
                                }
                                ide_location_touch(&ide);
                            }
                        },
                    ),
                );
                ide.element.ref_push(element.clone());
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
    ide.animation_start.set(js_sys::Date::now());
    ide_animate(ide, js_sys::Date::now());
    return;
}

fn ide_splice(ide: &Rc<Ide>, offset: usize, remove: usize, add: Vec<Rc<dyn Panel>>) {
    let focus = ide.focus.get();
    if focus >= offset + remove {
        ide.focus.set(focus - remove + add.len());
    }
    for panel in &add {
        panel.panel_editable(ide.ai.status.get() == AiStatus::Off);
        let path = panel.panel_path();
        if !Lang::lang_path_relevant(&path) {
            continue;
        }
        panel.panel_lang_errors(&path, &ide.lang.lang_errors(&path));
        ide.lang.lang_load(path);
    }
    ide.panels.borrow_mut().splice(offset .. offset + remove, add);
    ide_show(ide);
    return;
}

fn ide_sync(ide: &Rc<Ide>, index: usize) {
    let panel = ide.panels.borrow()[index].clone();
    let count = ide.panels.borrow().len();
    let Some((dir, path)) = panel.panel_selection() else {
        ide_splice(ide, index + 1, count - index - 1, vec![]);
        return;
    };
    *ide.child_request.borrow_mut() = None;
    let current = ide.panels.borrow().get(index + 1).map(|p| p.panel_path());
    if current.as_deref() == Some(path.as_str()) {
        return;
    }
    ide_splice(ide, index + 1, count - index - 1, vec![]);
    ide.selections.borrow_mut().insert(panel.panel_path(), path.clone());
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
        ide.history_select.borrow_mut().take_if(|(file, _)| *file == path).map(|(_, location)| location);
    let request = spawn_rooted({
        let keys = ide.keys.clone();
        let theme = ide.theme.clone();
        let host = ide_host(ide);
        let ide: Weak<Ide> = Rc::downgrade(ide);
        async move {
            let child: Rc<dyn Panel> = if dir {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                ide_list(&ide, path, None).await
            } else {
                ide_open(keys, theme, host, path, select).await
            };
            let Some(ide) = ide.upgrade() else {
                return;
            };
            let Some(index) = ide_index(&ide, &panel) else {
                return;
            };
            let child_path = child.panel_path();
            let start =
                ide.start_focus.borrow().as_ref().is_some_and(|file| Path::new(file).starts_with(&child_path));
            if ide.start_focus.borrow().as_deref() == Some(child_path.as_str()) {
                *ide.start_focus.borrow_mut() = None;
            }
            let jump = start || (ide.focus.get() == index && ide.focus_next.get() && child.panel_focusable());
            let count = ide.panels.borrow().len();
            ide_splice(&ide, index + 1, count - index - 1, vec![child]);
            if jump {
                ide_focus(&ide, index + 1);
            }
        }
    });
    *ide.child_request.borrow_mut() = Some(request);
    return;
}

fn panel_same(a: &Rc<dyn Panel>, b: &Rc<dyn Panel>) -> bool {
    return std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b));
}

#[wasm_bindgen]
pub fn start_ide() {
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
        let ide = Rc::new(Ide {
            animation: RefCell::new(None),
            animation_start: Cell::new(0.),
            keys: keys,
            dir: start.dir.clone(),
            ai: ai.clone(),
            lang: lang.clone(),
            editor_panel: RefCell::new(None),
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
        *ai.changed.borrow_mut() = Some(Rc::new({
            let ide: Weak<Ide> = Rc::downgrade(&ide);
            move |status: AiStatus| {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                let panels = ide.panels.borrow().clone();
                for panel in panels {
                    panel.panel_editable(status == AiStatus::Off);
                }
            }
        }));
        *lang.changed.borrow_mut() = Some(Rc::new({
            let ide: Weak<Ide> = Rc::downgrade(&ide);
            move |path: &str| {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                let errors = ide.lang.lang_errors(path);
                let panels = ide.panels.borrow().clone();
                for panel in panels {
                    panel.panel_lang_errors(path, &errors);
                }
                ide_status_refresh(&ide);
            }
        }));
        if let Some(file) = &start.file {
            let mut selections = ide.selections.borrow_mut();
            let mut child = Path::new(file);
            while let Some(parent) = child.parent().filter(|p| p.starts_with(&start.dir)) {
                selections.insert(parent.to_string_lossy().into_owned(), child.to_string_lossy().into_owned());
                child = parent;
            }
        }
        let root = ide_list(&ide, start.dir, None).await;
        ide
            .element
            .ref_own(
                |_| EventListener::new_with_options(
                    &document(),
                    "keydown",
                    EventListenerOptions::enable_prevent_default(),
                    {
                        let ide = ide.clone();
                        move |e| {
                            let e: &KeyboardEvent = e.dyn_ref().unwrap();
                            let index = ide.focus.get();
                            let panel = ide.panels.borrow().get(index).cloned();
                            let Some(panel) = panel else {
                                return;
                            };
                            if ide_result(&ide, index, panel.panel_key(e)) {
                                e.prevent_default();
                                ide_location_touch(&ide);
                            }
                        }
                    },
                ),
            );
        ai.icon.ref_on("click", {
            let ide: Weak<Ide> = Rc::downgrade(&ide);
            move |_| {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                ide_ai_open(&ide, ide.focus.get());
            }
        });
        let back =
            el("span")
                .classes(&["merman_status_icon", "merman_status_back"])
                .attr("title", "Back")
                .text("\u{e5c4}");
        back.ref_on("click", {
            let ide: Weak<Ide> = Rc::downgrade(&ide);
            move |_| {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                ide_result(&ide, ide.focus.get(), PanelResult::Unused(Action::PanelExit));
            }
        });
        ide.element.ref_own(|_| EventListener::new(&window(), "popstate", {
            let ide: Weak<Ide> = Rc::downgrade(&ide);
            move |e| {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                let e: &PopStateEvent = e.dyn_ref().unwrap();
                let Some(state) = e.state().as_string() else {
                    return;
                };
                let Ok((path, location)) = serde_json::from_str::<(String, Option<String>)>(&state) else {
                    return;
                };
                ide.history_navigating.set(true);
                ide_jump(&ide, path, location);
                ide.history_navigating.set(false);
            }
        }));
        if js_sys::Reflect::get(&window(), &JsValue::from_str("mermanWebview")).ok().and_then(|v| v.as_bool()) ==
            Some(true) {
            ide
                .element
                .ref_own(
                    |_| EventListener::new_with_options(
                        &document(),
                        "mouseup",
                        EventListenerOptions::enable_prevent_default(),
                        {
                            let ide: Weak<Ide> = Rc::downgrade(&ide);
                            move |e| {
                                let Some(ide) = ide.upgrade() else {
                                    return;
                                };
                                let e: &MouseEvent = e.dyn_ref().unwrap();
                                let action = match e.button() {
                                    3 => Action::HistoryBack,
                                    4 => Action::HistoryForward,
                                    _ => return,
                                };
                                e.prevent_default();
                                ide_result(&ide, ide.focus.get(), PanelResult::Unused(action));
                            }
                        },
                    ),
                );
        }
        ide.element.ref_on_resize({
            let ide: Weak<Ide> = Rc::downgrade(&ide);
            move |_, _, _| {
                let Some(ide) = ide.upgrade() else {
                    return;
                };
                ide_show(&ide);
            }
        });
        ide_connect(&ide);
        set_root(
            vec![
                el("div")
                    .classes(&["merman_root"])
                    .push(
                        el("div")
                            .classes(&["merman_status"])
                            .push(back)
                            .push(ai.icon.clone())
                            .push(ide.status_panel.clone()),
                    )
                    .push(ide.element.clone()),
            ],
        );
        ide_splice(&ide, 0, 0, vec![root]);
        ide_focus(&ide, 0);
        set_root_non_dom(ide);
    });
}
