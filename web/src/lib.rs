//! Browser side of the viewer: measures text with a canvas, lays the embedded
//! document out with the core engine and draws the visible lines as SVG text.
use gloo_events::EventListener;
use gloo_render::{request_animation_frame, AnimationFrame};
use gloo_utils::{document, window};
use lunk::{link, EventGraph, HistPrim, Prim};
use merman3_core::document::Document;
use merman3_core::layout::{Layout, LayoutConfig, Rows};
use merman3_core::spec::SpecDirection;
use merman3_core::matcher::match_document;
use merman3_core::measure::{FontMetrics, FontSpec, Measure};
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use merman3_core::visual::Visual;
use rooting::{el, el_from_raw, set_root, El};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

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

/// A laid out line with its extent along the page's scroll axis.
struct PageRow {
    page_start: f64,
    page_end: f64,
}

struct State {
    syntax: Rc<Syntax>,
    visual: Rc<Visual>,
    measure: MeasureWeb,
    layout: Option<Layout>,
    svg: El,
    /// Translates layout page coordinates (which may be negative for left/up
    /// directions) into the svg.
    group: El,
    rows: Option<Rows>,
    page_rows: Vec<PageRow>,
    /// Page offset of the group (subtracted from unconverted coordinates).
    origin: (f64, f64),
    raf: Option<AnimationFrame>,
}

impl State {
    /// `inline` and `block` are the container's content box size.
    fn relayout(&mut self, inline: f64, block: f64) {
        let convert = self.syntax.spec_root.convert;
        let (converse_size, _) = convert.direction_convert_span(inline, block);
        let pad = &self.syntax.spec_root.pad;
        let edge = f64::max(0., converse_size - pad.converse_start - pad.converse_end);
        match &mut self.layout {
            Some(l) => l.layout_set_edge(edge, &mut self.measure),
            None => {
                self.layout = Some(Layout::layout_build(
                    self.syntax.clone(),
                    self.visual.clone(),
                    LayoutConfig::default(),
                    edge,
                    &mut self.measure,
                ));
            }
        }
        let rows = self.layout.as_ref().unwrap().layout_rows();
        let (x_span, y_span) = convert.direction_unconvert_span(rows.width.max(edge), rows.height);
        self.origin = convert.direction_unconvert(0., 0., x_span, y_span);
        self.svg
            .ref_attr("width", &format!("{}", x_span))
            .ref_attr("height", &format!("{}", y_span));
        self.group
            .ref_attr("transform", &format!("translate({} {})", -self.origin.0, -self.origin.1));
        self.page_rows = rows
            .rows
            .iter()
            .map(|r| {
                let span = r.ascent + r.descent;
                let a = convert.direction_unconvert_transverse(r.transverse, span, span);
                let start = a.amount - if a.x { self.origin.0 } else { self.origin.1 };
                return PageRow {
                    page_start: start,
                    page_end: start + span,
                };
            })
            .collect();
        self.rows = Some(rows);
    }

    /// Draw the lines intersecting the viewport.
    fn render(&self, scroll: (f64, f64), viewport: (f64, f64)) {
        let Some(rows) = &self.rows else {
            return;
        };
        let convert = self.syntax.spec_root.convert;
        let rect = self.svg.raw().get_bounding_client_rect();
        let svg_page = (rect.left() + scroll.0, rect.top() + scroll.1);
        // Lines stack along x when text runs vertically
        let (view_start, view_size, svg_start) = if convert.direction_converse_vertical() {
            (scroll.0, viewport.0, svg_page.0)
        } else {
            (scroll.1, viewport.1, svg_page.1)
        };
        let start = view_start - svg_start - RENDER_MARGIN;
        let end = view_start - svg_start + view_size + RENDER_MARGIN;
        let mut els = vec![];
        for (row, page) in rows.rows.iter().zip(self.page_rows.iter()) {
            if page.page_end < start || page.page_start > end {
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
                let text_width = b.width - b.pad_before;
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
        self.group.ref_clear();
        self.group.ref_extend(els);
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

fn run() -> Result<(), String> {
    let spec: SpecSyntax = serde_json::from_str(&read_embedded("merman-syntax")?)
        .map_err(|e| format!("Error parsing syntax JSON: {}", e))?;
    let syntax = Syntax::syntax_resolve(spec)
        .map_err(|e| format!("Syntax errors:\n{}", e.join("\n")))?;
    let value: serde_json::Value = serde_json::from_str(&read_embedded("merman-source")?)
        .map_err(|e| format!("Error parsing source JSON: {}", e))?;
    let doc: Document = match_document(&syntax, &value)
        .map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?;
    let mut measure = MeasureWeb::new();
    let visual = Visual::visual_build(&syntax, &doc, &mut measure);
    document()
        .body()
        .unwrap()
        .style()
        .set_property("background", &syntax.spec_root.background)
        .unwrap();
    let svg = el_from_raw(document().create_element_ns(Some(SVG_NS), "svg").unwrap());
    let group = el_from_raw(document().create_element_ns(Some(SVG_NS), "g").unwrap());
    svg.ref_push(group.clone());
    let container = el("div")
        .classes(&["merman"])
        .attr("style", &container_style(&syntax))
        .push(svg.clone());
    let state = Rc::new(RefCell::new(State {
        syntax: Rc::new(syntax),
        visual: Rc::new(visual),
        measure,
        layout: None,
        svg,
        group,
        rows: None,
        page_rows: vec![],
        origin: (0., 0.),
        raf: None,
    }));
    let eg = EventGraph::new();
    eg.event(|pc| {
        let size: HistPrim<Option<(f64, f64)>> = HistPrim::new(pc, None);
        let view: HistPrim<((f64, f64), (f64, f64))> = HistPrim::new(pc, ((0., 0.), (0., 0.)));
        let layout_version: Prim<u64> = Prim::new(0);
        let link_layout = link!(
            (pc = pc),
            (size = size.clone()),
            (layout_version = layout_version.clone()),
            (state = state.clone()) {
                let Some((inline, block)) = *size.borrow() else {
                    return None;
                };
                state.borrow_mut().relayout(inline, block);
                let next = *layout_version.borrow() + 1;
                layout_version.set(pc, next);
            }
        );
        let link_render = link!(
            (_pc = pc),
            (layout_version = layout_version.clone(), view = view.clone()),
            (),
            (state = state.clone()) {
                let _ = layout_version;
                let (scroll, viewport) = *view.borrow();
                state.borrow().render(scroll, viewport);
            }
        );
        container.ref_own(|_| (link_layout, link_render));
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
