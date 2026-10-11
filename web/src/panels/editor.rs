use {
    crate::{
        client::client_send,
        lang::Lang,
        panels::{
            Panel,
            PanelChange,
            PanelResult,
            panel_key_stroke,
            toolbar::toolbar_action,
        },
    },
    base64::Engine,
    gloo_events::EventListener,
    merman_api::{
        ReqDocEdit,
        ReqDocOpen,
        ReqDocStep,
        ReqDocSync,
        ReqDocWrite,
        ReqFileRead,
        ReqFileWrite,
        ReqLangExportRead,
    },
    merman_core::{
        direction::DirectionConvert,
        keys::{
            Action,
            KeyName,
            KeyResolve,
            KeyStroke,
            Keymap,
        },
        spec::SpecDirection,
    },
    merman_langserver::{
        CompileError,
        Source,
    },
    rooting::{
        El,
        el,
    },
    std::{
        any::Any,
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
        closure::Closure,
    },
    wasm_bindgen_futures::JsFuture,
    web_sys::{
        CanvasRenderingContext2d,
        HtmlCanvasElement,
        KeyboardEvent,
        MouseEvent,
        PointerEvent,
    },
};

const POINTER_DOWN: i32 = 0;
const POINTER_MOVE: i32 = 1;

pub struct EditorPanel(Rc<State>);

struct State {
    buttons: El,
    canvas: El,
    closures: RefCell<Vec<Box<dyn Any>>>,
    context: CanvasRenderingContext2d,
    doc: RefCell<Option<Doc>>,
    element: El,
    import_state: i64,
    instance: RefCell<Option<Instance>>,
    keys: Keymap,
    lang: Rc<Lang>,
    loaded: Cell<Option<i64>>,
    path: String,
    pending: RefCell<Vec<KeyStroke>>,
    server: String,
    source: Source,
    started: f64,
    status: El,
}

struct Instance {
    exports: js_sys::Object,
    memory: js_sys::WebAssembly::Memory,
}

struct Doc {
    editor: String,
    path: String,
    revision: u64,
}

impl EditorPanel {
    pub fn editor_new(
        keys: Keymap,
        lang: Rc<Lang>,
        path: String,
        server: String,
        source: Source,
        import_state: i64,
    ) -> EditorPanel {
        let buttons = el("div").classes(&["merman_toolbar"]);
        let canvas = el("canvas").classes(&["merman_editor_canvas"]);
        let status = el("div").classes(&["merman_editor_status"]).text("Loading");
        let element =
            el("div")
                .classes(&["merman_editor"])
                .push(buttons.clone())
                .push(canvas.clone())
                .push(status.clone());
        let context: CanvasRenderingContext2d =
            canvas
                .raw()
                .dyn_into::<HtmlCanvasElement>()
                .unwrap()
                .get_context("2d")
                .unwrap()
                .unwrap()
                .dyn_into()
                .unwrap();
        let state = Rc::new(State {
            buttons: buttons,
            canvas: canvas.clone(),
            closures: RefCell::new(vec![]),
            context: context,
            doc: RefCell::new(None),
            element: element,
            import_state: import_state,
            instance: RefCell::new(None),
            keys: keys,
            lang: lang,
            loaded: Cell::new(None),
            path: path,
            pending: RefCell::new(vec![]),
            server: server,
            source: source,
            started: js_sys::Date::now(),
            status: status,
        });
        canvas.ref_on_resize({
            let state: Weak<State> = Rc::downgrade(&state);
            move |_, _, _| {
                let Some(state) = state.upgrade() else {
                    return;
                };
                editor_resize(&state);
            }
        });
        for (event, kind) in [("pointerdown", POINTER_DOWN), ("pointermove", POINTER_MOVE), ("pointerup", 2)] {
            canvas.ref_own(|e| EventListener::new(&e.raw(), event, {
                let state: Weak<State> = Rc::downgrade(&state);
                move |e| {
                    let Some(state) = state.upgrade() else {
                        return;
                    };
                    let e: &PointerEvent = e.dyn_ref().unwrap();
                    if kind == POINTER_DOWN {
                        _ = state.canvas.raw().set_pointer_capture(e.pointer_id());
                    }
                    let mut samples = vec![];
                    let coalesced =
                        js_sys::Reflect::has(e, &JsValue::from_str("getCoalescedEvents")).unwrap_or(false);
                    if kind == POINTER_MOVE && coalesced {
                        for sample in e.get_coalesced_events().iter() {
                            if let Ok(sample) = sample.dyn_into::<PointerEvent>() {
                                samples.push(sample);
                            }
                        }
                    }
                    if samples.is_empty() {
                        samples.push(e.clone());
                    }
                    for sample in samples {
                        editor_call(
                            &state,
                            "on_pointer",
                            &[
                                JsValue::from_f64(kind as f64),
                                JsValue::from_f64(sample.offset_x() as f64),
                                JsValue::from_f64(sample.offset_y() as f64),
                                JsValue::from_f64(sample.pressure() as f64),
                                JsValue::from_f64(sample.tilt_x() as f64),
                                JsValue::from_f64(sample.tilt_y() as f64),
                                JsValue::from_f64(sample.time_stamp()),
                            ],
                        );
                    }
                }
            }));
        }
        return EditorPanel(state);
    }
}

fn editor_status(state: &State, text: &str) {
    state.status.ref_text(text);
    return;
}

fn editor_call(state: &State, name: &str, args: &[JsValue]) -> Option<JsValue> {
    let function = {
        let instance = state.instance.borrow();
        let exports = &instance.as_ref()?.exports;
        js_sys::Reflect::get(exports, &JsValue::from_str(name)).ok()?.dyn_into::<js_sys::Function>().ok()?
    };
    let array = js_sys::Array::new();
    for arg in args {
        array.push(arg);
    }
    match function.apply(&JsValue::NULL, &array) {
        Ok(value) => return Some(value),
        Err(e) => {
            editor_status(state, &format!("{} failed: {:?}", name, e));
            *state.instance.borrow_mut() = None;
            return None;
        },
    }
}

fn editor_memory_read(state: &State, ptr: i32, len: i32) -> Vec<u8> {
    let instance = state.instance.borrow();
    let Some(instance) = instance.as_ref() else {
        return vec![];
    };
    return js_sys::Uint8Array::new_with_byte_offset_and_length(
        &instance.memory.buffer(),
        ptr as u32,
        len as u32,
    ).to_vec();
}

fn editor_string(state: &State, ptr: i32, len: i32) -> String {
    return String::from_utf8_lossy(&editor_memory_read(state, ptr, len)).into_owned();
}

fn editor_stroke(context: &CanvasRenderingContext2d, width: f64, rgba: i32) {
    let rgba = rgba as u32;
    context.set_line_width(width);
    context.set_line_cap("round");
    context.set_line_join("round");
    context.set_stroke_style_str(
        &format!(
            "rgba({}, {}, {}, {})",
            rgba >> 24,
            (rgba >> 16) & 0xff,
            (rgba >> 8) & 0xff,
            (rgba & 0xff) as f64 / 255.
        ),
    );
    context.stroke();
    return;
}

fn editor_deliver(state: &State, bytes: &[u8]) -> Option<i32> {
    let ptr =
        editor_call(state, "alloc", &[JsValue::from_f64(bytes.len() as f64)])
            .and_then(|p| p.as_f64())
            .map(|p| p as i32)?;
    let instance = state.instance.borrow();
    let instance = instance.as_ref()?;
    js_sys::Uint8Array::new_with_byte_offset_and_length(
        &instance.memory.buffer(),
        ptr as u32,
        bytes.len() as u32,
    ).copy_from(bytes);
    return Some(ptr);
}

fn editor_doc_contents(state: &State) -> Option<Vec<u8>> {
    let ptr = editor_call(state, "doc_ptr", &[])?.as_f64()? as i32;
    let len = editor_call(state, "doc_len", &[])?.as_f64()? as i32;
    return Some(editor_memory_read(state, ptr, len));
}

fn editor_doc_sync(state: &Rc<State>, force: bool) {
    let Some((path, revision)) = state.doc.borrow().as_ref().map(|d| (d.path.clone(), d.revision)) else {
        return;
    };
    wasm_bindgen_futures::spawn_local({
        let state: Weak<State> = Rc::downgrade(state);
        async move {
            let response = client_send(ReqDocSync {
                path: path,
                revision: if force {
                    u64::MAX
                } else {
                    revision
                },
            }).await;
            let Some(state) = state.upgrade() else {
                return;
            };
            let response = match response {
                Ok(response) => response,
                Err(e) => {
                    editor_status(&state, &e);
                    return;
                },
            };
            if let Some(doc) = state.doc.borrow_mut().as_mut() {
                doc.revision = response.revision;
            }
            let Some(contents) = response.contents_base64 else {
                return;
            };
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(contents) else {
                editor_status(&state, "Error decoding the document's base64");
                return;
            };
            let Some(ptr) = editor_deliver(&state, &bytes) else {
                return;
            };
            editor_call(&state, "on_doc", &[JsValue::from_f64(ptr as f64), JsValue::from_f64(bytes.len() as f64)]);
        }
    });
    return;
}

fn editor_fs_done(state: &State, req: i32, result: Result<Vec<u8>, String>) {
    let (status, ptr, len) = match result {
        Ok(bytes) => match editor_deliver(state, &bytes) {
            Some(ptr) => (0, ptr, bytes.len() as i32),
            None => (2, 0, 0),
        },
        Err(e) => {
            editor_status(state, &e);
            (1, 0, 0)
        },
    };
    editor_call(
        state,
        "on_fs",
        &[
            JsValue::from_f64(req as f64),
            JsValue::from_f64(status as f64),
            JsValue::from_f64(ptr as f64),
            JsValue::from_f64(len as f64),
        ],
    );
    return;
}

fn editor_resize(state: &State) {
    let canvas: HtmlCanvasElement = state.canvas.raw().dyn_into().unwrap();
    let ratio = gloo_utils::window().device_pixel_ratio();
    let width = canvas.client_width() as f64;
    let height = canvas.client_height() as f64;
    canvas.set_width((width * ratio) as u32);
    canvas.set_height((height * ratio) as u32);
    _ = state.context.reset_transform();
    _ = state.context.scale(ratio, ratio);
    editor_call(state, "on_resize", &[JsValue::from_f64(width), JsValue::from_f64(height)]);
    return;
}

impl Panel for EditorPanel {
    fn panel_attach(&self) -> El {
        return el("div").classes(&["merman_panel", "merman_panel_editor"]).push(self.0.element.clone());
    }

    fn panel_changed(&self, path: &str) -> Option<PanelChange> {
        if self.0.doc.borrow().as_ref().is_none_or(|doc| doc.path != path) {
            return None;
        }
        editor_doc_sync(&self.0, false);
        return Some(PanelChange::Handled);
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_detach(&self) { }

    fn panel_editable(&self, _editable: bool) { }

    fn panel_focusable(&self) -> bool {
        return true;
    }

    fn panel_focused(&self, _focused: bool) { }

    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult {
        let state = &self.0;
        let Some(stroke) =
            panel_key_stroke(e, DirectionConvert::new(SpecDirection::Right, SpecDirection::Down)) else {
                return PanelResult::Ignored;
            };
        if state.pending.borrow().is_empty() {
            let code = match stroke.key {
                KeyName::Char(c) => c as u32,
                KeyName::Space => ' ' as u32,
                KeyName::Enter => '\n' as u32,
                _ => 0,
            };
            let mods =
                (stroke.ctrl as i32) | ((stroke.alt as i32) << 1) | ((stroke.shift as i32) << 2) |
                    ((stroke.meta as i32) << 3);
            let handled =
                editor_call(state, "on_key", &[JsValue::from_f64(code as f64), JsValue::from_f64(mods as f64)])
                    .and_then(|v| v.as_f64())
                    .is_some_and(|v| v != 0.);
            if handled {
                return PanelResult::Used;
            }
        }
        let action = match state.keys.keymap_read(&mut state.pending.borrow_mut(), stroke, None, false) {
            KeyResolve::Unbound | KeyResolve::Type => return PanelResult::Ignored,
            KeyResolve::Pending => return PanelResult::Used,
            KeyResolve::Action(action) => action,
        };
        let redo = match action {
            Action::Undo => false,
            Action::Redo => true,
            action => return PanelResult::Unused(action),
        };
        let Some(request) = state.doc.borrow().as_ref().map(|doc| ReqDocStep {
            editor: doc.editor.clone(),
            path: doc.path.clone(),
            redo: redo,
            revision: doc.revision,
        }) else {
            return PanelResult::Unused(action);
        };
        wasm_bindgen_futures::spawn_local({
            let state: Weak<State> = Rc::downgrade(state);
            async move {
                let response = client_send(request).await;
                let Some(state) = state.upgrade() else {
                    return;
                };
                let response = match response {
                    Ok(response) => response,
                    Err(e) => {
                        editor_status(&state, &e);
                        return;
                    },
                };
                let behind = state.doc.borrow().as_ref().is_some_and(|doc| doc.revision != response.revision);
                if let Some(doc) = state.doc.borrow_mut().as_mut() {
                    doc.revision = response.revision;
                }
                let Some(level) = response.level_base64 else {
                    if behind {
                        editor_doc_sync(&state, true);
                    }
                    return;
                };
                let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(level) else {
                    editor_status(&state, "Error decoding the level's base64");
                    return;
                };
                let Some(ptr) = editor_deliver(&state, &bytes) else {
                    return;
                };
                editor_call(
                    &state,
                    "on_level",
                    &[
                        JsValue::from_f64(ptr as f64),
                        JsValue::from_f64(bytes.len() as f64),
                        JsValue::from_f64(redo as i32 as f64),
                    ],
                );
                let Some(contents) = editor_doc_contents(&state) else {
                    editor_status(&state, "The extension has no doc_ptr and doc_len to report the document from");
                    return;
                };
                let Some(request) = state.doc.borrow().as_ref().map(|doc| ReqDocWrite {
                    contents_base64: base64::engine::general_purpose::STANDARD.encode(contents),
                    path: doc.path.clone(),
                    revision: doc.revision,
                }) else {
                    return;
                };
                let weak: Weak<State> = Rc::downgrade(&state);
                drop(state);
                let response = client_send(request).await;
                let Some(state) = weak.upgrade() else {
                    return;
                };
                match response {
                    Ok(response) if response.accepted => {
                        if let Some(doc) = state.doc.borrow_mut().as_mut() {
                            doc.revision = response.revision;
                        }
                    },
                    Ok(_) => editor_doc_sync(&state, true),
                    Err(e) => editor_status(&state, &e),
                }
            }
        });
        return PanelResult::Used;
    }

    fn panel_lang_errors(&self, path: &str, _errors: &[CompileError]) {
        let state = &self.0;
        if path != state.path {
            return;
        }
        let Some(export) = state.lang.lang_editor(path) else {
            return;
        };
        let ended_ms = export.ended_ms;
        if state.loaded.get() == Some(ended_ms) {
            return;
        }
        state.loaded.set(Some(ended_ms));
        *state.instance.borrow_mut() = None;
        *state.doc.borrow_mut() = None;
        state.buttons.ref_clear();
        editor_status(state, "Loading");
        let request = ReqLangExportRead {
            import_state: state.import_state,
            path: vec!["files".to_string(), "main.wasm".to_string()],
            server: state.server.clone(),
            source: state.source.clone(),
        };
        wasm_bindgen_futures::spawn_local({
            let state: Weak<State> = Rc::downgrade(state);
            async move {
                let response = client_send(request).await;
                let Some(state) = state.upgrade() else {
                    return;
                };
                if state.loaded.get() != Some(ended_ms) {
                    return;
                }
                let started: Result<(js_sys::Object, js_sys::WebAssembly::Memory), String> = (async {
                    let bytes =
                        base64::engine::general_purpose::STANDARD
                            .decode(response?.bytes_base64)
                            .map_err(|e| format!("Error decoding the wasm's base64: {}", e))?;
                    let imports = {
                        let host = js_sys::Object::new();
                        let mut closures: Vec<Box<dyn Any>> = vec![];
                        let define = |name: &str, function: &JsValue| {
                            js_sys::Reflect::set(&host, &JsValue::from_str(name), function).unwrap();
                        };
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure = Closure::wrap(Box::new(move || {
                                let Some(state) = state.upgrade() else {
                                    return;
                                };
                                let canvas: HtmlCanvasElement = state.canvas.raw().dyn_into().unwrap();
                                state.context.save();
                                _ = state.context.reset_transform();
                                state.context.clear_rect(0., 0., canvas.width() as f64, canvas.height() as f64);
                                state.context.restore();
                            }) as Box<dyn FnMut()>);
                            define("canvas_clear", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure =
                                Closure::wrap(
                                    Box::new(move |x0: f64, y0: f64, x1: f64, y1: f64, width: f64, rgba: i32| {
                                        let Some(state) = state.upgrade() else {
                                            return;
                                        };
                                        state.context.begin_path();
                                        state.context.move_to(x0, y0);
                                        state.context.line_to(x1, y1);
                                        editor_stroke(&state.context, width, rgba);
                                    }) as Box<dyn FnMut(f64, f64, f64, f64, f64, i32)>,
                                );
                            define("canvas_segment", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure = Closure::wrap(Box::new(move |ptr: i32, count: i32, width: f64, rgba: i32| {
                                let Some(state) = state.upgrade() else {
                                    return;
                                };
                                let points = {
                                    let instance = state.instance.borrow();
                                    let Some(instance) = instance.as_ref() else {
                                        return;
                                    };
                                    js_sys::Float64Array::new_with_byte_offset_and_length(
                                        &instance.memory.buffer(),
                                        ptr as u32,
                                        (count as u32) * 2,
                                    ).to_vec()
                                };
                                state.context.begin_path();
                                for (i, point) in points.chunks_exact(2).enumerate() {
                                    if i == 0 {
                                        state.context.move_to(point[0], point[1]);
                                    } else {
                                        state.context.line_to(point[0], point[1]);
                                    }
                                }
                                editor_stroke(&state.context, width, rgba);
                            }) as Box<dyn FnMut(i32, i32, f64, i32)>);
                            define("canvas_polyline", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure = Closure::wrap(Box::new(move |ptr: i32, len: i32| -> i32 {
                                let Some(state) = state.upgrade() else {
                                    return -1;
                                };
                                let label = editor_string(&state, ptr, len);
                                let id = state.buttons.raw().child_element_count() as i32;
                                state
                                    .buttons
                                    .ref_push(
                                        el("div")
                                            .classes(&["merman_toolbar_button", "merman_editor_button"])
                                            .attr("data-action", &id.to_string())
                                            .attr("title", &label)
                                            .text(&label),
                                    );
                                return id;
                            }) as Box<dyn FnMut(i32, i32) -> i32>);
                            define("button_add", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure = Closure::wrap(Box::new(move |ptr: i32, len: i32| {
                                let Some(state) = state.upgrade() else {
                                    return;
                                };
                                let text = editor_string(&state, ptr, len);
                                editor_status(&state, &text);
                            }) as Box<dyn FnMut(i32, i32)>);
                            define("text_set", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure = Closure::wrap(Box::new(move |req: i32, ptr: i32, len: i32| {
                                let Some(state) = state.upgrade() else {
                                    return;
                                };
                                let path = editor_string(&state, ptr, len);
                                wasm_bindgen_futures::spawn_local({
                                    let state: Weak<State> = Rc::downgrade(&state);
                                    async move {
                                        let result = match client_send(ReqFileRead { path: path }).await {
                                            Ok(resp) => base64::engine::general_purpose::STANDARD
                                                .decode(resp.bytes_base64)
                                                .map_err(|e| format!("Error decoding the file's base64: {}", e)),
                                            Err(e) => Err(e),
                                        };
                                        let Some(state) = state.upgrade() else {
                                            return;
                                        };
                                        editor_fs_done(&state, req, result);
                                    }
                                });
                            }) as Box<dyn FnMut(i32, i32, i32)>);
                            define("fs_read", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure =
                                Closure::wrap(
                                    Box::new(move |req: i32, path_ptr: i32, path_len: i32, ptr: i32, len: i32| {
                                        let Some(state) = state.upgrade() else {
                                            return;
                                        };
                                        let path = editor_string(&state, path_ptr, path_len);
                                        let bytes = editor_memory_read(&state, ptr, len);
                                        wasm_bindgen_futures::spawn_local({
                                            let state: Weak<State> = Rc::downgrade(&state);
                                            async move {
                                                let result = client_send(ReqFileWrite {
                                                    bytes_base64: base64
                                                    ::engine
                                                    ::general_purpose
                                                    ::STANDARD.encode(bytes),
                                                    path: path,
                                                }).await;
                                                let Some(state) = state.upgrade() else {
                                                    return;
                                                };
                                                editor_fs_done(&state, req, result.map(|_| vec![]));
                                            }
                                        });
                                    }) as Box<dyn FnMut(i32, i32, i32, i32, i32)>,
                                );
                            define("fs_write", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure =
                                Closure::wrap(
                                    Box::new(move |name_ptr: i32, name_len: i32, key_ptr: i32, key_len: i32| {
                                        let Some(state) = state.upgrade() else {
                                            return;
                                        };
                                        let name = editor_string(&state, name_ptr, name_len);
                                        let key = editor_string(&state, key_ptr, key_len);
                                        let parent = state.path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                                        let request = ReqDocOpen {
                                            editor: format!("{}#{}", state.path, key),
                                            path: format!("{}/{}", parent, name),
                                        };
                                        wasm_bindgen_futures::spawn_local({
                                            let state: Weak<State> = Rc::downgrade(&state);
                                            async move {
                                                let (editor, path) = (request.editor.clone(), request.path.clone());
                                                let response = client_send(request).await;
                                                let Some(state) = state.upgrade() else {
                                                    return;
                                                };
                                                let response = match response {
                                                    Ok(response) => response,
                                                    Err(e) => {
                                                        editor_status(&state, &e);
                                                        return;
                                                    },
                                                };
                                                *state.doc.borrow_mut() = Some(Doc {
                                                    editor: editor,
                                                    path: path,
                                                    revision: response.revision,
                                                });
                                                let Ok(bytes) =
                                                    base64
                                                    ::engine
                                                    ::general_purpose
                                                    ::STANDARD.decode(response.contents_base64) else {
                                                        editor_status(
                                                            &state,
                                                            "Error decoding the document's base64",
                                                        );
                                                        return;
                                                    };
                                                let Some(ptr) = editor_deliver(&state, &bytes) else {
                                                    return;
                                                };
                                                editor_call(
                                                    &state,
                                                    "on_doc",
                                                    &[
                                                        JsValue::from_f64(ptr as f64),
                                                        JsValue::from_f64(bytes.len() as f64),
                                                    ],
                                                );
                                            }
                                        });
                                    }) as Box<dyn FnMut(i32, i32, i32, i32)>,
                                );
                            define("doc_open", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure = Closure::wrap(Box::new(move |ptr: i32, len: i32| {
                                let Some(state) = state.upgrade() else {
                                    return;
                                };
                                let level = editor_memory_read(&state, ptr, len);
                                let Some(contents) = editor_doc_contents(&state) else {
                                    editor_status(
                                        &state,
                                        "The extension edited a document without doc_ptr and doc_len",
                                    );
                                    return;
                                };
                                let Some(request) = state.doc.borrow().as_ref().map(|doc| ReqDocEdit {
                                    contents_base64: base64::engine::general_purpose::STANDARD.encode(contents),
                                    editor: doc.editor.clone(),
                                    level_base64: base64::engine::general_purpose::STANDARD.encode(level),
                                    path: doc.path.clone(),
                                    revision: doc.revision,
                                }) else {
                                    editor_status(&state, "The extension edited a document before opening one");
                                    return;
                                };
                                wasm_bindgen_futures::spawn_local({
                                    let state: Weak<State> = Rc::downgrade(&state);
                                    async move {
                                        let response = client_send(request).await;
                                        let Some(state) = state.upgrade() else {
                                            return;
                                        };
                                        match response {
                                            Ok(response) if response.accepted => {
                                                if let Some(doc) = state.doc.borrow_mut().as_mut() {
                                                    doc.revision = response.revision;
                                                }
                                            },
                                            Ok(_) => editor_doc_sync(&state, true),
                                            Err(e) => editor_status(&state, &e),
                                        }
                                    }
                                });
                            }) as Box<dyn FnMut(i32, i32)>);
                            define("doc_edit", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let started = state.started;
                            let closure = Closure::wrap(Box::new(move || -> f64 {
                                return js_sys::Date::now() - started;
                            }) as Box<dyn FnMut() -> f64>);
                            define("now_ms", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        {
                            let state: Weak<State> = Rc::downgrade(&state);
                            let closure = Closure::wrap(Box::new(move |ptr: i32, len: i32| {
                                let Some(state) = state.upgrade() else {
                                    return;
                                };
                                web_sys::console::log_1(&JsValue::from_str(&editor_string(&state, ptr, len)));
                            }) as Box<dyn FnMut(i32, i32)>);
                            define("log", closure.as_ref());
                            closures.push(Box::new(closure));
                        }
                        *state.closures.borrow_mut() = closures;
                        let imports = js_sys::Object::new();
                        js_sys::Reflect::set(&imports, &JsValue::from_str("merman"), &host).unwrap();
                        imports
                    };
                    let result =
                        JsFuture::from(js_sys::WebAssembly::instantiate_buffer(&bytes, &imports))
                            .await
                            .map_err(|e| format!("Error instantiating the wasm: {:?}", e))?;
                    let instance: js_sys::WebAssembly::Instance =
                        js_sys::Reflect::get(&result, &JsValue::from_str("instance"))
                            .map_err(|e| format!("Error reading the instance: {:?}", e))?
                            .dyn_into()
                            .map_err(|e| format!("Error reading the instance: {:?}", e))?;
                    let exports = instance.exports();
                    let memory: js_sys::WebAssembly::Memory =
                        js_sys::Reflect::get(&exports, &JsValue::from_str("memory"))
                            .ok()
                            .and_then(|m| m.dyn_into().ok())
                            .ok_or_else(|| "The wasm doesn't export its memory".to_string())?;
                    return Ok((exports, memory));
                }).await;
                let (exports, memory) = match started {
                    Ok(started) => started,
                    Err(e) => {
                        editor_status(&state, &e);
                        return;
                    },
                };
                if state.loaded.get() != Some(ended_ms) {
                    return;
                }
                *state.instance.borrow_mut() = Some(Instance {
                    exports: exports,
                    memory: memory,
                });
                editor_status(&state, "");
                editor_resize(&state);
                editor_call(&state, "_start", &[]);
            }
        });
        return;
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        let Some(id) = toolbar_action(e).and_then(|a| a.parse::<i32>().ok()) else {
            return PanelResult::Ignored;
        };
        editor_call(&self.0, "on_button", &[JsValue::from_f64(id as f64)]);
        return PanelResult::Used;
    }

    fn panel_parent(&self) -> Option<String> {
        return None;
    }

    fn panel_path(&self) -> String {
        return self.0.path.clone();
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
