mod display;

use {
    crate::{
        client::client_send,
        panels::{
            Panel,
            PanelChange,
            PanelHost,
            PanelResult,
            choices::ChoicesPanel,
            code::display::{
                DisplayWeb,
                display_el,
            },
            conflict::ConflictPanel,
            error::ErrorPanel,
            panel_key_stroke,
            toolbar::{
                toolbar_action,
                toolbar_new,
            },
        },
    },
    gloo_events::{
        EventListener,
        EventListenerOptions,
    },
    gloo_render::{
        AnimationFrame,
        request_animation_frame,
    },
    gloo_timers::callback::Timeout,
    gloo_utils::window,
    merman_api::{
        ReqEdit,
        ReqFlush,
        ReqRedo,
        ReqSync,
        ReqUndo,
    },
    merman_langserver::CompileError,
    crate::lang::lang_source_key,
    merman_core::{
        back::back_locate,
        context::{
            Context,
            ContextConfig,
            MarkId,
            Vector,
        },
        cursor::Located,
        direction::DirectionConvert,
        document::{
            AtomId,
            Document,
        },
        edit::EditBatch,
        environment::Environment,
        keys::{
            Action,
            KeyResolve,
            Keymap,
        },
        matcher::{
            match_document,
            source_parse,
        },
        patch::{
            Patch,
            PatchApplied,
            SetTarget,
        },
        reference::Reference,
        serialize::serialize_atom,
        syntax::Syntax,
    },
    rooting::{
        El,
        el,
    },
    std::{
        cell::RefCell,
        collections::{
            HashMap,
            VecDeque,
        },
        rc::{
            Rc,
            Weak,
        },
    },
    wasm_bindgen::JsCast,
    wasm_bindgen_futures::JsFuture,
    web_sys::{
        Element,
        KeyboardEvent,
        MouseEvent,
        WheelEvent,
    },
};

fn after_context(state: &Rc<RefCell<State>>) {
    {
        let mut s = state.borrow_mut();
        let State { context, details_atom, details_errors, errors_by_atom, .. } = &mut *s;
        if let Some(ctx) = context.as_mut() {
            let atom = ctx.cursor_atom();
            if atom != *details_atom {
                *details_atom = atom;
                let errors = atom.and_then(|a| errors_by_atom.get(&a)).cloned().unwrap_or_default();
                let changed = *details_errors.borrow() != errors;
                *details_errors.borrow_mut() = errors;
                if changed && ctx.details.is_some() {
                    ctx.details_close();
                    ctx.details_open();
                }
            }
        }
        'sync: {
            let convert = s.convert();
            let Some(context) = s.context.as_mut() else {
                break 'sync;
            };
            let edge = context.edge;
            let converse_edge = context.display.group_converse_span(context.text_layer);
            let (t_min, t_max) = context.wall_usage;
            let (x_span, y_span) = convert.direction_unconvert_span(converse_edge.max(edge), t_max - t_min);
            let new_origin = convert.direction_unconvert(0., 0., x_span.round(), y_span.round());
            if s.origin != new_origin {
                s.origin = new_origin;
                s.shift.ref_attr("style", &format!("left: {}px; top: {}px", -new_origin.0, -new_origin.1));
            }
            break 'sync;
        }
    };
    let request = {
        let mut s = state.borrow_mut();
        match s.context.as_mut() {
            Some(ctx) => ctx.take_timer_request(),
            None => false,
        }
    };
    if request {
        let state2 = state.clone();
        let timeout = Timeout::new(50, move || {
            {
                let mut s = state2.borrow_mut();
                s.timer = None;
                if let Some(ctx) = s.context.as_mut() {
                    ctx.handle_timer();
                }
            }
            after_context(&state2);
        });
        state.borrow_mut().timer = Some(timeout);
    }
}

fn code_after_edit(state: &Rc<RefCell<State>>) -> Option<PanelResult> {
    let out = {
        let mut s = state.borrow_mut();
        let State { context, edit, choices, .. } = &mut *s;
        let ctx = context.as_mut()?;
        let batches = ctx.edit_take();
        if let Some(edit) = edit.as_mut() {
            edit.queue.extend(batches.into_iter().map(Request::Edit));
        }
        ctx.gap_choices_sync();
        match (ctx.gap_choices.as_ref().filter(|c| !c.choices.is_empty()), choices.as_mut()) {
            (Some(gap_choices), Some((panel, shown))) => {
                panel.choices_set(gap_choices);
                if *shown {
                    None
                } else {
                    *shown = true;
                    Some(PanelResult::Detail(panel.clone()))
                }
            },
            (None, Some((_, shown))) if *shown => {
                *shown = false;
                Some(PanelResult::Selected)
            },
            _ => None,
        }
    };
    code_pump(state);
    return out;
}

fn code_in_details(e: &MouseEvent) -> bool {
    return e
        .target()
        .and_then(|t| t.dyn_into::<Element>().ok())
        .and_then(|t| t.closest(".merman_display_details").ok().flatten())
        .is_some();
}

fn code_host(state: &Rc<RefCell<State>>, result: PanelResult) {
    let (host, this) = {
        let s = state.borrow();
        let Some(edit) = s.edit.as_ref() else {
            return;
        };
        (edit.host.clone(), s.this.clone())
    };
    wasm_bindgen_futures::spawn_local(async move {
        let Some(this) = this.upgrade() else {
            return;
        };
        (host.0)(this, result);
    });
}

fn code_pump(state: &Rc<RefCell<State>>) {
    let (request, revision, path) = {
        let mut s = state.borrow_mut();
        let path = s.path.clone();
        let Some(edit) = s.edit.as_mut() else {
            return;
        };
        if edit.running {
            return;
        }
        let Some(request) = edit.queue.pop_front() else {
            return;
        };
        edit.running = true;
        (request, edit.revision, path)
    };
    let weak = Rc::downgrade(state);
    wasm_bindgen_futures::spawn_local(async move {
        match request {
            Request::Edit(batch) => {
                let result = client_send(ReqEdit {
                    path: path,
                    revision: revision,
                    new_level: batch.new_level,
                    patches: serde_json::to_string(&batch.patches).unwrap(),
                    select_before: batch.select_before,
                    select_after: batch.select_after,
                }).await;
                let Some(state) = weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(r) if r.accepted => state.borrow_mut().edit.as_mut().unwrap().revision = r.revision,
                    _ => {
                        let mut s = state.borrow_mut();
                        let edit = s.edit.as_mut().unwrap();
                        edit.rejected = true;
                        edit.queue.retain(|r| !matches!(r, Request::Edit(_)));
                        edit.queue.push_front(Request::Sync(false));
                    },
                }
                code_pumped(&state);
            },
            Request::Step(redo) => {
                let result = if redo {
                    client_send(ReqRedo {
                        path: path,
                        revision: revision,
                    }).await.map(|r| (r.revision, r.step))
                } else {
                    client_send(ReqUndo {
                        path: path,
                        revision: revision,
                    }).await.map(|r| (r.revision, r.step))
                };
                let Some(state) = weak.upgrade() else {
                    return;
                };
                state.borrow_mut().edit.as_mut().unwrap().busy = false;
                match result {
                    Ok((revision, Some(step))) => {
                        state.borrow_mut().edit.as_mut().unwrap().revision = revision;
                        'apply: {
                            let patches: Vec<Patch> = match serde_json::from_str(&step.patches) {
                                Ok(p) => p,
                                Err(_) => {
                                    code_queue_front(&state, Request::Sync(true));
                                    break 'apply;
                                },
                            };
                            for patch in &patches {
                                let applied = {
                                    let mut s = state.borrow_mut();
                                    let Some(ctx) = s.context.as_mut() else {
                                        break 'apply;
                                    };
                                    ctx.patch_apply(patch)
                                };
                                match applied {
                                    Ok(PatchApplied::Done) => { },
                                    Ok(PatchApplied::Reload(value)) => {
                                        if code_reload_value(&state, &value).is_err() {
                                            code_queue_front(&state, Request::Sync(true));
                                            break 'apply;
                                        }
                                    },
                                    Err(_) => {
                                        code_queue_front(&state, Request::Sync(true));
                                        break 'apply;
                                    },
                                }
                            }
                            let mut s = state.borrow_mut();
                            let Some(ctx) = s.context.as_mut() else {
                                break 'apply;
                            };
                            if let Some(reference) = step.select.and_then(|t| Reference::reference_parse(&t).ok()) {
                                ctx.cursor_select_reference(&reference);
                            }
                            ctx.edit_break();
                            ctx.input_flush();
                            drop(s);
                            if let Some(result) = code_after_edit(&state) {
                                code_host(&state, result);
                            }
                        }
                    },
                    Ok((_, None)) => code_queue_front(&state, Request::Sync(false)),
                    Err(_) => code_queue_front(&state, Request::Sync(true)),
                }
                code_pumped(&state);
            },
            Request::Sync(force) => {
                let result = client_send(ReqSync {
                    path: path,
                    revision: if force {
                        u64::MAX
                    } else {
                        revision
                    },
                }).await;
                let Some(state) = weak.upgrade() else {
                    return;
                };
                if let Ok(r) = result {
                    if let Some(source) = r.source {
                        let (revision, unwritten) = (r.revision, r.unwritten && !force);
                        'synced: {
                            let rejected = state.borrow().edit.as_ref().unwrap().rejected;
                            if !unwritten && !rejected {
                                state.borrow_mut().edit.as_mut().unwrap().revision = revision;
                                code_reload_source(&state, &source);
                                break 'synced;
                            }
                            {
                                let mut s = state.borrow_mut();
                                s.edit.as_mut().unwrap().conflict = Some((source, revision));
                                if let Some(ctx) = s.context.as_mut() {
                                    ctx.config.editable = false;
                                }
                            }
                            let keys = state.borrow().keys.clone();
                            let weak = Rc::downgrade(&state);
                            code_host(
                                &state,
                                PanelResult::Open(Rc::new(ConflictPanel::conflict_new(keys, Box::new(move |keep| {
                                    let Some(state) = weak.upgrade() else {
                                        return;
                                    };
                                    let Some((source, revision)) =
                                        state.borrow_mut().edit.as_mut().and_then(|e| e.conflict.take()) else {
                                            return;
                                        };
                                    {
                                        let mut s = state.borrow_mut();
                                        let edit = s.edit.as_mut().unwrap();
                                        edit.revision = revision;
                                        edit.rejected = false;
                                        if let Some(ctx) = s.context.as_mut() {
                                            ctx.config.editable = true;
                                        }
                                    }
                                    if keep {
                                        let batch = {
                                            let s = state.borrow();
                                            let ctx = s.context.as_ref().unwrap();
                                            let select = ctx.cursor_reference().map(|r| r.reference_format());
                                            EditBatch {
                                                new_level: true,
                                                patches: vec![Patch::Set {
                                                    path: vec![],
                                                    target: SetTarget::Document,
                                                    value: serialize_atom(
                                                        &ctx.syntax,
                                                        &ctx.document,
                                                        ctx.document.root,
                                                    ),
                                                }],
                                                select_after: select.clone(),
                                                select_before: select,
                                            }
                                        };
                                        code_queue(&state, Request::Edit(batch));
                                    } else {
                                        code_reload_source(&state, &source);
                                    }
                                    code_host(&state, PanelResult::Selected);
                                })))),
                            );
                        }
                    }
                }
                code_pumped(&state);
            },
            Request::Flush => {
                _ = client_send(ReqFlush { path: path }).await;
                let Some(state) = weak.upgrade() else {
                    return;
                };
                code_pumped(&state);
            },
        }
    });
}

fn code_pumped(state: &Rc<RefCell<State>>) {
    state.borrow_mut().edit.as_mut().unwrap().running = false;
    after_context(state);
    code_pump(state);
}

fn code_queue(state: &Rc<RefCell<State>>, request: Request) {
    if let Some(edit) = state.borrow_mut().edit.as_mut() {
        edit.queue.push_back(request);
    }
    code_pump(state);
}

fn code_queue_front(state: &Rc<RefCell<State>>, request: Request) {
    if let Some(edit) = state.borrow_mut().edit.as_mut() {
        edit.queue.push_front(request);
    }
}

fn code_reload_value(state: &Rc<RefCell<State>>, value: &serde_json::Value) -> Result<(), String> {
    let syntax = state.borrow().syntax.clone();
    let document =
        match_document(
            &syntax,
            value,
        ).map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?;
    {
        let mut s = state.borrow_mut();
        let select = s.context.as_ref().and_then(|c| c.cursor_reference()).map(|r| r.reference_format());
        if let Some(select) = select {
            s.select = Some(select);
        }
        s.context = None;
        s.timer = None;
        s.shift.ref_clear();
        s.origin = (0., 0.);
        s.document = Some(document);
        s.lay_out();
    }
    after_context(state);
    return Ok(());
}

fn code_reload_source(state: &Rc<RefCell<State>>, source: &str) {
    let result =
        source_parse(source)
            .map_err(|e| format!("Error parsing source JSON: {}", e))
            .and_then(|value| code_reload_value(state, &value));
    if let Err(e) = result {
        let path = state.borrow().path.clone();
        code_host(state, PanelResult::Replace(Rc::new(ErrorPanel::error_new(path, false, &e))));
    }
}

pub struct CodePanel(Rc<RefCell<State>>);

pub struct CodeEdit {
    pub host: PanelHost,
    pub revision: u64,
}

impl CodePanel {
    pub fn code_new(
        keys: Keymap,
        path: String,
        syntax: Rc<Syntax>,
        document: Document,
        select: Option<String>,
        edit: Option<CodeEdit>,
    ) -> Rc<CodePanel> {
        return Rc::new_cyclic(|this: &Weak<CodePanel>| {
            let shift = display_el("merman_origin");
            let measure = el("canvas").classes(&["merman_measure"]);
            let host = el("div").classes(&["merman"]).push(shift.clone()).push(measure.clone());
            let panel = el("div").classes(&["merman_panel", "merman_panel_code"]).push(host.clone());
            let this: Weak<dyn Panel> = this.clone();
            let state = Rc::new(RefCell::new(State {
                choices: edit
                    .as_ref()
                    .map(|_| (Rc::new(ChoicesPanel::choices_new(keys.clone(), syntax.clone())), false)),
                details_atom: None,
                details_errors: Rc::new(RefCell::new(vec![])),
                errors: vec![],
                errors_by_atom: HashMap::new(),
                marks: vec![],
                edit: edit.map(|e| EditState {
                    busy: false,
                    conflict: None,
                    host: e.host,
                    queue: VecDeque::new(),
                    rejected: false,
                    revision: e.revision,
                    running: false,
                }),
                path: path,
                syntax: syntax,
                document: Some(document),
                keys: keys,
                context: None,
                focused: false,
                select: select,
                panel: panel.clone(),
                host: host.clone(),
                measure: measure,
                shift: shift,
                origin: (0., 0.),
                host_size: None,
                panel_size: None,
                hover_point: None,
                hover_raf: None,
                timer: None,
                this: this,
            }));
            let (host, panel) = {
                let s = state.borrow();
                (s.host.clone(), s.panel.clone())
            };
            let weak = Rc::downgrade(&state);
            host.ref_on_resize({
                let weak = weak.clone();
                move |_, inline_size, block_size| {
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    state.borrow_mut().host_size = Some((inline_size, block_size));
                    state.borrow_mut().lay_out();
                    after_context(&state);
                }
            });
            panel.ref_on_resize({
                let weak = weak.clone();
                move |_, inline_size, block_size| {
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    state.borrow_mut().panel_size = Some((inline_size, block_size));
                    state.borrow_mut().lay_out();
                    after_context(&state);
                }
            });
            panel.ref_own(
                |e| EventListener::new_with_options(
                    &e.raw(),
                    "wheel",
                    EventListenerOptions::enable_prevent_default(),
                    {
                        let weak = weak.clone();
                        move |e| {
                            let Some(state) = weak.upgrade() else {
                                return;
                            };
                            let e: &WheelEvent = e.dyn_ref().unwrap();
                            e.prevent_default();
                            let convert = state.borrow().convert();
                            let (_, transverse) = convert.direction_convert_point(e.delta_x(), e.delta_y());
                            with_context(&state, |ctx| ctx.context_scroll_by(transverse));
                        }
                    },
                ),
            );
            panel.ref_on("mousemove", {
                let weak = weak.clone();
                move |e| {
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    let e: &MouseEvent = e.dyn_ref().unwrap();
                    if code_in_details(e) {
                        return;
                    }
                    let point = {
                        let s = state.borrow();
                        let local = (e.offset_x() as f64 + s.origin.0, e.offset_y() as f64 + s.origin.1);
                        let (c, t) = s.convert().direction_convert_point(local.0, local.1);
                        Vector::new(c, t)
                    };
                    {
                        let mut s = state.borrow_mut();
                        s.hover_point = Some(point);
                        if s.hover_raf.is_some() {
                            return;
                        }
                    }
                    let frame = request_animation_frame({
                        let weak = weak.clone();
                        move |_| {
                            let Some(state) = weak.upgrade() else {
                                return;
                            };
                            let point = {
                                let mut s = state.borrow_mut();
                                s.hover_raf = None;
                                s.hover_point.take()
                            };
                            let Some(point) = point else {
                                return;
                            };
                            with_context(&state, |ctx| ctx.mouse_moved(point));
                        }
                    });
                    state.borrow_mut().hover_raf = Some(frame);
                }
            });
            panel.ref_on("mouseleave", {
                let weak = weak.clone();
                move |_| {
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    {
                        let mut s = state.borrow_mut();
                        s.hover_point = None;
                        s.hover_raf = None;
                    }
                    with_context(&state, |ctx| ctx.mouse_exited())
                }
            });
            panel.ref_own(
                |e| EventListener::new_with_options(
                    &e.raw(),
                    "mousedown",
                    EventListenerOptions::enable_prevent_default(),
                    {
                        let weak = weak.clone();
                        move |e| {
                            let Some(state) = weak.upgrade() else {
                                return;
                            };
                            let e: &MouseEvent = e.dyn_ref().unwrap();
                            if e.button() != 0 || code_in_details(e) {
                                return;
                            }
                            e.prevent_default();
                            with_context(&state, |ctx| {
                                ctx.mouse_button(true);
                            });
                            if let Some(result) = code_after_edit(&state) {
                                code_host(&state, result);
                            }
                        }
                    },
                ),
            );
            panel.ref_on("mouseup", {
                let weak = weak.clone();
                move |e| {
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    let e: &MouseEvent = e.dyn_ref().unwrap();
                    if e.button() != 0 {
                        return;
                    }
                    with_context(&state, |ctx| {
                        ctx.mouse_button(false);
                    });
                }
            });
            return CodePanel(state);
        });
    }
}

impl Panel for CodePanel {
    fn panel_attach(&self) -> El {
        return self.0.borrow().panel.clone();
    }

    fn panel_changed(&self, path: &str) -> Option<PanelChange> {
        if path != self.0.borrow().path {
            return None;
        }
        if self.0.borrow().edit.is_none() {
            return Some(PanelChange::Reload(false));
        }
        code_queue(&self.0, Request::Sync(false));
        return Some(PanelChange::Handled);
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        let s = self.0.borrow();
        match s.context.as_ref() {
            Some(ctx) => return ctx.cursor_reference().map(|r| r.reference_format()),
            None => return s.select.clone(),
        }
    }

    fn panel_detach(&self) {
        return;
    }

    fn panel_focusable(&self) -> bool {
        return true;
    }

    fn panel_focused(&self, focused: bool) {
        {
            let mut s = self.0.borrow_mut();
            s.focused = focused;
            let State { context, select, edit, .. } = &mut *s;
            if let Some(ctx) = context.as_mut() {
                if !focused {
                    ctx.clear_cursor();
                    ctx.details_close();
                } else {
                    if ctx.cursor.is_none() {
                        select_initial(ctx, select);
                    }
                    if edit.is_some() {
                        ctx.details_open();
                    }
                }
            }
        }
        if !focused {
            code_queue(&self.0, Request::Flush);
        }
        if let Some(result) = code_after_edit(&self.0) {
            code_host(&self.0, result);
        }
        after_context(&self.0);
        return;
    }

    fn panel_lang_errors(&self, path: &str, errors: &[CompileError]) {
        {
            let mut s = self.0.borrow_mut();
            if path != s.path {
                return;
            }
            s.errors = errors.to_vec();
            s.marks_apply();
        }
        after_context(&self.0);
        return;
    }

    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult {
        let state = &self.0;
        let convert = state.borrow().convert();
        let Some(stroke) = panel_key_stroke(e, convert) else {
            return PanelResult::Ignored;
        };
        if state.borrow().edit.as_ref().is_some_and(|e| e.busy) {
            return PanelResult::Used;
        }
        let resolved = {
            let mut s = state.borrow_mut();
            match s.context.as_mut() {
                Some(ctx) => ctx.key_resolve(stroke),
                None => return PanelResult::Ignored,
            }
        };
        let editable = state.borrow().context.as_ref().is_some_and(|c| c.config.editable);
        let result = match resolved {
            KeyResolve::Unbound => return PanelResult::Ignored,
            KeyResolve::Pending => return PanelResult::Used,
            KeyResolve::Type => {
                let text = e.key();
                with_context(state, |ctx| {
                    ctx.edit_type(&text);
                    ctx.input_flush();
                });
                PanelResult::Used
            },
            KeyResolve::Action(action @ (Action::Undo | Action::Redo)) if editable => {
                if let Some(result) = code_after_edit(state) {
                    code_host(state, result);
                }
                state.borrow_mut().edit.as_mut().unwrap().busy = true;
                code_queue(state, Request::Step(action == Action::Redo));
                return PanelResult::Used;
            },
            KeyResolve::Action(Action::Paste) if editable => {
                let weak = Rc::downgrade(state);
                wasm_bindgen_futures::spawn_local(async move {
                    let Ok(text) = JsFuture::from(window().navigator().clipboard().read_text()).await else {
                        return;
                    };
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    let text = text.as_string().unwrap_or_default();
                    with_context(&state, |ctx| {
                        ctx.edit_paste(&text);
                        ctx.input_flush();
                    });
                    if let Some(result) = code_after_edit(&state) {
                        code_host(&state, result);
                    }
                });
                return PanelResult::Used;
            },
            KeyResolve::Action(action) => {
                let handled = {
                    let mut s = state.borrow_mut();
                    let ctx = s.context.as_mut().unwrap();
                    let handled = ctx.key_action(action);
                    if handled {
                        ctx.input_flush();
                    }
                    handled
                };
                after_context(state);
                if handled {
                    PanelResult::Used
                } else {
                    PanelResult::Unused(action)
                }
            },
        };
        let choices = code_after_edit(state);
        if let PanelResult::Unused(_) = result {
            if let Some(choices) = choices {
                code_host(state, choices);
            }
            return result;
        }
        return choices.unwrap_or(result);
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        if toolbar_action(e).as_deref() == Some("ai") {
            return PanelResult::Unused(Action::AiOpenReference);
        }
        return PanelResult::Ignored;
    }

    fn panel_parent(&self) -> Option<String> {
        return None;
    }

    fn panel_path(&self) -> String {
        return self.0.borrow().path.clone();
    }

    fn panel_reference(&self) -> Option<String> {
        return Some(format!("{}{}", self.panel_path(), self.panel_cursor_reference()?));
    }

    fn panel_select(&self, location: &str) {
        with_context(&self.0, |ctx| {
            if let Ok(reference) = Reference::reference_parse(location) {
                ctx.cursor_select_reference(&reference);
            }
        });
        let mut s = self.0.borrow_mut();
        if s.context.is_none() {
            s.select = Some(location.to_string());
        }
        return;
    }

    fn panel_selection(&self) -> Option<(bool, String)> {
        return None;
    }

    fn panel_size(&self) -> f64 {
        return 30.;
    }
}

struct EditState {
    busy: bool,
    conflict: Option<(String, u64)>,
    host: PanelHost,
    queue: VecDeque<Request>,
    rejected: bool,
    revision: u64,
    running: bool,
}
struct EnvironmentWeb;

impl Environment for EnvironmentWeb {
    fn environment_clipboard_set(&mut self, text: &str) {
        let _ = window().navigator().clipboard().write_text(text);
    }

    fn environment_now_ms(&mut self) -> f64 {
        return js_sys::Date::now();
    }
}

enum Request {
    Edit(EditBatch),
    Flush,
    Step(bool),
    Sync(bool),
}

fn select_initial(ctx: &mut Context, select: &mut Option<String>) {
    if let Some(text) = select.take() {
        if let Ok(reference) = Reference::reference_parse(&text) {
            if ctx.cursor_select_reference(&reference) {
                return;
            }
        }
    }
    let root = ctx.root_visual;
    ctx.visual_select_into_any_child(root);
}

struct State {
    choices: Option<(Rc<ChoicesPanel>, bool)>,
    context: Option<Context>,
    details_atom: Option<AtomId>,
    details_errors: Rc<RefCell<Vec<String>>>,
    document: Option<Document>,
    edit: Option<EditState>,
    errors: Vec<CompileError>,
    errors_by_atom: HashMap<AtomId, Vec<String>>,
    focused: bool,
    host: El,
    host_size: Option<(f64, f64)>,
    hover_point: Option<Vector>,
    hover_raf: Option<AnimationFrame>,
    keys: Keymap,
    marks: Vec<MarkId>,
    measure: El,
    origin: (f64, f64),
    panel: El,
    panel_size: Option<(f64, f64)>,
    path: String,
    select: Option<String>,
    shift: El,
    syntax: Rc<Syntax>,
    this: Weak<dyn Panel>,
    timer: Option<Timeout>,
}

impl State {
    fn convert(&self) -> DirectionConvert {
        return self.syntax.spec_root.convert;
    }

    fn marks_apply(&mut self) {
        let Some(ctx) = self.context.as_mut() else {
            return;
        };
        for mark in self.marks.drain(..) {
            ctx.mark_destroy(mark);
        }
        self.errors_by_atom.clear();
        for error in &self.errors {
            let atom = match error.location.expr {
                Some(expr) => {
                    let Some(target) = back_locate(&ctx.syntax, &ctx.document, &Reference {
                        id: Some(expr),
                        path: vec![],
                        range: None,
                    }) else {
                        continue;
                    };
                    match target.located {
                        Located::Atom(atom) => atom,
                        Located::Field(atom, _) => atom,
                    }
                },
                None => ctx.document.root,
            };
            let mut message = error.message.clone();
            for related in &error.related {
                message.push_str(
                    &format!("\n{}: {}", related.description, lang_source_key(&related.location.source)),
                );
            }
            self.errors_by_atom.entry(atom).or_default().push(message);
            self.marks.push(ctx.mark_new(atom, ctx.stylist.style_mark()));
        }
        self.details_atom = None;
        return;
    }

    fn lay_out(&mut self) {
        let (Some(host), Some(panel)) = (self.host_size, self.panel_size) else {
            return;
        };
        let convert = self.convert();
        let converse = convert.direction_convert_span(host.0, host.1).0;
        let transverse = convert.direction_convert_span(panel.0, panel.1).1;
        if let Some(ctx) = self.context.as_mut() {
            ctx.context_resize(converse, transverse);
            return;
        }
        let Some(document) = self.document.take() else {
            return;
        };
        let (inset_x, inset_y) =
            convert.direction_unconvert(self.syntax.spec_root.pad.converse_start.round(), 0., 0., 0.);
        let details_errors = self.details_errors.clone();
        let display = DisplayWeb {
            convert: convert,
            details: Rc::new(move || {
                let details =
                    el("div")
                        .push(toolbar_new(&[("ai", "\u{e0ca}", "Ask Claude about the cursor's location")]))
                        .attr(
                            "style",
                            &format!(
                                "position: relative; left: max(0px, calc({}px - var(--merman-spacing-details))); top: {}px",
                                inset_x,
                                inset_y
                            ),
                        );
                let errors = details_errors.borrow();
                if !errors.is_empty() {
                    let list = el("div").classes(&["merman_details_errors"]);
                    for error in errors.iter() {
                        list.ref_push(el("div").classes(&["merman_error"]).text(error));
                    }
                    details.ref_push(list);
                }
                return details;
            }),
            root: self.shift.clone(),
            background: self.panel.clone(),
            nodes: vec![],
            measure: self.measure.clone(),
            measure_font: String::new(),
            widths: HashMap::new(),
            metrics: HashMap::new(),
        };
        let editable = self.edit.as_ref().is_some_and(|e| e.conflict.is_none());
        let mut ctx = Context::context_new(self.syntax.clone(), document, ContextConfig {
            keys: self.keys.clone(),
            editable: editable,
            ..ContextConfig::default()
        }, Box::new(display), Box::new(EnvironmentWeb), converse, transverse);
        if self.focused {
            select_initial(&mut ctx, &mut self.select);
            if self.edit.is_some() {
                ctx.details_open();
            }
        }
        self.context = Some(ctx);
        self.marks_apply();
    }
}

fn with_context(state: &Rc<RefCell<State>>, f: impl FnOnce(&mut Context)) {
    {
        let mut s = state.borrow_mut();
        let Some(ctx) = s.context.as_mut() else {
            return;
        };
        f(ctx);
    }
    after_context(state);
}
