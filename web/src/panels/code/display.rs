use {
    gloo_utils::document,
    merman_core::{
        context::Vector,
        direction::DirectionConvert,
        display::{
            Display,
            DisplayLayer,
            DisplayNodeId,
            DrawCommand,
        },
        measure::{
            FontMetrics,
            FontSpec,
        },
        spec::SpecDirection,
    },
    rooting::{
        El,
        el,
        el_from_raw,
    },
    std::{
        collections::HashMap,
        rc::Rc,
    },
    wasm_bindgen::JsCast,
    web_sys::{
        CanvasRenderingContext2d,
        HtmlCanvasElement,
        TextMetrics,
    },
};

pub fn display_el(class: &str) -> El {
    return el("div").classes(&["merman_display", class]);
}

pub struct DisplayWeb {
    pub background: El,
    pub convert: DirectionConvert,
    pub details: Rc<dyn Fn() -> El>,
    pub measure: El,
    pub measure_font: String,
    pub metrics: HashMap<String, FontMetrics>,
    pub nodes: Vec<Option<Node>>,
    pub root: El,
    pub widths: HashMap<(String, String), f64>,
}

impl DisplayWeb {
    fn fix_position(&mut self, node: DisplayNodeId, animate: bool) {
        let Some(n) = self.nodes[node].as_ref() else {
            return;
        };
        let (corner, span) = match &n.kind {
            Kind::Text(_) => (
                Vector::new(n.converse, n.baseline - n.ascent),
                Vector::new(n.converse_span, n.ascent + n.descent),
            ),
            Kind::Blank => (Vector::new(n.converse, 0.), Vector::new(n.converse_span, n.ascent + n.descent)),
            Kind::Drawing(size) => (Vector::new(n.converse, n.transverse), *size),
            Kind::Details | Kind::Group | Kind::Image => (
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

    fn group_span_changed(&mut self, node: DisplayNodeId) {
        let mut at = node;
        loop {
            let Some(n) = self.nodes[at].as_ref() else {
                return;
            };
            let Some(group) = n.parent else {
                return;
            };
            let extent = match n.kind {
                Kind::Group => n.converse_span,
                _ => n.converse + n.converse_span,
            };
            let old_extent = n.extent;
            self.nodes[at].as_mut().unwrap().extent = extent;
            let group_span = self.nodes[group].as_ref().unwrap().converse_span;
            let span = if extent >= group_span {
                extent
            } else if old_extent >= group_span {
                let mut span: f64 = 0.;
                for child in &self.nodes[group].as_ref().unwrap().children {
                    if let Some(c) = self.nodes[*child].as_ref() {
                        span = span.max(c.extent);
                    }
                }
                span
            } else {
                return;
            };
            let g = self.nodes[group].as_mut().unwrap();
            if g.converse_span == span {
                return;
            }
            g.converse_span = span;
            at = group;
        }
    }

    fn measure_context(&mut self, font: &FontSpec) -> CanvasRenderingContext2d {
        let canvas: HtmlCanvasElement = self.measure.raw().dyn_into().unwrap();
        let ctx: CanvasRenderingContext2d = canvas.get_context("2d").unwrap().unwrap().dyn_into().unwrap();
        let css = font.font_css();
        if self.measure_font != css {
            ctx.set_font(&css);
            self.measure_font = css;
        }
        return ctx;
    }

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
            extent: 0.,
        }));
        return id;
    }
}

impl Display for DisplayWeb {
    fn display_blank(&mut self) -> DisplayNodeId {
        return self.new_node(display_el("merman_display_blank"), Kind::Blank);
    }

    fn display_details(&mut self) -> (DisplayNodeId, f64) {
        let element = display_el("merman_display_details").push((self.details)());
        let raw = element.raw();
        _ = self.root.raw().append_child(&raw);
        let rect = raw.get_bounding_client_rect();
        raw.remove();
        let (converse_span, transverse_span) = self.convert.direction_convert_span(rect.width(), rect.height());
        let node = self.new_node(element, Kind::Details);
        let n = self.nodes[node].as_mut().unwrap();
        n.converse_span = converse_span;
        n.ascent = transverse_span;
        return (node, transverse_span);
    }

    fn display_destroy(&mut self, node: DisplayNodeId) {
        self.nodes[node] = None;
    }

    fn display_drawing(&mut self) -> DisplayNodeId {
        return self.new_node(
            svg_el("svg").classes(&["merman_display", "merman_display_drawing"]),
            Kind::Drawing(Vector::default()),
        );
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

    fn display_group(&mut self) -> DisplayNodeId {
        return self.new_node(display_el("merman_display_group"), Kind::Group);
    }

    fn display_image(&mut self) -> DisplayNodeId {
        return self.new_node(el("img").classes(&["merman_display", "merman_display_image"]), Kind::Image);
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

    fn display_root_add(&mut self, index: usize, node: DisplayNodeId) {
        let el = self.nodes[node].as_ref().unwrap().el.clone();
        self.root.ref_splice(index, 0, vec![el]);
    }

    fn display_root_child_count(&self) -> usize {
        return self.root.raw().child_element_count() as usize;
    }

    fn display_set_background(&mut self, color: &str) {
        set_style(&self.background, "background", color);
    }

    fn display_text(&mut self) -> DisplayNodeId {
        let inner = svg_el("text");
        let outer = svg_el("svg").classes(&["merman_display", "merman_display_text"]).push(inner.clone());
        return self.new_node(outer, Kind::Text(Text {
            inner: inner,
            text: String::new(),
            font: String::new(),
            color: String::new(),
        }));
    }

    fn drawing_clear(&mut self, node: DisplayNodeId) {
        if let Some(n) = self.nodes[node].as_ref() {
            n.el.ref_clear();
        }
    }

    fn drawing_draw(&mut self, node: DisplayNodeId, commands: &[DrawCommand]) {
        let Some(n) = self.nodes[node].as_ref() else {
            return;
        };
        let el = n.el.clone();
        let convert = self.convert;
        let mut offset = Vector::default();
        let mut line_color = String::from("none");
        let mut fill_color = String::from("none");
        let mut thickness: f64 = 1.;
        let mut cap = "butt";
        let mut fill = false;
        let mut data = String::new();
        let mut current = (0., 0.);
        let mut paths = vec![];
        let flush =
            |
                data: &mut String,
                fill: bool,
                fill_color: &str,
                line_color: &str,
                thickness: f64,
                cap: &str,
                paths: &mut Vec<El>,
            | {
                if data.is_empty() {
                    return;
                }
                let (fill, stroke) = if fill {
                    (fill_color, "none")
                } else {
                    ("none", line_color)
                };
                paths.push(
                    svg_el("path")
                        .attr("d", data)
                        .attr("fill", fill)
                        .attr("stroke", stroke)
                        .attr("stroke-width", &format!("{}", thickness))
                        .attr("stroke-linecap", cap),
                );
                data.clear();
            };
        let point = |v: Vector, offset: Vector, centered: bool| -> (f64, f64) {
            if !centered {
                return convert.direction_unconvert(
                    v.converse + offset.converse,
                    v.transverse + offset.transverse,
                    0.,
                    0.,
                );
            }
            let (x, y) =
                convert.direction_unconvert(v.converse + offset.converse, v.transverse + offset.transverse, 1., 1.);
            return (x + 0.5, y + 0.5);
        };
        for command in commands {
            let centered = !fill && thickness.round() % 2. == 1.;
            match command {
                DrawCommand::Translate(v) => offset = *v,
                DrawCommand::SetLineColor(c) => line_color = c.clone(),
                DrawCommand::SetLineThickness(t) => thickness = *t,
                DrawCommand::SetLineCapRound => cap = "round",
                DrawCommand::SetLineCapFlat => cap = "butt",
                DrawCommand::SetFillColor(c) => fill_color = c.clone(),
                DrawCommand::BeginStrokePath => {
                    flush(&mut data, fill, &fill_color, &line_color, thickness, cap, &mut paths);
                    fill = false;
                },
                DrawCommand::BeginFillPath => {
                    flush(&mut data, fill, &fill_color, &line_color, thickness, cap, &mut paths);
                    fill = true;
                },
                DrawCommand::MoveTo(v) => {
                    current = point(*v, offset, centered);
                    data.push_str(&format!("M {} {} ", current.0, current.1));
                },
                DrawCommand::LineTo(v) => {
                    current = point(*v, offset, centered);
                    data.push_str(&format!("L {} {} ", current.0, current.1));
                },
                DrawCommand::SplineTo { handle1, handle2, to } => {
                    let h1 = point(*handle1, offset, centered);
                    let h2 = point(*handle2, offset, centered);
                    current = point(*to, offset, centered);
                    data.push_str(&format!("C {} {} {} {} {} {} ", h1.0, h1.1, h2.0, h2.1, current.0, current.1));
                },
                DrawCommand::ArcTo { corner, to, radius } => {
                    let corner = point(*corner, offset, centered);
                    let to = point(*to, offset, centered);
                    if *radius <= 0. {
                        data.push_str(&format!("L {} {} ", corner.0, corner.1));
                        current = corner;
                        continue;
                    }
                    let d0 = (current.0 - corner.0, current.1 - corner.1);
                    let len0 = (d0.0 * d0.0 + d0.1 * d0.1).sqrt();
                    let t1 = if len0 == 0. {
                        corner
                    } else {
                        (corner.0 + d0.0 / len0 * radius, corner.1 + d0.1 / len0 * radius)
                    };
                    let cross = (t1.0 - corner.0) * (to.1 - corner.1) - (t1.1 - corner.1) * (to.0 - corner.0);
                    let sweep = if cross < 0. {
                        1
                    } else {
                        0
                    };
                    data.push_str(
                        &format!("L {} {} A {} {} 0 0 {} {} {} ", t1.0, t1.1, radius, radius, sweep, to.0, to.1),
                    );
                    current = to;
                },
                DrawCommand::ClosePath => {
                    data.push('Z');
                    flush(&mut data, fill, &fill_color, &line_color, thickness, cap, &mut paths);
                },
            }
        }
        flush(&mut data, fill, &fill_color, &line_color, thickness, cap, &mut paths);
        el.ref_extend(paths);
    }

    fn drawing_resize(&mut self, node: DisplayNodeId, size: Vector) {
        let Some(n) = self.nodes[node].as_mut() else {
            return;
        };
        n.kind = Kind::Drawing(size);
        self.fix_position(node, false);
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
        let mut span: f64 = 0.;
        for child in &self.nodes[group].as_ref().unwrap().children {
            if let Some(c) = self.nodes[*child].as_ref() {
                span = span.max(c.extent);
            }
        }
        let g = self.nodes[group].as_mut().unwrap();
        if g.converse_span != span {
            g.converse_span = span;
            self.group_span_changed(group);
        }
    }

    fn group_remove_node(&mut self, group: DisplayNodeId, child: DisplayNodeId) {
        let Some(index) = self.nodes[group].as_ref().unwrap().children.iter().position(|n| *n == child) else {
            return;
        };
        self.group_remove(group, index, 1);
    }

    fn image_set_rotate(&mut self, node: DisplayNodeId, rotate: f64) {
        set_style(&self.nodes[node].as_ref().unwrap().el, "transform", &format!("rotate({}deg)", rotate));
    }

    fn image_set_source(&mut self, node: DisplayNodeId, path: &str) {
        self.nodes[node].as_ref().unwrap().el.ref_attr("src", path);
    }

    fn node_set_baseline_transverse(&mut self, node: DisplayNodeId, baseline: f64, animate: bool) {
        self.nodes[node].as_mut().unwrap().baseline = baseline;
        self.fix_position(node, animate);
    }

    fn node_set_converse(&mut self, node: DisplayNodeId, converse: f64, animate: bool) {
        self.nodes[node].as_mut().unwrap().converse = converse;
        self.fix_position(node, animate);
        self.group_span_changed(node);
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
        match self.nodes[node].as_ref() {
            Some(Node { kind: Kind::Blank | Kind::Details, el, .. }) => {
                let (x_span, y_span) = self.convert.direction_unconvert_span(converse_span, ascent + descent);
                set_style(el, "width", &format!("{}px", x_span));
                set_style(el, "height", &format!("{}px", y_span));
            },
            Some(Node { kind: Kind::Text(t), .. }) => {
                let convert = self.convert;
                let height = ascent + descent;
                if convert.direction_converse_vertical() {
                    let center_x = height / 2.;
                    let mode = match convert.transverse {
                        SpecDirection::Left => "vertical-rl",
                        _ => "vertical-lr",
                    };
                    t
                        .inner
                        .ref_attr("x", &format!("{}", center_x))
                        .ref_attr("y", "0")
                        .ref_attr("dominant-baseline", "central")
                        .ref_attr("style", &format!("writing-mode: {}", mode));
                    if convert.converse == SpecDirection::Up {
                        t.inner.ref_attr("transform", &format!("rotate(180 {} {})", center_x, converse_span / 2.));
                    }
                } else {
                    t.inner.ref_attr("x", "0").ref_attr("y", &format!("{}", ascent));
                }
            },
            _ => { },
        }
        self.fix_position(node, false);
        self.group_span_changed(node);
    }

    fn node_set_transverse(&mut self, node: DisplayNodeId, transverse: f64, animate: bool) {
        self.nodes[node].as_mut().unwrap().transverse = transverse;
        self.fix_position(node, animate);
    }

    fn text_set(&mut self, node: DisplayNodeId, text: &str, font: &FontSpec, color: &str) {
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
        t
            .inner
            .ref_attr("font-family", &font.family)
            .ref_attr("font-size", &format!("{}", font.size))
            .ref_attr("fill", color)
            .ref_text(text);
    }
}

enum Kind {
    Blank,
    Details,
    Drawing(Vector),
    Group,
    Image,
    Text(Text),
}

pub struct Node {
    animated: bool,
    ascent: f64,
    baseline: f64,
    children: Vec<DisplayNodeId>,
    converse: f64,
    converse_span: f64,
    descent: f64,
    el: El,
    extent: f64,
    kind: Kind,
    parent: Option<DisplayNodeId>,
    placed: (f64, f64),
    transverse: f64,
}

fn set_style(el: &El, key: &str, value: &str) {
    let raw = el.raw();
    let style = match raw.dyn_ref::<web_sys::HtmlElement>() {
        Some(h) => h.style(),
        None => raw.dyn_ref::<web_sys::SvgElement>().unwrap().style(),
    };
    let _ = style.set_property(key, value);
}

fn svg_el(tag: &str) -> El {
    return el_from_raw(document().create_element_ns(Some("http://www.w3.org/2000/svg"), tag).unwrap());
}

pub struct Text {
    color: String,
    font: String,
    inner: El,
    text: String,
}
