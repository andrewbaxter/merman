use gloo_utils::document;
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
use merman3_core::spec::SpecDirection;
use rooting::{
    el_from_raw,
    El,
};
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use web_sys::SvgTextContentElement;

pub fn svg_el(tag: &str) -> El {
    return el_from_raw(document().create_element_ns(Some("http://www.w3.org/2000/svg"), tag).unwrap());
}

enum Kind {
    Group,
    Text,
    Blank,
    Drawing,
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
        }));
        return id;
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

    fn place(&mut self, node: DisplayNodeId) {
        let Some(n) = self.nodes[node].as_ref() else {
            return;
        };
        if !matches!(n.kind, Kind::Text) {
            return;
        }
        let (converse, span, ascent, descent) = (n.converse, n.converse_span, n.ascent, n.descent);
        let transverse = n.baseline - ascent;
        let height = ascent + descent;
        let el = n.el.clone();
        let convert = self.convert;
        if convert.direction_converse_vertical() {
            let (x, y) = convert.direction_unconvert(converse, transverse, height, span);
            let center_x = x + height / 2.;
            let mode = match convert.transverse {
                SpecDirection::Left => "vertical-rl",
                _ => "vertical-lr",
            };
            el
                .ref_attr("x", &format!("{}", center_x))
                .ref_attr("y", &format!("{}", y))
                .ref_attr("dominant-baseline", "central")
                .ref_attr("style", &format!("writing-mode: {}", mode));
            if convert.converse == SpecDirection::Up {
                el.ref_attr("transform", &format!("rotate(180 {} {})", center_x, y + span / 2.));
            }
        } else {
            let (x, y) = convert.direction_unconvert(converse, transverse, span, height);
            el.ref_attr("x", &format!("{}", x)).ref_attr("y", &format!("{}", y + ascent));
        }
    }

    fn translate(&mut self, node: DisplayNodeId, animate: bool) {
        if self.nodes[node].as_ref().map(|n| n.animated) != Some(animate) {
            if let Some(n) = self.nodes[node].as_mut() {
                n.animated = animate;
            }
            if let Some(n) = self.nodes[node].as_ref() {
                if animate {
                    n.el.ref_attr("style", "transition: transform 0.1s linear");
                } else {
                    n.el.ref_remove_attr("style");
                }
            }
        }
        let Some(n) = self.nodes[node].as_ref() else {
            return;
        };
        if matches!(n.kind, Kind::Text) {
            return;
        }
        let convert = self.convert;
        let converse = match convert.converse {
            SpecDirection::Right | SpecDirection::Down => n.converse,
            SpecDirection::Left | SpecDirection::Up => -n.converse,
        };
        let transverse = match convert.transverse {
            SpecDirection::Right | SpecDirection::Down => n.transverse,
            SpecDirection::Left | SpecDirection::Up => -n.transverse,
        };
        let (x, y) = if convert.direction_converse_vertical() {
            (transverse, converse)
        } else {
            (converse, transverse)
        };
        n.el.ref_attr("transform", &format!("translate({} {})", x, y));
    }

    fn point(&self, v: Vector) -> (f64, f64) {
        return self.convert.direction_unconvert(v.converse, v.transverse, 0., 0.);
    }
}

impl Display for DisplayWeb {
    fn display_font_width(&mut self, font: &FontSpec, text: &str) -> f64 {
        let css = font.font_css();
        let key = (css, text.to_string());
        if let Some(w) = self.widths.get(&key) {
            return *w;
        }
        let el = self.measure.clone();
        if self.measure_font != key.0 {
            el.ref_attr("font-family", &font.family).ref_attr("font-size", &format!("{}", font.size));
            self.measure_font = key.0.clone();
        }
        el.ref_text(text);
        let measured: SvgTextContentElement = el.raw().dyn_into().unwrap();
        let w = measured.get_computed_text_length() as f64;
        self.widths.insert(key, w);
        return w;
    }

    fn display_font_metrics(&mut self, font: &FontSpec) -> FontMetrics {
        let css = font.font_css();
        if let Some(m) = self.metrics.get(&css) {
            return *m;
        }
        let el = self.measure.clone();
        el.ref_attr("font-family", &font.family).ref_attr("font-size", &format!("{}", font.size));
        self.measure_font = css.clone();
        el.ref_text("W");
        let measured: SvgTextContentElement = el.raw().dyn_into().unwrap();
        let bbox = measured.get_b_box().unwrap();
        let out = FontMetrics {
            ascent: -bbox.y() as f64,
            descent: (bbox.y() + bbox.height()) as f64,
        };
        self.metrics.insert(css, out);
        return out;
    }

    fn display_layer(&mut self, _layer: DisplayLayer) -> DisplayNodeId {
        let node = self.new_node(svg_el("g"), Kind::Group);
        let at = self.display_root_child_count();
        self.display_root_add(at, node);
        return node;
    }

    fn display_group(&mut self) -> DisplayNodeId {
        return self.new_node(svg_el("g"), Kind::Group);
    }

    fn display_text(&mut self) -> DisplayNodeId {
        return self.new_node(svg_el("text"), Kind::Text);
    }

    fn display_blank(&mut self) -> DisplayNodeId {
        return self.new_node(svg_el("g"), Kind::Blank);
    }

    fn display_image(&mut self) -> DisplayNodeId {
        return self.new_node(svg_el("image"), Kind::Blank);
    }

    fn image_set_source(&mut self, node: DisplayNodeId, path: &str) {
        self.nodes[node].as_ref().unwrap().el.ref_attr("href", path);
    }

    fn image_set_rotate(&mut self, node: DisplayNodeId, rotate: f64) {
        self.nodes[node].as_ref().unwrap().el.ref_attr("transform", &format!("rotate({})", rotate));
    }

    fn display_drawing(&mut self) -> DisplayNodeId {
        return self.new_node(svg_el("g"), Kind::Drawing);
    }

    fn display_destroy(&mut self, node: DisplayNodeId) {
        self.nodes[node] = None;
    }

    fn display_set_background(&mut self, color: &str) {
        self.background.ref_attr("style", &format!("background: {}", color));
    }

    fn display_root_add(&mut self, index: usize, node: DisplayNodeId) {
        let el = self.nodes[node].as_ref().unwrap().el.clone();
        self.root.ref_splice(index, 0, vec![el]);
    }

    fn display_root_child_count(&self) -> usize {
        return self.root.raw().child_element_count() as usize;
    }

    fn node_set_converse(&mut self, node: DisplayNodeId, converse: f64, _animate: bool) {
        self.nodes[node].as_mut().unwrap().converse = converse;
        self.place(node);
        self.group_span_changed(node);
    }

    fn node_set_transverse(&mut self, node: DisplayNodeId, transverse: f64, animate: bool) {
        self.nodes[node].as_mut().unwrap().transverse = transverse;
        self.translate(node, animate);
    }

    fn node_set_baseline_transverse(&mut self, node: DisplayNodeId, baseline: f64, _animate: bool) {
        self.nodes[node].as_mut().unwrap().baseline = baseline;
        self.place(node);
    }

    fn node_set_position(&mut self, node: DisplayNodeId, converse: f64, transverse: f64, animate: bool) {
        {
            let n = self.nodes[node].as_mut().unwrap();
            n.converse = converse;
            n.transverse = transverse;
        }
        self.translate(node, animate);
    }

    fn node_set_span(&mut self, node: DisplayNodeId, converse_span: f64, ascent: f64, descent: f64) {
        {
            let n = self.nodes[node].as_mut().unwrap();
            n.converse_span = converse_span;
            n.ascent = ascent;
            n.descent = descent;
        }
        self.place(node);
        self.group_span_changed(node);
    }

    fn group_add(&mut self, group: DisplayNodeId, index: usize, child: DisplayNodeId) {
        self.nodes[group].as_mut().unwrap().children.insert(index, child);
        self.nodes[child].as_mut().unwrap().parent = Some(group);
        let (parent_el, child_el) =
            (self.nodes[group].as_ref().unwrap().el.clone(), self.nodes[child].as_ref().unwrap().el.clone());
        parent_el.ref_splice(index, 0, vec![child_el]);
        self.place(child);
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
        let el = self.nodes[node].as_ref().unwrap().el.clone();
        el
            .ref_attr("font-family", &font.family)
            .ref_attr("font-size", &format!("{}", font.size))
            .ref_attr("fill", color)
            .ref_text(text);
    }

    fn drawing_clear(&mut self, node: DisplayNodeId) {
        self.nodes[node].as_ref().unwrap().el.ref_clear();
    }

    fn drawing_resize(&mut self, _node: DisplayNodeId, _size: Vector) { }

    fn drawing_draw(&mut self, node: DisplayNodeId, commands: &[DrawCommand]) {
        let mut offset = Vector::default();
        let mut line_color = String::from("none");
        let mut fill_color = String::from("none");
        let mut thickness = 1.;
        let mut cap = "butt";
        let mut fill = false;
        let mut data = String::new();
        let mut current = (0., 0.);
        let mut els = vec![];
        let flush =
            |
                data: &mut String,
                fill: bool,
                fill_color: &str,
                line_color: &str,
                thickness: f64,
                cap: &str,
                els: &mut Vec<El>,
            | {
                if data.is_empty() {
                    return;
                }
                let path_el = svg_el("path");
                path_el.ref_attr("d", data).ref_attr("fill", if fill {
                    fill_color
                } else {
                    "none"
                }).ref_attr("stroke", if fill {
                    "none"
                } else {
                    line_color
                }).ref_attr("stroke-width", &format!("{}", thickness)).ref_attr("stroke-linecap", cap);
                els.push(path_el);
                data.clear();
            };
        for command in commands {
            match command {
                DrawCommand::Translate(v) => offset = *v,
                DrawCommand::SetLineColor(c) => line_color = c.clone(),
                DrawCommand::SetLineThickness(t) => thickness = *t,
                DrawCommand::SetLineCapRound => cap = "round",
                DrawCommand::SetLineCapFlat => cap = "butt",
                DrawCommand::SetFillColor(c) => fill_color = c.clone(),
                DrawCommand::BeginFillPath => {
                    flush(&mut data, fill, &fill_color, &line_color, thickness, cap, &mut els);
                    fill = true;
                },
                DrawCommand::BeginStrokePath => {
                    flush(&mut data, fill, &fill_color, &line_color, thickness, cap, &mut els);
                    fill = false;
                },
                DrawCommand::MoveTo(p) => {
                    current =
                        self.point(Vector::new(p.converse + offset.converse, p.transverse + offset.transverse));
                    data.push_str(&format!("M {} {} ", current.0, current.1));
                },
                DrawCommand::LineTo(p) => {
                    current =
                        self.point(Vector::new(p.converse + offset.converse, p.transverse + offset.transverse));
                    data.push_str(&format!("L {} {} ", current.0, current.1));
                },
                DrawCommand::SplineTo { handle1, handle2, to } => {
                    let h1 =
                        self.point(
                            Vector::new(handle1.converse + offset.converse, handle1.transverse + offset.transverse),
                        );
                    let h2 =
                        self.point(
                            Vector::new(handle2.converse + offset.converse, handle2.transverse + offset.transverse),
                        );
                    current =
                        self.point(Vector::new(to.converse + offset.converse, to.transverse + offset.transverse));
                    data.push_str(&format!("C {} {} {} {} {} {} ", h1.0, h1.1, h2.0, h2.1, current.0, current.1));
                },
                DrawCommand::ArcTo { corner, to, radius } => {
                    let corner =
                        self.point(
                            Vector::new(corner.converse + offset.converse, corner.transverse + offset.transverse),
                        );
                    let to =
                        self.point(Vector::new(to.converse + offset.converse, to.transverse + offset.transverse));
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
                    flush(&mut data, fill, &fill_color, &line_color, thickness, cap, &mut els);
                },
            }
        }
        flush(&mut data, fill, &fill_color, &line_color, thickness, cap, &mut els);
        let el = self.nodes[node].as_ref().unwrap().el.clone();
        el.ref_extend(els);
    }
}
