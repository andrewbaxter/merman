use {
    crate::{
        ai::Ai,
        client::client_send,
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
            panel_theme_apply,
        },
    },
    futures::channel::oneshot::Receiver,
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
        WebSocket,
    },
};

struct Editor {
    ai: Rc<Ai>,
    changed: RefCell<Vec<String>>,
    changed_timer: RefCell<Option<Timeout>>,
    child_request: RefCell<Option<Receiver<()>>>,
    dir: String,
    element: El,
    focus: Cell<usize>,
    focus_next: Cell<bool>,
    keys: Keymap,
    last_seq: Cell<Option<u64>>,
    location_timer: RefCell<Option<Timeout>>,
    panels: RefCell<Vec<Rc<dyn Panel>>>,
    parent_request: RefCell<Option<Receiver<()>>>,
    reconnect: RefCell<Option<Timeout>>,
    reloads: RefCell<HashMap<String, Receiver<()>>>,
    selections: RefCell<HashMap<String, String>>,
    shown: RefCell<Vec<(Rc<dyn Panel>, El)>>,
    socket: RefCell<Option<(WebSocket, Vec<EventListener>)>>,
    start: Cell<usize>,
    start_focus: RefCell<Option<String>>,
    theme: Rc<SpecTheme>,
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
    editor.start.set((index + 2).min(count).saturating_sub(2));
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
        Action::Exit => {
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
    let start = editor.start.get().clamp(focus.saturating_sub(1), focus).min(panels.len());
    editor.start.set(start);
    let wanted = &panels[start .. (start + 2).min(panels.len())];
    let mut shown = editor.shown.borrow_mut();
    let mut i = 0;
    while i < shown.len() {
        if wanted.iter().any(|w| panel_same(w, &shown[i].0)) {
            i += 1;
            continue;
        }
        editor.element.ref_splice(i, 1, vec![]);
        let (panel, _) = shown.remove(i);
        panel.panel_detach();
    }
    for (i, panel) in wanted.iter().enumerate() {
        if shown.iter().any(|(p, _)| panel_same(p, panel)) {
            continue;
        }
        let element = panel.panel_attach();
        _ =
            element
                .raw()
                .dyn_into::<HtmlElement>()
                .unwrap()
                .style()
                .set_property("flex-grow", &panel.panel_size().to_string());
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
        editor.element.ref_splice(i, 0, vec![element.clone()]);
        shown.insert(i, (panel.clone(), element));
    }
    for (panel, element) in shown.iter() {
        let focused = panels.get(focus).is_some_and(|f| panel_same(panel, f));
        element.ref_modify_classes(&[("merman_panel_focus", focused)]);
        if focused {
            element.raw().scroll_into_view();
        }
    }
    return;
}

fn editor_splice(editor: &Rc<Editor>, offset: usize, remove: usize, add: Vec<Rc<dyn Panel>>) {
    let focus = editor.focus.get();
    if focus >= offset + remove {
        editor.focus.set(focus - remove + add.len());
    }
    let start = editor.start.get();
    if start >= offset + remove {
        editor.start.set(start - remove + add.len());
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
                editor_open(keys, theme, host, path, None).await
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
        let editor = Rc::new(Editor {
            keys: keys,
            dir: start.dir.clone(),
            ai: ai.clone(),
            element: el("div").classes(&["merman_panels"]),
            panels: RefCell::new(vec![]),
            shown: RefCell::new(vec![]),
            focus: Cell::new(0),
            focus_next: Cell::new(false),
            selections: RefCell::new(HashMap::new()),
            child_request: RefCell::new(None),
            parent_request: RefCell::new(None),
            changed: RefCell::new(vec![]),
            changed_timer: RefCell::new(None),
            reloads: RefCell::new(HashMap::new()),
            socket: RefCell::new(None),
            start: Cell::new(0),
            last_seq: Cell::new(None),
            reconnect: RefCell::new(None),
            location_timer: RefCell::new(None),
            start_focus: RefCell::new(start.file.clone()),
            theme: Rc::new(theme),
        });
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
        editor_connect(&editor);
        set_root(
            vec![el("div").classes(&["merman_root"]).push(ai.status.clone()).push(editor.element.clone())],
        );
        editor_splice(&editor, 0, 0, vec![root]);
        editor_focus(&editor, 0);
        set_root_non_dom(editor);
    });
}
