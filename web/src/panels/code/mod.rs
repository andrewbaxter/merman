mod display;

use {
    crate::panels::{
        Panel,
        PanelResult,
        code::display::{
            DisplayWeb,
            display_el,
        },
        panel_key_stroke,
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
    merman_core::{
        context::{
            Context,
            ContextConfig,
            Vector,
        },
        direction::DirectionConvert,
        document::Document,
        environment::Environment,
        keys::{
            KeyResolve,
            Keymap,
        },
        reference::Reference,
        syntax::Syntax,
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
        collections::HashMap,
        rc::Rc,
    },
    wasm_bindgen::JsCast,
    web_sys::{
        KeyboardEvent,
        MouseEvent,
        WheelEvent,
    },
};

fn after_context(state: &Rc<RefCell<State>>) {
    {
        let mut s = state.borrow_mut();
        'sync: {
            let convert = s.convert();
            let Some(context) = s.context.as_mut() else {
                break 'sync;
            };
            let edge = context.edge;
            let converse_edge = context.display.group_converse_span(context.text_layer);
            let (t_min, t_max) = context.wall_usage;
            let (x_span, y_span) = convert.direction_unconvert_span(converse_edge.max(edge), t_max - t_min);
            let new_origin = convert.direction_unconvert(0., 0., x_span, y_span);
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

pub struct CodePanel {
    attached: RefCell<Option<Rc<RefCell<State>>>>,
    document: Rc<Document>,
    focused: Cell<bool>,
    keys: Keymap,
    path: String,
    select: RefCell<Option<String>>,
    syntax: Rc<Syntax>,
}

impl CodePanel {
    pub fn code_new(
        keys: Keymap,
        path: String,
        syntax: Rc<Syntax>,
        document: Rc<Document>,
        select: Option<String>,
    ) -> CodePanel {
        return CodePanel {
            keys: keys,
            path: path,
            syntax: syntax,
            document: document,
            focused: Cell::new(false),
            select: RefCell::new(select),
            attached: RefCell::new(None),
        };
    }
}

impl Panel for CodePanel {
    fn panel_attach(&self) -> El {
        let shift = display_el("merman_origin");
        let host = el("div").classes(&["merman"]).push(shift.clone());
        let panel = el("div").classes(&["merman_panel", "merman_panel_code"]).push(host.clone());
        let state = Rc::new(RefCell::new(State {
            syntax: self.syntax.clone(),
            document: self.document.clone(),
            keys: self.keys.clone(),
            context: None,
            focused: self.focused.get(),
            select: self.select.borrow_mut().take(),
            panel: panel.clone(),
            host: host.clone(),
            shift: shift,
            origin: (0., 0.),
            host_size: None,
            panel_size: None,
            hover_point: None,
            hover_raf: None,
            timer: None,
        }));
        host.ref_on_resize({
            let state = state.clone();
            move |_, inline_size, block_size| {
                state.borrow_mut().host_size = Some((inline_size, block_size));
                state.borrow_mut().lay_out();
                after_context(&state);
            }
        });
        panel.ref_on_resize({
            let state = state.clone();
            move |_, inline_size, block_size| {
                state.borrow_mut().panel_size = Some((inline_size, block_size));
                state.borrow_mut().lay_out();
                after_context(&state);
            }
        });
        panel.ref_own(
            |e| EventListener::new_with_options(&e.raw(), "wheel", EventListenerOptions::enable_prevent_default(), {
                let state = state.clone();
                move |e| {
                    let e: &WheelEvent = e.dyn_ref().unwrap();
                    e.prevent_default();
                    let convert = state.borrow().convert();
                    let (_, transverse) = convert.direction_convert_point(e.delta_x(), e.delta_y());
                    with_context(&state, |ctx| ctx.context_scroll_by(transverse));
                }
            }),
        );
        panel.ref_on("mousemove", {
            let state = state.clone();
            move |e| {
                let e: &MouseEvent = e.dyn_ref().unwrap();
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
                    let state = state.clone();
                    move |_| {
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
            let state = state.clone();
            move |_| {
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
                    let state = state.clone();
                    move |e| {
                        let e: &MouseEvent = e.dyn_ref().unwrap();
                        if e.button() != 0 {
                            return;
                        }
                        e.prevent_default();
                        with_context(&state, |ctx| {
                            ctx.mouse_button(true);
                        });
                    }
                },
            ),
        );
        panel.ref_on("mouseup", {
            let state = state.clone();
            move |e| {
                let e: &MouseEvent = e.dyn_ref().unwrap();
                if e.button() != 0 {
                    return;
                }
                with_context(&state, |ctx| {
                    ctx.mouse_button(false);
                });
            }
        });
        *self.attached.borrow_mut() = Some(state);
        return panel;
    }

    fn panel_changed(&self, path: &str) -> Option<bool> {
        if path == self.path {
            return Some(false);
        }
        return None;
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        let Some(state) = self.attached.borrow().clone() else {
            return self.select.borrow().clone();
        };
        let s = state.borrow();
        match s.context.as_ref() {
            Some(ctx) => return ctx.cursor_reference().map(|r| r.reference_format()),
            None => return s.select.clone(),
        }
    }

    fn panel_detach(&self) {
        *self.attached.borrow_mut() = None;
        return;
    }

    fn panel_focusable(&self) -> bool {
        return true;
    }

    fn panel_focused(&self, focused: bool) {
        self.focused.set(focused);
        let Some(state) = self.attached.borrow().clone() else {
            return;
        };
        {
            let mut s = state.borrow_mut();
            s.focused = focused;
            let State { context, select, .. } = &mut *s;
            if let Some(ctx) = context.as_mut() {
                if !focused {
                    ctx.clear_cursor();
                } else if ctx.cursor.is_none() {
                    select_initial(ctx, select);
                }
            }
        }
        after_context(&state);
        return;
    }

    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult {
        let Some(state) = self.attached.borrow().clone() else {
            return PanelResult::Ignored;
        };
        let convert = state.borrow().convert();
        let Some(stroke) = panel_key_stroke(e, convert) else {
            return PanelResult::Ignored;
        };
        let resolved = {
            let mut s = state.borrow_mut();
            match s.context.as_mut() {
                Some(ctx) => ctx.key_resolve(stroke),
                None => return PanelResult::Ignored,
            }
        };
        let action = match resolved {
            KeyResolve::Unbound => return PanelResult::Ignored,
            KeyResolve::Pending => return PanelResult::Used,
            KeyResolve::Action(a) => a,
        };
        let handled = {
            let mut s = state.borrow_mut();
            let ctx = s.context.as_mut().unwrap();
            let handled = ctx.key_action(action);
            if handled {
                ctx.input_flush();
            }
            handled
        };
        after_context(&state);
        if handled {
            return PanelResult::Used;
        }
        return PanelResult::Unused(action);
    }

    fn panel_mouse(&self, _e: &MouseEvent) -> PanelResult {
        return PanelResult::Ignored;
    }

    fn panel_parent(&self) -> Option<String> {
        return None;
    }

    fn panel_path(&self) -> String {
        return self.path.clone();
    }

    fn panel_reference(&self) -> Option<String> {
        return Some(format!("{}{}", self.path, self.panel_cursor_reference()?));
    }

    fn panel_selection(&self) -> Option<(bool, String)> {
        return None;
    }

    fn panel_size(&self) -> f64 {
        return 5.;
    }
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
    context: Option<Context>,
    document: Rc<Document>,
    focused: bool,
    host: El,
    host_size: Option<(f64, f64)>,
    hover_point: Option<Vector>,
    hover_raf: Option<AnimationFrame>,
    keys: Keymap,
    origin: (f64, f64),
    panel: El,
    panel_size: Option<(f64, f64)>,
    select: Option<String>,
    shift: El,
    syntax: Rc<Syntax>,
    timer: Option<Timeout>,
}

impl State {
    fn convert(&self) -> DirectionConvert {
        return self.syntax.spec_root.convert;
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
        let measure = el("canvas").classes(&["merman_measure"]);
        self.host.ref_push(measure.clone());
        let display = DisplayWeb {
            convert: convert,
            root: self.shift.clone(),
            background: self.panel.clone(),
            nodes: vec![],
            measure: measure,
            measure_font: String::new(),
            widths: HashMap::new(),
            metrics: HashMap::new(),
        };
        let mut ctx = Context::context_new(self.syntax.clone(), self.document.clone(), ContextConfig {
            keys: self.keys.clone(),
            ..ContextConfig::default()
        }, Box::new(display), Box::new(EnvironmentWeb), converse, transverse);
        if self.focused {
            select_initial(&mut ctx, &mut self.select);
        }
        self.context = Some(ctx);
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
