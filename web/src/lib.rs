//! Browser side of the viewer: measures text with a canvas, drives the core
//! context (idle timer, mouse, keyboard) and draws the laid bricks and
//! selection/hover boxes as SVG.
use gloo_events::EventListener;
use gloo_render::{request_animation_frame, AnimationFrame};
use gloo_timers::callback::Timeout;
use gloo_utils::{document, window};
use lunk::{link, EventGraph, HistPrim, Prim};
use merman3_core::attachment::DrawingLayer;
use merman3_core::context::{Context, ContextConfig, Vector};
use merman3_core::direction::DirectionConvert;
use merman3_core::matcher::match_document;
use merman3_core::measure::{FontMetrics, FontSpec, Measure};
use merman3_core::render::{PathCommand, RenderDrawing, Snapshot};
use merman3_core::spec::{SpecDirection, SpecSyntax};
use merman3_core::syntax::Syntax;
use rooting::{el, el_from_raw, set_root, El};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, KeyboardEvent, MouseEvent};

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Lines this far outside the viewport are rendered too, so small scrolls
/// don't show blank space before the next frame.
const RENDER_MARGIN: f64 = 800.;

struct MeasureWeb {
    context: CanvasRenderingContext2d,
    current_font: String,
    widths: HashMap<(String, String), f64>,
    metrics: HashMap<String, FontMetrics>,
}

impl MeasureWeb {
    fn new() -> MeasureWeb {
        let canvas: HtmlCanvasElement = document()
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        let context: CanvasRenderingContext2d = canvas
            .get_context("2d")
            .unwrap()
            .expect("canvas 2d context unavailable")
            .dyn_into()
            .unwrap();
        return MeasureWeb {
            context,
            current_font: String::new(),
            widths: HashMap::new(),
            metrics: HashMap::new(),
        };
    }

    fn set_font(&mut self, css: &str) {
        if self.current_font != css {
            self.context.set_font(css);
            self.current_font = css.to_string();
        }
    }
}

impl Measure for MeasureWeb {
    fn measure_width(&mut self, font: &FontSpec, text: &str) -> f64 {
        let css = font.font_css();
        let key = (css, text.to_string());
        if let Some(w) = self.widths.get(&key) {
            return *w;
        }
        self.set_font(&key.0);
        let w = self.context.measure_text(text).unwrap().width();
        self.widths.insert(key, w);
        return w;
    }

    fn measure_metrics(&mut self, font: &FontSpec) -> FontMetrics {
        let css = font.font_css();
        if let Some(m) = self.metrics.get(&css) {
            return *m;
        }
        self.set_font(&css);
        let m = self.context.measure_text("W").unwrap();
        let out = FontMetrics {
            ascent: m.font_bounding_box_ascent(),
            descent: m.font_bounding_box_descent(),
        };
        self.metrics.insert(css, out);
        return out;
    }
}

fn now_ms() -> f64 {
    return js_sys::Date::now();
}

struct State {
    syntax: Rc<Syntax>,
    context: Option<Context>,
    svg: El,
    /// Layers inside the svg, translated so layout coordinates fit.
    background: El,
    text: El,
    overlay: El,
    /// Page offset subtracted from unconverted coordinates.
    origin: (f64, f64),
    origin_known: bool,
    raf: Option<AnimationFrame>,
    timer: Option<Timeout>,
}

impl State {
    fn convert(&self) -> DirectionConvert {
        return self.syntax.spec_root.convert;
    }

    /// Page coordinates of the svg's origin.
    fn svg_page(&self) -> (f64, f64) {
        let rect = self.svg.raw().get_bounding_client_rect();
        let w = window();
        return (
            rect.left() + w.scroll_x().unwrap_or(0.),
            rect.top() + w.scroll_y().unwrap_or(0.),
        );
    }

    /// Layout coordinates of a page point.
    fn page_to_layout(&self, x: f64, y: f64) -> Vector {
        let svg = self.svg_page();
        let local = (x - svg.0 + self.origin.0, y - svg.1 + self.origin.1);
        let (c, t) = self.convert().direction_convert_point(local.0, local.1);
        return Vector::new(c, t);
    }

    /// Draw everything laid out so far and any pending scroll; returns the
    /// clipboard text to write, if copying happened.
    fn render(&mut self) -> Option<String> {
        let convert = self.convert();
        let Some(context) = self.context.as_mut() else {
            return None;
        };
        let snapshot = context.render_snapshot();
        let clipboard = context.clipboard.take();
        let pending_scroll = context.pending_scroll.take();
        let edge = context.edge;
        // Size the svg to the laid extent and translate layout coordinates into it
        let (t_min, t_max) = snapshot.transverse;
        let (x_span, y_span) = convert.direction_unconvert_span(snapshot.width.max(edge), t_max - t_min);
        let new_origin = convert.direction_unconvert(0., t_min, x_span, y_span);
        let old_origin = self.origin;
        self.origin = new_origin;
        self.svg
            .ref_attr("width", &format!("{}", x_span))
            .ref_attr("height", &format!("{}", y_span));
        let transform = format!("translate({} {})", -new_origin.0, -new_origin.1);
        self.background.ref_attr("transform", &transform);
        self.text.ref_attr("transform", &transform);
        self.overlay.ref_attr("transform", &transform);
        // Keep the view still when courses are laid before the start
        if self.origin_known && (old_origin.0 != new_origin.0 || old_origin.1 != new_origin.1) {
            window().scroll_by_with_x_and_y(old_origin.0 - new_origin.0, old_origin.1 - new_origin.1);
        }
        self.origin_known = true;
        self.render_rows(&snapshot);
        self.render_drawings(&snapshot);
        if let Some(scroll) = pending_scroll {
            let transverse_edge = self.context.as_ref().unwrap().transverse_edge;
            let axis = convert.direction_unconvert_transverse(scroll, transverse_edge, transverse_edge);
            let svg = self.svg_page();
            if axis.x {
                let x = axis.amount - self.origin.0 + svg.0;
                window().scroll_to_with_x_and_y(x, window().scroll_y().unwrap_or(0.));
            } else {
                let y = axis.amount - self.origin.1 + svg.1;
                window().scroll_to_with_x_and_y(window().scroll_x().unwrap_or(0.), y);
            }
        }
        return clipboard;
    }

    fn render_rows(&self, snapshot: &Snapshot) {
        let convert = self.convert();
        let w = window();
        let scroll = (w.scroll_x().unwrap_or(0.), w.scroll_y().unwrap_or(0.));
        let viewport = (
            w.inner_width().unwrap().as_f64().unwrap_or(0.),
            w.inner_height().unwrap().as_f64().unwrap_or(0.),
        );
        let svg = self.svg_page();
        // Lines stack along x when text runs vertically
        let (view_start, view_size, svg_start, origin) = if convert.direction_converse_vertical() {
            (scroll.0, viewport.0, svg.0, self.origin.0)
        } else {
            (scroll.1, viewport.1, svg.1, self.origin.1)
        };
        let start = view_start - svg_start + origin - RENDER_MARGIN;
        let end = view_start - svg_start + origin + view_size + RENDER_MARGIN;
        let mut els = vec![];
        for row in &snapshot.rows {
            let span = row.ascent + row.descent;
            let axis = convert.direction_unconvert_transverse(row.transverse, span, span);
            if axis.amount + span < start || axis.amount > end {
                continue;
            }
            let baseline = row.transverse + row.ascent;
            for b in &row.bricks {
                let style = self.syntax.syntax_style(b.style);
                let text = el_from_raw(document().create_element_ns(Some(SVG_NS), "text").unwrap());
                text.ref_attr("font-family", &style.font.family)
                    .ref_attr("font-size", &format!("{}", style.font.size))
                    .ref_attr("fill", &style.color)
                    .ref_text(&b.text);
                let text_width = b.converse_span - b.pad_before;
                let height = b.ascent + b.descent;
                let converse = b.converse + b.pad_before;
                let transverse = baseline - b.ascent;
                if convert.direction_converse_vertical() {
                    let (x, y) = convert.direction_unconvert(converse, transverse, height, text_width);
                    let center_x = x + height / 2.;
                    let mode = match convert.transverse {
                        SpecDirection::Left => "vertical-rl",
                        _ => "vertical-lr",
                    };
                    text.ref_attr("x", &format!("{}", center_x))
                        .ref_attr("y", &format!("{}", y))
                        .ref_attr("dominant-baseline", "central")
                        .ref_attr("style", &format!("writing-mode: {}", mode));
                    if convert.converse == SpecDirection::Up {
                        text.ref_attr(
                            "transform",
                            &format!("rotate(180 {} {})", center_x, y + text_width / 2.),
                        );
                    }
                } else {
                    let (x, y) = convert.direction_unconvert(converse, transverse, text_width, height);
                    text.ref_attr("x", &format!("{}", x))
                        .ref_attr("y", &format!("{}", y + b.ascent));
                }
                els.push(text);
            }
        }
        self.text.ref_clear();
        self.text.ref_extend(els);
    }

    fn render_drawings(&self, snapshot: &Snapshot) {
        let convert = self.convert();
        let point = |v: Vector| convert.direction_unconvert(v.converse, v.transverse, 0., 0.);
        let mut background = vec![];
        let mut overlay = vec![];
        for d in &snapshot.drawings {
            let (layer, el) = match d {
                RenderDrawing::Obbox {
                    layer,
                    path,
                    line,
                    line_color,
                    line_thickness,
                    fill,
                    fill_color,
                } => {
                    let mut data = String::new();
                    let mut current = (0., 0.);
                    for cmd in path {
                        match cmd {
                            PathCommand::MoveTo(p) => {
                                current = point(*p);
                                data.push_str(&format!("M {} {} ", current.0, current.1));
                            }
                            PathCommand::LineTo(p) => {
                                current = point(*p);
                                data.push_str(&format!("L {} {} ", current.0, current.1));
                            }
                            PathCommand::ArcTo { corner, to, radius } => {
                                let corner = point(*corner);
                                let to = point(*to);
                                if *radius <= 0. {
                                    data.push_str(&format!("L {} {} ", corner.0, corner.1));
                                    current = corner;
                                    continue;
                                }
                                // Tangent point on the incoming segment
                                let d0 = (current.0 - corner.0, current.1 - corner.1);
                                let len0 = (d0.0 * d0.0 + d0.1 * d0.1).sqrt();
                                let t1 = if len0 == 0. {
                                    corner
                                } else {
                                    (
                                        corner.0 + d0.0 / len0 * radius,
                                        corner.1 + d0.1 / len0 * radius,
                                    )
                                };
                                let cross = (t1.0 - corner.0) * (to.1 - corner.1) - (t1.1 - corner.1) * (to.0 - corner.0);
                                let sweep = if cross < 0. { 1 } else { 0 };
                                data.push_str(&format!(
                                    "L {} {} A {} {} 0 0 {} {} {} ",
                                    t1.0, t1.1, radius, radius, sweep, to.0, to.1
                                ));
                                current = to;
                            }
                        }
                    }
                    data.push('Z');
                    let el = el_from_raw(document().create_element_ns(Some(SVG_NS), "path").unwrap());
                    el.ref_attr("d", &data)
                        .ref_attr("fill", if *fill { fill_color } else { "none" })
                        .ref_attr("stroke", if *line { line_color } else { "none" })
                        .ref_attr("stroke-width", &format!("{}", line_thickness));
                    (*layer, el)
                }
                RenderDrawing::Line {
                    layer,
                    from,
                    to,
                    thickness,
                    color,
                    round_cap,
                } => {
                    let (from, to) = (point(*from), point(*to));
                    let el = el_from_raw(document().create_element_ns(Some(SVG_NS), "line").unwrap());
                    el.ref_attr("x1", &format!("{}", from.0))
                        .ref_attr("y1", &format!("{}", from.1))
                        .ref_attr("x2", &format!("{}", to.0))
                        .ref_attr("y2", &format!("{}", to.1))
                        .ref_attr("stroke", color)
                        .ref_attr("stroke-width", &format!("{}", thickness))
                        .ref_attr("stroke-linecap", if *round_cap { "round" } else { "butt" });
                    (*layer, el)
                }
            };
            match layer {
                DrawingLayer::Background => background.push(el),
                DrawingLayer::Overlay => overlay.push(el),
            }
        }
        self.background.ref_clear();
        self.background.ref_extend(background);
        self.overlay.ref_clear();
        self.overlay.ref_extend(overlay);
    }
}

/// Run `f` on the context, then render and schedule the idle timer if the
/// context asked for it.
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
    let clipboard = state.borrow_mut().render();
    if let Some(text) = clipboard {
        let _ = window().navigator().clipboard().write_text(&text);
    }
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
                    ctx.handle_timer(&mut now_ms);
                }
            }
            after_context(&state2);
        });
        state.borrow_mut().timer = Some(timeout);
    }
}

/// CSS for the container: the syntax's padding mapped onto page sides, and a
/// fixed viewport height when lines stack horizontally (so the page scrolls
/// sideways instead of growing).
fn container_style(syntax: &Syntax) -> String {
    let convert = syntax.spec_root.convert;
    let pad = &syntax.spec_root.pad;
    let mut sides = [0.; 4]; // top, right, bottom, left
    let mut put = |d: SpecDirection, start: f64, end: f64| match d {
        SpecDirection::Right => {
            sides[3] += start;
            sides[1] += end;
        }
        SpecDirection::Left => {
            sides[1] += start;
            sides[3] += end;
        }
        SpecDirection::Down => {
            sides[0] += start;
            sides[2] += end;
        }
        SpecDirection::Up => {
            sides[2] += start;
            sides[0] += end;
        }
    };
    put(convert.converse, pad.converse_start, pad.converse_end);
    put(convert.transverse, pad.transverse_start, pad.transverse_end);
    let mut out = format!(
        "padding: {}px {}px {}px {}px",
        sides[0], sides[1], sides[2], sides[3]
    );
    if convert.direction_converse_vertical() {
        out.push_str("; height: 100vh");
    }
    return out;
}

fn read_embedded(id: &str) -> Result<String, String> {
    let Some(e) = document().get_element_by_id(id) else {
        return Err(format!("Page is missing embedded data element `{}`", id));
    };
    return Ok(e.text_content().unwrap_or_default());
}

fn show_error(message: String) {
    let pre = el("pre").classes(&["merman-error"]).text(&message);
    set_root(vec![pre]);
}

fn svg_el(tag: &str) -> El {
    return el_from_raw(document().create_element_ns(Some(SVG_NS), tag).unwrap());
}

fn run() -> Result<(), String> {
    let spec: SpecSyntax = serde_json::from_str(&read_embedded("merman-syntax")?)
        .map_err(|e| format!("Error parsing syntax JSON: {}", e))?;
    let syntax = Rc::new(
        Syntax::syntax_resolve(spec).map_err(|e| format!("Syntax errors:\n{}", e.join("\n")))?,
    );
    let value: serde_json::Value = serde_json::from_str(&read_embedded("merman-source")?)
        .map_err(|e| format!("Error parsing source JSON: {}", e))?;
    let document_ = Rc::new(
        match_document(&syntax, &value)
            .map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?,
    );
    document()
        .body()
        .unwrap()
        .style()
        .set_property("background", &syntax.spec_root.background)
        .unwrap();
    let svg = svg_el("svg");
    let background = svg_el("g");
    let text = svg_el("g");
    let overlay = svg_el("g");
    svg.ref_push(background.clone())
        .ref_push(text.clone())
        .ref_push(overlay.clone());
    let container = el("div")
        .classes(&["merman"])
        .attr("style", &container_style(&syntax))
        .push(svg.clone());
    let state = Rc::new(RefCell::new(State {
        syntax: syntax.clone(),
        context: None,
        svg,
        background,
        text,
        overlay,
        origin: (0., 0.),
        origin_known: false,
        raf: None,
        timer: None,
    }));
    let eg = EventGraph::new();
    eg.event(|pc| {
        let size: HistPrim<Option<(f64, f64)>> = HistPrim::new(pc, None);
        let view: HistPrim<((f64, f64), (f64, f64))> = HistPrim::new(pc, ((0., 0.), (0., 0.)));
        let layout_version: Prim<u64> = Prim::new(0);
        // Container size -> context edge (creating the context on first size)
        let link_size = link!(
            (pc = pc),
            (size = size.clone()),
            (layout_version = layout_version.clone()),
            (state = state.clone(), syntax = syntax.clone(), document_ = document_.clone()) {
                let Some((inline, block)) = *size.borrow() else {
                    return None;
                };
                let convert = syntax.spec_root.convert;
                let (converse, transverse) = convert.direction_convert_span(inline, block);
                {
                    let mut s = state.borrow_mut();
                    match s.context.as_mut() {
                        Some(ctx) => ctx.context_resize(converse, transverse),
                        None => {
                            s.context = Some(Context::context_new(
                                syntax.clone(),
                                document_.clone(),
                                ContextConfig::default(),
                                Box::new(MeasureWeb::new()),
                                converse,
                                transverse,
                            ));
                        }
                    }
                }
                after_context(state);
                let next = *layout_version.borrow() + 1;
                layout_version.set(pc, next);
            }
        );
        // Scroll/viewport -> which rows are drawn, and merman's scroll offset
        let link_view = link!(
            (_pc = pc),
            (layout_version = layout_version.clone(), view = view.clone()),
            (),
            (state = state.clone(), syntax = syntax.clone()) {
                let _ = layout_version;
                let ((scroll_x, scroll_y), _viewport) = *view.borrow();
                let convert = syntax.spec_root.convert;
                {
                    let mut s = state.borrow_mut();
                    let svg = s.svg_page();
                    let (axis_scroll, svg_start, origin) = if convert.direction_converse_vertical() {
                        (scroll_x, svg.0, s.origin.0)
                    } else {
                        (scroll_y, svg.1, s.origin.1)
                    };
                    if let Some(ctx) = s.context.as_mut() {
                        let edge = ctx.transverse_edge;
                        let transverse = convert.direction_convert_transverse(axis_scroll - svg_start + origin, edge);
                        ctx.context_scrolled(transverse);
                    }
                }
                state.borrow_mut().render();
            }
        );
        container.ref_own(|_| (link_size, link_view));
        container.ref_on_resize({
            let eg = eg.clone();
            let size = size.clone();
            move |_, inline_size, block_size| {
                eg.event(|pc| size.set(pc, Some((inline_size, block_size))));
            }
        });
        let schedule_view_update = {
            let eg = eg.clone();
            let view = view.clone();
            let state = state.clone();
            Rc::new(move || {
                let eg = eg.clone();
                let view = view.clone();
                let state2 = state.clone();
                let frame = request_animation_frame(move |_| {
                    state2.borrow_mut().raf = None;
                    let w = window();
                    let scroll = (w.scroll_x().unwrap_or(0.), w.scroll_y().unwrap_or(0.));
                    let viewport = (
                        w.inner_width().unwrap().as_f64().unwrap_or(0.),
                        w.inner_height().unwrap().as_f64().unwrap_or(0.),
                    );
                    eg.event(|pc| view.set(pc, (scroll, viewport)));
                });
                state.borrow_mut().raf = Some(frame);
            })
        };
        container.ref_own(|_| {
            let s = schedule_view_update.clone();
            EventListener::new(&window(), "scroll", move |_| s())
        });
        container.ref_own(|_| {
            let s = schedule_view_update.clone();
            EventListener::new(&window(), "resize", move |_| s())
        });
        // Mouse
        container.ref_on("mousemove", {
            let state = state.clone();
            move |e| {
                let e: &MouseEvent = e.dyn_ref().unwrap();
                let w = window();
                let x = e.client_x() as f64 + w.scroll_x().unwrap_or(0.);
                let y = e.client_y() as f64 + w.scroll_y().unwrap_or(0.);
                let point = state.borrow().page_to_layout(x, y);
                with_context(&state, |ctx| ctx.mouse_moved(point));
            }
        });
        container.ref_on("mouseleave", {
            let state = state.clone();
            move |_| with_context(&state, |ctx| ctx.mouse_exited())
        });
        container.ref_on("mousedown", {
            let state = state.clone();
            move |e| {
                let e: &MouseEvent = e.dyn_ref().unwrap();
                if e.button() != 0 {
                    return;
                }
                e.prevent_default();
                with_context(&state, |ctx| {
                    ctx.mouse_button(true, &mut now_ms);
                });
            }
        });
        container.ref_on("mouseup", {
            let state = state.clone();
            move |e| {
                let e: &MouseEvent = e.dyn_ref().unwrap();
                if e.button() != 0 {
                    return;
                }
                with_context(&state, |ctx| {
                    ctx.mouse_button(false, &mut now_ms);
                });
            }
        });
        container.ref_own(|_| {
            let state = state.clone();
            EventListener::new(&document(), "keydown", move |e| {
                let e: &KeyboardEvent = e.dyn_ref().unwrap();
                if (e.ctrl_key() || e.meta_key()) && e.key() == "c" {
                    e.prevent_default();
                    with_context(&state, |ctx| ctx.key_copy(&mut now_ms));
                }
            })
        });
        schedule_view_update();
    });
    set_root(vec![container]);
    return Ok(());
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    if let Err(e) = run() {
        show_error(e);
    }
}
