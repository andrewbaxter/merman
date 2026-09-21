mod display;

use crate::panels::code::display::{
    display_el,
    DisplayWeb,
};
use crate::panels::{
    panel_key_stroke,
    PanelKey,
};
use gloo_render::{
    request_animation_frame,
    AnimationFrame,
};
use gloo_timers::callback::Timeout;
use gloo_utils::window;
use merman3_core::context::{
    Context,
    ContextConfig,
    Vector,
};
use merman3_core::direction::DirectionConvert;
use merman3_core::document::Document;
use merman3_core::environment::Environment;
use merman3_core::keys::{
    KeyResolve,
    Keymap,
};
use merman3_core::syntax::Syntax;
use rooting::{
    el,
    El,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{
    KeyboardEvent,
    MouseEvent,
    WheelEvent,
};

struct EnvironmentWeb;

impl Environment for EnvironmentWeb {
    fn environment_now_ms(&mut self) -> f64 {
        return js_sys::Date::now();
    }

    fn environment_clipboard_set(&mut self, text: &str) {
        let _ = window().navigator().clipboard().write_text(text);
    }
}

struct State {
    syntax: Rc<Syntax>,
    document: Rc<Document>,
    keys: Keymap,
    context: Option<Context>,
    panel: El,
    host: El,
    shift: El,
    origin: (f64, f64),
    host_size: Option<(f64, f64)>,
    panel_size: Option<(f64, f64)>,
    hover_point: Option<Vector>,
    hover_raf: Option<AnimationFrame>,
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
        self.context = Some(Context::context_new(self.syntax.clone(), self.document.clone(), ContextConfig {
            keys: self.keys.clone(),
            ..ContextConfig::default()
        }, Box::new(display), Box::new(EnvironmentWeb), converse, transverse));
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
            let new_origin = convert.direction_unconvert(0., t_min, x_span, y_span);
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

pub struct CodePanel(Rc<RefCell<State>>);

impl CodePanel {
    pub fn code_new(keys: Keymap, syntax: Rc<Syntax>, document: Rc<Document>) -> CodePanel {
        let shift = display_el("merman_origin");
        let host = el("div").classes(&["merman"]).push(shift.clone());
        let panel = el("div").classes(&["merman_panel", "merman_panel_code"]).push(host.clone());
        let state = Rc::new(RefCell::new(State {
            syntax: syntax,
            document: document,
            keys: keys,
            context: None,
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
        let out = CodePanel(state.clone());
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
        panel.ref_on("wheel", {
            let state = state.clone();
            move |e| {
                let e: &WheelEvent = e.dyn_ref().unwrap();
                e.prevent_default();
                let convert = state.borrow().convert();
                let (_, transverse) = convert.direction_convert_point(e.delta_x(), e.delta_y());
                with_context(&state, |ctx| ctx.context_scroll_by(transverse));
            }
        });
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
        panel.ref_on("mousedown", {
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
        });
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
        return out;
    }

    pub fn code_element(&self) -> El {
        return self.0.borrow().panel.clone();
    }

    pub fn code_set(&self, syntax: Rc<Syntax>, document: Rc<Document>) {
        {
            let mut s = self.0.borrow_mut();
            s.syntax = syntax;
            s.document = document;
            s.context = None;
            s.origin = (0., 0.);
            s.shift.ref_clear();
        }
        self.0.borrow_mut().lay_out();
        after_context(&self.0);
    }

    pub fn code_key(&self, e: &KeyboardEvent) -> PanelKey {
        let convert = self.0.borrow().convert();
        let Some(stroke) = panel_key_stroke(e, convert) else {
            return PanelKey::Ignored;
        };
        let resolved = {
            let mut s = self.0.borrow_mut();
            match s.context.as_mut() {
                Some(ctx) => ctx.key_resolve(stroke),
                None => return PanelKey::Ignored,
            }
        };
        let action = match resolved {
            KeyResolve::Unbound => return PanelKey::Ignored,
            KeyResolve::Pending => return PanelKey::Used,
            KeyResolve::Action(a) => a,
        };
        let handled = {
            let mut s = self.0.borrow_mut();
            let ctx = s.context.as_mut().unwrap();
            let handled = ctx.key_action(action);
            if handled {
                ctx.input_flush();
            }
            handled
        };
        after_context(&self.0);
        if handled {
            return PanelKey::Used;
        }
        return PanelKey::Unused(action);
    }
}
