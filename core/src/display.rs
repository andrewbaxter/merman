use {
    crate::{
        context::Vector,
        measure::{
            FontMetrics,
            FontSpec,
        },
        spec::{
            SpecDisplayUnit,
            SpecObbox,
        },
    },
    unicode_segmentation::UnicodeSegmentation,
};

pub trait Display {
    fn display_blank(&mut self) -> DisplayNodeId;
    fn display_destroy(&mut self, node: DisplayNodeId);
    fn display_drawing(&mut self) -> DisplayNodeId;
    fn display_font_metrics(&mut self, font: &FontSpec) -> FontMetrics;
    fn display_font_width(&mut self, font: &FontSpec, text: &str) -> f64;
    fn display_group(&mut self) -> DisplayNodeId;
    fn display_image(&mut self) -> DisplayNodeId;

    fn display_index_at_converse(&mut self, font: &FontSpec, text: &str, converse: f64) -> usize {
        return crate::measure::measure_index_at_converse(self, font, text, converse);
    }
    fn display_layer(&mut self, layer: DisplayLayer) -> DisplayNodeId;
    fn display_root_add(&mut self, index: usize, node: DisplayNodeId);
    fn display_root_child_count(&self) -> usize;
    fn display_set_background(&mut self, color: &str);
    fn display_text(&mut self) -> DisplayNodeId;

    fn display_to_pixels(&self, unit: SpecDisplayUnit) -> f64 {
        return display_unit_to_pixels(unit);
    }
    fn drawing_clear(&mut self, node: DisplayNodeId);
    fn drawing_draw(&mut self, node: DisplayNodeId, commands: &[DrawCommand]);
    fn drawing_resize(&mut self, node: DisplayNodeId, size: Vector);
    fn group_add(&mut self, group: DisplayNodeId, index: usize, child: DisplayNodeId);
    fn group_clear(&mut self, group: DisplayNodeId);
    fn group_converse_span(&self, group: DisplayNodeId) -> f64;
    fn group_remove(&mut self, group: DisplayNodeId, index: usize, count: usize);
    fn group_remove_node(&mut self, group: DisplayNodeId, child: DisplayNodeId);
    fn image_set_rotate(&mut self, node: DisplayNodeId, rotate: f64);
    fn image_set_source(&mut self, node: DisplayNodeId, path: &str);
    fn node_set_baseline_transverse(&mut self, node: DisplayNodeId, baseline: f64, animate: bool);
    fn node_set_converse(&mut self, node: DisplayNodeId, converse: f64, animate: bool);
    fn node_set_position(&mut self, node: DisplayNodeId, converse: f64, transverse: f64, animate: bool);
    fn node_set_span(&mut self, node: DisplayNodeId, converse_span: f64, ascent: f64, descent: f64);
    fn node_set_transverse(&mut self, node: DisplayNodeId, transverse: f64, animate: bool);
    fn text_set(&mut self, node: DisplayNodeId, text: &str, font: &FontSpec, color: &str);
}

pub fn display_unit_to_pixels(unit: SpecDisplayUnit) -> f64 {
    match unit {
        SpecDisplayUnit::Px => return 1.,
        SpecDisplayUnit::Mm => return 96. / 25.4,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayLayer {
    Background,
    Overlay,
    Text,
}

pub type DisplayNodeId = usize;

#[derive(Clone, Default)]
pub struct DisplayTest(pub std::rc::Rc<std::cell::RefCell<DisplayTestState>>);

impl DisplayTest {
    pub fn display_test_drawings(&self) -> usize {
        let s = self.0.borrow();
        return s.nodes.iter().flatten().filter(|n| matches!(n.kind, TestKind::Drawing(true))).count();
    }

    pub fn display_test_drawn(&self) -> Vec<TestDrawing> {
        let s = self.0.borrow();
        let mut out = vec![];
        for node in s.nodes.iter().flatten() {
            if !matches!(node.kind, TestKind::Drawing(true)) {
                continue;
            }
            out.push(TestDrawing {
                converse: node.converse,
                transverse: node.transverse,
                size: node.size,
            });
        }
        return out;
    }

    pub fn display_test_rows(&self) -> Vec<TestRow> {
        let s = self.0.borrow();
        let mut out = vec![];
        for course in s.children(s.layers[1]) {
            let mut bricks = vec![];
            for b in s.children(course) {
                let node = s.nodes[b].as_ref().unwrap();
                if let TestKind::Text(text) = &node.kind {
                    bricks.push(TestBrick {
                        converse: node.converse,
                        text: text.clone(),
                    });
                }
            }
            out.push(TestRow {
                transverse: s.nodes[course].as_ref().unwrap().transverse,
                bricks: bricks,
            });
        }
        return out;
    }
}

impl Display for DisplayTest {
    fn display_blank(&mut self) -> DisplayNodeId {
        return self.0.borrow_mut().new_node(TestKind::Blank);
    }

    fn display_destroy(&mut self, node: DisplayNodeId) {
        self.0.borrow_mut().nodes[node] = None;
    }

    fn display_drawing(&mut self) -> DisplayNodeId {
        return self.0.borrow_mut().new_node(TestKind::Drawing(false));
    }

    fn display_font_metrics(&mut self, font: &FontSpec) -> FontMetrics {
        return FontMetrics {
            ascent: font.size * 0.8,
            descent: font.size * 0.2,
        };
    }

    fn display_font_width(&mut self, font: &FontSpec, text: &str) -> f64 {
        return text.graphemes(true).count() as f64 * font.size * 0.6;
    }

    fn display_group(&mut self) -> DisplayNodeId {
        return self.0.borrow_mut().new_node(TestKind::Group(vec![]));
    }

    fn display_image(&mut self) -> DisplayNodeId {
        return self.0.borrow_mut().new_node(TestKind::Blank);
    }

    fn display_layer(&mut self, _layer: DisplayLayer) -> DisplayNodeId {
        let mut s = self.0.borrow_mut();
        let id = s.new_node(TestKind::Group(vec![]));
        s.layers.push(id);
        return id;
    }

    fn display_root_add(&mut self, _index: usize, node: DisplayNodeId) {
        self.0.borrow_mut().layers.push(node);
    }

    fn display_root_child_count(&self) -> usize {
        return self.0.borrow().layers.len();
    }

    fn display_set_background(&mut self, _color: &str) { }

    fn display_text(&mut self) -> DisplayNodeId {
        return self.0.borrow_mut().new_node(TestKind::Text(String::new()));
    }

    fn drawing_clear(&mut self, node: DisplayNodeId) {
        let mut s = self.0.borrow_mut();
        s.nodes[node].as_mut().unwrap().kind = TestKind::Drawing(false);
    }

    fn drawing_draw(&mut self, node: DisplayNodeId, commands: &[DrawCommand]) {
        if commands.is_empty() {
            return;
        }
        let mut s = self.0.borrow_mut();
        s.nodes[node].as_mut().unwrap().kind = TestKind::Drawing(true);
    }

    fn drawing_resize(&mut self, node: DisplayNodeId, size: Vector) {
        let mut s = self.0.borrow_mut();
        s.nodes[node].as_mut().unwrap().size = size;
    }

    fn group_add(&mut self, group: DisplayNodeId, index: usize, child: DisplayNodeId) {
        let mut s = self.0.borrow_mut();
        match &mut s.nodes[group].as_mut().unwrap().kind {
            TestKind::Group(c) => c.insert(index, child),
            _ => panic!("node {} is not a group", group),
        }
    }

    fn group_clear(&mut self, group: DisplayNodeId) {
        let mut s = self.0.borrow_mut();
        match &mut s.nodes[group].as_mut().unwrap().kind {
            TestKind::Group(c) => c.clear(),
            _ => panic!("node {} is not a group", group),
        }
    }

    fn group_converse_span(&self, group: DisplayNodeId) -> f64 {
        let s = self.0.borrow();
        let mut out: f64 = 0.;
        for child in s.children(group) {
            let node = s.nodes[child].as_ref().unwrap();
            out = out.max(match &node.kind {
                TestKind::Group(_) => 0.,
                _ => node.converse + node.converse_span,
            });
        }
        return out;
    }

    fn group_remove(&mut self, group: DisplayNodeId, index: usize, count: usize) {
        let mut s = self.0.borrow_mut();
        match &mut s.nodes[group].as_mut().unwrap().kind {
            TestKind::Group(c) => {
                c.drain(index .. index + count);
            },
            _ => panic!("node {} is not a group", group),
        }
    }

    fn group_remove_node(&mut self, group: DisplayNodeId, child: DisplayNodeId) {
        let mut s = self.0.borrow_mut();
        match &mut s.nodes[group].as_mut().unwrap().kind {
            TestKind::Group(c) => c.retain(|n| *n != child),
            _ => panic!("node {} is not a group", group),
        }
    }

    fn image_set_rotate(&mut self, _node: DisplayNodeId, _rotate: f64) { }

    fn image_set_source(&mut self, _node: DisplayNodeId, _path: &str) { }

    fn node_set_baseline_transverse(&mut self, node: DisplayNodeId, baseline: f64, _animate: bool) {
        self.0.borrow_mut().nodes[node].as_mut().unwrap().transverse = baseline;
    }

    fn node_set_converse(&mut self, node: DisplayNodeId, converse: f64, _animate: bool) {
        self.0.borrow_mut().nodes[node].as_mut().unwrap().converse = converse;
    }

    fn node_set_position(&mut self, node: DisplayNodeId, converse: f64, transverse: f64, _animate: bool) {
        let mut s = self.0.borrow_mut();
        let n = s.nodes[node].as_mut().unwrap();
        n.converse = converse;
        n.transverse = transverse;
    }

    fn node_set_span(&mut self, node: DisplayNodeId, converse_span: f64, _ascent: f64, _descent: f64) {
        self.0.borrow_mut().nodes[node].as_mut().unwrap().converse_span = converse_span;
    }

    fn node_set_transverse(&mut self, node: DisplayNodeId, transverse: f64, _animate: bool) {
        self.0.borrow_mut().nodes[node].as_mut().unwrap().transverse = transverse;
    }

    fn text_set(&mut self, node: DisplayNodeId, text: &str, _font: &FontSpec, _color: &str) {
        let mut s = self.0.borrow_mut();
        s.nodes[node].as_mut().unwrap().kind = TestKind::Text(text.to_string());
    }
}

#[derive(Default)]
pub struct DisplayTestState {
    layers: Vec<DisplayNodeId>,
    nodes: Vec<Option<TestNode>>,
}

impl DisplayTestState {
    fn children(&self, group: DisplayNodeId) -> Vec<DisplayNodeId> {
        match &self.nodes[group].as_ref().unwrap().kind {
            TestKind::Group(c) => return c.clone(),
            _ => panic!("node {} is not a group", group),
        }
    }

    fn new_node(&mut self, kind: TestKind) -> DisplayNodeId {
        let id = self.nodes.len();
        self.nodes.push(Some(TestNode {
            kind: kind,
            converse: 0.,
            converse_span: 0.,
            transverse: 0.,
            size: Vector::default(),
        }));
        return id;
    }
}

#[derive(Debug, Clone)]
pub enum DrawCommand {
    ArcTo {
        corner: Vector,
        to: Vector,
        radius: f64,
    },
    BeginFillPath,
    BeginStrokePath,
    ClosePath,
    LineTo(Vector),
    MoveTo(Vector),
    SetFillColor(String),
    SetLineCapFlat,
    SetLineCapRound,
    SetLineColor(String),
    SetLineThickness(f64),
    SplineTo {
        handle1: Vector,
        handle2: Vector,
        to: Vector,
    },
    Translate(Vector),
}

pub fn obbox_commands(points: &[(Vector, bool)], style: &SpecObbox) -> Vec<DrawCommand> {
    let mut out = vec![];
    if style.fill {
        out.push(DrawCommand::BeginFillPath);
        out.push(DrawCommand::SetFillColor(style.fill_color.clone()));
        obbox_path(points, style, &mut out);
        out.push(DrawCommand::ClosePath);
    }
    if style.line {
        out.push(DrawCommand::BeginStrokePath);
        out.push(DrawCommand::SetLineColor(style.line_color.clone()));
        out.push(DrawCommand::SetLineThickness(style.line_thickness));
        obbox_path(points, style, &mut out);
        out.push(DrawCommand::ClosePath);
    }
    return out;
}

pub fn obbox_path(points: &[(Vector, bool)], style: &SpecObbox, out: &mut Vec<DrawCommand>) {
    let base_radius = style.round_radius;
    let n = points.len();
    for i in 0 .. n {
        let (mid, round) = points[i];
        if round {
            let (pre, _) = points[(i + n - 1) % n];
            let (post, _) = points[(i + 1) % n];
            let to_pre = Vector::new(pre.converse - mid.converse, pre.transverse - mid.transverse);
            let to_post = Vector::new(post.converse - mid.converse, post.transverse - mid.transverse);
            let mut radius = base_radius;
            radius = radius.min(f64::max(to_pre.converse.abs(), to_pre.transverse.abs()) / 2.);
            radius = radius.min(f64::max(to_post.converse.abs(), to_post.transverse.abs()) / 2.);
            if i == 0 {
                out.push(
                    DrawCommand::MoveTo(
                        Vector::new(
                            mid.converse + simple_norm(to_pre.converse) * radius,
                            mid.transverse + simple_norm(to_pre.transverse) * radius,
                        ),
                    ),
                );
            }
            out.push(DrawCommand::ArcTo {
                corner: mid,
                to: Vector::new(
                    mid.converse + simple_norm(to_post.converse) * radius,
                    mid.transverse + simple_norm(to_post.transverse) * radius,
                ),
                radius: radius,
            });
        } else if i == 0 {
            out.push(DrawCommand::MoveTo(mid));
        } else {
            out.push(DrawCommand::LineTo(mid));
        }
    }
}

fn simple_norm(v: f64) -> f64 {
    if (v * v) < 0.1 * 0.1 {
        return 0.;
    }
    if v < 0. {
        return -1.;
    }
    return 1.;
}

pub struct TestBrick {
    pub converse: f64,
    pub text: String,
}

pub struct TestDrawing {
    pub converse: f64,
    pub size: Vector,
    pub transverse: f64,
}

enum TestKind {
    Blank,
    Drawing(bool),
    Group(Vec<DisplayNodeId>),
    Text(String),
}

struct TestNode {
    converse: f64,
    converse_span: f64,
    kind: TestKind,
    size: Vector,
    transverse: f64,
}

pub struct TestRow {
    pub bricks: Vec<TestBrick>,
    pub transverse: f64,
}
