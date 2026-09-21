use gloo_utils::window;
use merman3_core::context::Vector;
use merman3_core::direction::DirectionConvert;
use merman3_core::display::{
    Display,
    DisplayLayer,
    DisplayNodeId,
    DrawCommand,
};
use merman3_core::measure::{
    FontMetrics,
    FontSpec,
};
use rooting::{
    el,
    El,
};
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use web_sys::{
    CanvasRenderingContext2d,
    HtmlCanvasElement,
    TextMetrics,
};

pub fn display_el(class: &str) -> El {
    return el("div").classes(&["merman_display", class]);
}

fn canvas_el(class: &str) -> El {
    return el("canvas").classes(&["merman_display", class]);
}

fn set_style(el: &El, key: &str, value: &str) {
    let style = el.raw().dyn_ref::<web_sys::HtmlElement>().unwrap().style();
    let _ = style.set_property(key, value);
}

fn canvas_context(el: &El) -> CanvasRenderingContext2d {
    let canvas: HtmlCanvasElement = el.raw().dyn_into().unwrap();
    return canvas.get_context("2d").unwrap().unwrap().dyn_into().unwrap();
}

fn canvas_resize(el: &El, width: f64, height: f64) -> CanvasRenderingContext2d {
    let canvas: HtmlCanvasElement = el.raw().clone().dyn_into().unwrap();
    let ratio = {
        let reported = window().device_pixel_ratio();
        if reported > 0. {
            reported
        } else {
            1.
        }
    };
    let (width, height) = (width.ceil().max(1.), height.ceil().max(1.));
    canvas.set_width((width * ratio) as u32);
    canvas.set_height((height * ratio) as u32);
    set_style(el, "width", &format!("{}px", width));
    set_style(el, "height", &format!("{}px", height));
    let ctx = canvas_context(el);
    let _ = ctx.set_transform(ratio, 0., 0., ratio, 0., 0.);
    return ctx;
}

enum Kind {
    Group,
    Text(Text),
    Blank,
    Drawing(Vector),
    Image,
}

#[derive(Default)]
pub struct Text {
    text: String,
    font: String,
    color: String,
    ink_ascent: f64,
    ink_descent: f64,
    pad_converse_half: f64,
    pad_transverse_half: f64,
}

pub struct Node {
    el: El,
    kind: Kind,
    parent: Option<DisplayNodeId>,
    children: Vec<DisplayNodeId>,
    converse: f64,
    transverse: f64,
    baseline: f64,
    converse_span: f64,
    ascent: f64,
    descent: f64,
    animated: bool,
    placed: (f64, f64),
}

pub struct DisplayWeb {
    pub convert: DirectionConvert,
    pub root: El,
    pub background: El,
    pub nodes: Vec<Option<Node>>,
    pub measure: El,
    pub measure_font: String,
    pub widths: HashMap<(String, String), f64>,
    pub metrics: HashMap<String, FontMetrics>,
}

impl DisplayWeb {
    fn new_node(&mut self, el: El, kind: Kind) -> DisplayNodeId {
        let id = self.nodes.len();
        self.nodes.push(Some(Node {
            el: el,
            kind: kind,
            parent: None,
            children: vec![],
            converse: 0.,
            transverse: 0.,
            baseline: 0.,
            converse_span: 0.,
            ascent: 0.,
            descent: 0.,
            animated: false,
            placed: (f64::NAN, f64::NAN),
        }));
        return id;
    }

    fn measure_context(&mut self, font: &FontSpec) -> CanvasRenderingContext2d {
        let ctx = canvas_context(&self.measure);
        let css = font.font_css();
        if self.measure_font != css {
            ctx.set_font(&css);
            self.measure_font = css;
        }
        return ctx;
    }

    fn group_span_changed(&mut self, node: DisplayNodeId) {
        let mut at = self.nodes[node].as_ref().and_then(|n| n.parent);
        while let Some(group) = at {
            let mut span: f64 = 0.;
            for child in self.nodes[group].as_ref().unwrap().children.clone() {
                let Some(n) = self.nodes[child].as_ref() else {
                    continue;
                };
                span = span.max(match n.kind {
                    Kind::Group => n.converse_span,
                    _ => n.converse + n.converse_span,
                });
            }
            let n = self.nodes[group].as_mut().unwrap();
            if n.converse_span == span {
                return;
            }
            n.converse_span = span;
            at = n.parent;
        }
    }

    fn fix_position(&mut self, node: DisplayNodeId, animate: bool) {
        let Some(n) = self.nodes[node].as_ref() else {
            return;
        };
        let (corner, span) = match &n.kind {
            Kind::Text(t) => (
                Vector::new(n.converse - t.pad_converse_half, n.baseline - t.ink_ascent - t.pad_transverse_half),
                Vector::new(
                    n.converse_span + t.pad_converse_half * 2.,
                    t.ink_ascent + t.ink_descent + t.pad_transverse_half * 2.,
                ),
            ),
            Kind::Blank => (Vector::new(n.converse, 0.), Vector::new(n.converse_span, n.ascent + n.descent)),
            Kind::Drawing(size) => (Vector::new(n.converse, n.transverse), *size),
            Kind::Group | Kind::Image => (
                Vector::new(n.converse, n.transverse),
                Vector::new(n.converse_span, n.ascent + n.descent),
            ),
        };
        let (x_span, y_span) = self.convert.direction_unconvert_span(span.converse, span.transverse);
        let (x, y) = self.convert.direction_unconvert(corner.converse, corner.transverse, x_span, y_span);
        let n = self.nodes[node].as_mut().unwrap();
        if n.animated != animate {
            n.animated = animate;
            if animate {
                n.el.ref_classes(&["merman_animate"]);
            } else {
                n.el.ref_remove_classes(&["merman_animate"]);
            }
        }
        if n.placed == (x, y) {
            return;
        }
        n.placed = (x, y);
        set_style(&n.el, "left", &format!("{}px", x));
        set_style(&n.el, "top", &format!("{}px", y));
    }
}

impl Display for DisplayWeb {
    fn display_font_width(&mut self, font: &FontSpec, text: &str) -> f64 {
        let css = font.font_css();
        let key = (css, text.to_string());
        if let Some(w) = self.widths.get(&key) {
            return *w;
        }
        let ctx = self.measure_context(font);
        let w = ctx.measure_text(text).unwrap().width();
        self.widths.insert(key, w);
        return w;
    }

    fn display_font_metrics(&mut self, font: &FontSpec) -> FontMetrics {
        let css = font.font_css();
        if let Some(m) = self.metrics.get(&css) {
            return *m;
        }
        let ctx = self.measure_context(font);
        let measured: TextMetrics = ctx.measure_text("Wgy|").unwrap();
        let out = FontMetrics {
            ascent: measured.actual_bounding_box_ascent().max(0.),
            descent: measured.actual_bounding_box_descent().max(0.),
        };
        self.metrics.insert(css, out);
        return out;
    }

    fn display_layer(&mut self, layer: DisplayLayer) -> DisplayNodeId {
        let class = match layer {
            DisplayLayer::Background => "merman_background",
            DisplayLayer::Text => "merman_text",
            DisplayLayer::Overlay => "merman_overlay",
        };
        let node = self.new_node(display_el(class), Kind::Group);
        let at = self.display_root_child_count();
        self.display_root_add(at, node);
        return node;
    }

    fn display_group(&mut self) -> DisplayNodeId {
        return self.new_node(display_el("merman_display_group"), Kind::Group);
    }

    fn display_text(&mut self) -> DisplayNodeId {
        return self.new_node(canvas_el("merman_display_text"), Kind::Text(Text::default()));
    }

    fn display_blank(&mut self) -> DisplayNodeId {
        return self.new_node(display_el("merman_display_blank"), Kind::Blank);
    }

    fn display_image(&mut self) -> DisplayNodeId {
        return self.new_node(el("img").classes(&["merman_display", "merman_display_image"]), Kind::Image);
    }

    fn image_set_source(&mut self, node: DisplayNodeId, path: &str) {
        self.nodes[node].as_ref().unwrap().el.ref_attr("src", path);
    }

    fn image_set_rotate(&mut self, node: DisplayNodeId, rotate: f64) {
        set_style(&self.nodes[node].as_ref().unwrap().el, "transform", &format!("rotate({}deg)", rotate));
    }

    fn display_drawing(&mut self) -> DisplayNodeId {
        return self.new_node(canvas_el("merman_display_drawing"), Kind::Drawing(Vector::default()));
    }

    fn display_destroy(&mut self, node: DisplayNodeId) {
        self.nodes[node] = None;
    }

    fn display_set_background(&mut self, color: &str) {
        set_style(&self.background, "background", color);
    }

    fn display_root_add(&mut self, index: usize, node: DisplayNodeId) {
        let el = self.nodes[node].as_ref().unwrap().el.clone();
        self.root.ref_splice(index, 0, vec![el]);
    }

    fn display_root_child_count(&self) -> usize {
        return self.root.raw().child_element_count() as usize;
    }

    fn node_set_converse(&mut self, node: DisplayNodeId, converse: f64, animate: bool) {
        self.nodes[node].as_mut().unwrap().converse = converse;
        self.fix_position(node, animate);
        self.group_span_changed(node);
    }

    fn node_set_transverse(&mut self, node: DisplayNodeId, transverse: f64, animate: bool) {
        self.nodes[node].as_mut().unwrap().transverse = transverse;
        self.fix_position(node, animate);
    }

    fn node_set_baseline_transverse(&mut self, node: DisplayNodeId, baseline: f64, animate: bool) {
        self.nodes[node].as_mut().unwrap().baseline = baseline;
        self.fix_position(node, animate);
    }

    fn node_set_position(&mut self, node: DisplayNodeId, converse: f64, transverse: f64, animate: bool) {
        {
            let n = self.nodes[node].as_mut().unwrap();
            n.converse = converse;
            n.transverse = transverse;
        }
        self.fix_position(node, animate);
    }

    fn node_set_span(&mut self, node: DisplayNodeId, converse_span: f64, ascent: f64, descent: f64) {
        {
            let n = self.nodes[node].as_mut().unwrap();
            n.converse_span = converse_span;
            n.ascent = ascent;
            n.descent = descent;
        }
        if let Some(Node { kind: Kind::Blank, el, .. }) = self.nodes[node].as_ref() {
            let (x_span, y_span) = self.convert.direction_unconvert_span(converse_span, ascent + descent);
            set_style(el, "width", &format!("{}px", x_span));
            set_style(el, "height", &format!("{}px", y_span));
        }
        self.fix_position(node, false);
        self.group_span_changed(node);
    }

    fn group_add(&mut self, group: DisplayNodeId, index: usize, child: DisplayNodeId) {
        self.nodes[group].as_mut().unwrap().children.insert(index, child);
        self.nodes[child].as_mut().unwrap().parent = Some(group);
        let (parent_el, child_el) =
            (self.nodes[group].as_ref().unwrap().el.clone(), self.nodes[child].as_ref().unwrap().el.clone());
        parent_el.ref_splice(index, 0, vec![child_el]);
        self.fix_position(child, false);
        self.group_span_changed(child);
    }

    fn group_remove(&mut self, group: DisplayNodeId, index: usize, count: usize) {
        let removed: Vec<DisplayNodeId> =
            self.nodes[group].as_mut().unwrap().children.drain(index .. index + count).collect();
        for child in removed {
            if let Some(n) = self.nodes[child].as_mut() {
                n.parent = None;
            }
        }
        let el = self.nodes[group].as_ref().unwrap().el.clone();
        el.ref_splice(index, count, vec![]);
        if let Some(first) = self.nodes[group].as_ref().unwrap().children.first().copied() {
            self.group_span_changed(first);
        }
    }

    fn group_remove_node(&mut self, group: DisplayNodeId, child: DisplayNodeId) {
        let Some(index) = self.nodes[group].as_ref().unwrap().children.iter().position(|n| *n == child) else {
            return;
        };
        self.group_remove(group, index, 1);
    }

    fn group_clear(&mut self, group: DisplayNodeId) {
        let count = self.nodes[group].as_ref().unwrap().children.len();
        if count == 0 {
            return;
        }
        self.group_remove(group, 0, count);
    }

    fn group_converse_span(&self, group: DisplayNodeId) -> f64 {
        return self.nodes[group].as_ref().map(|n| n.converse_span).unwrap_or(0.);
    }

    fn text_set(&mut self, node: DisplayNodeId, text: &str, font: &FontSpec, color: &str) {
        {
            let Some(n) = self.nodes[node].as_mut() else {
                return;
            };
            let Kind::Text(t) = &mut n.kind else {
                return;
            };
            let css = font.font_css();
            if t.text == text && t.font == css && t.color == color {
                return;
            }
            t.text = text.to_string();
            t.font = css;
            t.color = color.to_string();
        }
        let Some(n) = self.nodes[node].as_ref() else {
            return;
        };
        let Kind::Text(t) = &n.kind else {
            return;
        };
        if t.font.is_empty() {
            return;
        }
        let (text, font, color) = (t.text.clone(), t.font.clone(), t.color.clone());
        let el = n.el.clone();
        let measured = {
            let ctx = canvas_context(&self.measure);
            ctx.set_font(&font);
            self.measure_font = font.clone();
            ctx.measure_text(&text).unwrap()
        };
        let converse_span = measured.width();
        let ascent = measured.actual_bounding_box_ascent().max(0.);
        let descent = measured.actual_bounding_box_descent().max(0.);
        let pad_converse_half = converse_span / 4.;
        let pad_transverse_half = (ascent + descent) / 4.;
        let (x_span, y_span) =
            self
                .convert
                .direction_unconvert_span(
                    converse_span + pad_converse_half * 2.,
                    ascent + descent + pad_transverse_half * 2.,
                );
        let ctx = canvas_resize(&el, x_span, y_span);
        ctx.set_font(&font);
        ctx.set_fill_style_str(&color);
        let (x, y) =
            self.convert.direction_unconvert(pad_converse_half, pad_transverse_half + ascent, x_span, y_span);
        let _ = ctx.fill_text(&text, x, y);
        {
            let n = self.nodes[node].as_mut().unwrap();
            if let Kind::Text(t) = &mut n.kind {
                t.ink_ascent = ascent;
                t.ink_descent = descent;
                t.pad_converse_half = pad_converse_half;
                t.pad_transverse_half = pad_transverse_half;
            }
        }
        self.fix_position(node, false);
    }

    fn drawing_clear(&mut self, node: DisplayNodeId) {
        let Some(n) = self.nodes[node].as_ref() else {
            return;
        };
        let Kind::Drawing(size) = n.kind else {
            return;
        };
        let (x_span, y_span) = self.convert.direction_unconvert_span(size.converse, size.transverse);
        let ctx = canvas_context(&n.el);
        ctx.clear_rect(0., 0., x_span.ceil().max(1.), y_span.ceil().max(1.));
    }

    fn drawing_resize(&mut self, node: DisplayNodeId, size: Vector) {
        let (x_span, y_span) = self.convert.direction_unconvert_span(size.converse, size.transverse);
        let Some(n) = self.nodes[node].as_mut() else {
            return;
        };
        n.kind = Kind::Drawing(size);
        let el = n.el.clone();
        canvas_resize(&el, x_span, y_span);
        self.fix_position(node, false);
    }

    fn drawing_draw(&mut self, node: DisplayNodeId, commands: &[DrawCommand]) {
        let Some(n) = self.nodes[node].as_ref() else {
            return;
        };
        let ctx = canvas_context(&n.el);
        let convert = self.convert;
        let point = |v: Vector, offset: Vector, stroke: bool| -> (f64, f64) {
            let bias = if stroke {
                0.5
            } else {
                0.
            };
            let (x, y) =
                convert.direction_unconvert(v.converse + offset.converse, v.transverse + offset.transverse, 0., 0.);
            return (x + bias, y + bias);
        };
        let mut offset = Vector::default();
        let mut stroke = true;
        ctx.save();
        for command in commands {
            match command {
                DrawCommand::Translate(v) => offset = *v,
                DrawCommand::SetLineColor(c) => ctx.set_stroke_style_str(c),
                DrawCommand::SetLineThickness(t) => ctx.set_line_width(*t),
                DrawCommand::SetLineCapRound => ctx.set_line_cap("round"),
                DrawCommand::SetLineCapFlat => ctx.set_line_cap("butt"),
                DrawCommand::SetFillColor(c) => ctx.set_fill_style_str(c),
                DrawCommand::BeginStrokePath => {
                    stroke = true;
                    ctx.begin_path();
                },
                DrawCommand::BeginFillPath => {
                    stroke = false;
                    ctx.begin_path();
                },
                DrawCommand::MoveTo(v) => {
                    let to = point(*v, offset, stroke);
                    ctx.move_to(to.0, to.1);
                },
                DrawCommand::LineTo(v) => {
                    let to = point(*v, offset, stroke);
                    ctx.line_to(to.0, to.1);
                },
                DrawCommand::SplineTo { handle1, handle2, to } => {
                    let h1 = point(*handle1, offset, stroke);
                    let h2 = point(*handle2, offset, stroke);
                    let to = point(*to, offset, stroke);
                    ctx.bezier_curve_to(h1.0, h1.1, h2.0, h2.1, to.0, to.1);
                },
                DrawCommand::ArcTo { corner, to, radius } => {
                    let corner = point(*corner, offset, stroke);
                    let to = point(*to, offset, stroke);
                    if *radius <= 0. {
                        ctx.line_to(corner.0, corner.1);
                        continue;
                    }
                    let _ = ctx.arc_to(corner.0, corner.1, to.0, to.1, *radius);
                },
                DrawCommand::ClosePath => {
                    ctx.close_path();
                    if stroke {
                        ctx.stroke();
                    } else {
                        ctx.fill();
                    }
                },
            }
        }
        ctx.restore();
    }
}
