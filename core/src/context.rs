use crate::alignment::Alignment;
use crate::attachment::{
    Border,
    Caret,
    Drawing,
    TextBorder,
};
use crate::cursor::{
    Cursor,
    DragSelect,
    Hoverable,
};
use crate::environment::Environment;
use crate::display::{
    Display,
    DisplayLayer,
    DisplayNodeId,
};
use crate::document::{
    AtomId,
    Document,
};
use crate::iteration::{
    QueueEntry,
    Task,
};
use crate::keys::{
    KeyStroke,
    Keymap,
};
use crate::stylist::{
    Stylist,
    StylistDirect,
};
use crate::syntax::Syntax;
use crate::visual::Visual;
use crate::wall::{
    Brick,
    Course,
    Wall,
};
use std::collections::BinaryHeap;
use std::rc::Rc;

pub type VisualId = usize;
pub type BrickId = usize;
pub type CourseId = usize;
pub type AlignId = usize;
pub type TaskId = usize;
pub type BorderId = usize;
pub type TextBorderId = usize;
pub type CaretId = usize;
pub type DrawingId = usize;
pub type CursorId = usize;
pub type HoverableId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector {
    pub converse: f64,
    pub transverse: f64,
}

impl Vector {
    pub fn new(converse: f64, transverse: f64) -> Vector {
        return Vector {
            converse: converse,
            transverse: transverse,
        };
    }
}

pub struct ContextConfig {
    pub lay_brick_batch_size: usize,
    pub retry_expand_factor: f64,
    pub ellipsize_threshold: i64,
    pub animate_course_placement: bool,
    pub start_windowed: bool,
    pub scroll_factor: f64,
    pub scroll_alot_factor: f64,
    pub keys: Keymap,
}

impl Default for ContextConfig {
    fn default() -> Self {
        return ContextConfig {
            lay_brick_batch_size: 10,
            retry_expand_factor: 1.25,
            ellipsize_threshold: i64::MAX,
            animate_course_placement: false,
            start_windowed: false,
            scroll_factor: 0.1,
            scroll_alot_factor: 0.8,
            keys: Keymap::default(),
        };
    }
}

pub struct Context {
    pub syntax: Rc<Syntax>,
    pub document: Rc<Document>,
    pub config: ContextConfig,
    pub display: Box<dyn Display>,
    pub environment: Box<dyn Environment>,
    pub stylist: Rc<dyn Stylist>,
    pub to_pixels: f64,
    pub from_pixels_to_mm: f64,
    pub background_layer: DisplayNodeId,
    pub text_layer: DisplayNodeId,
    pub overlay_layer: DisplayNodeId,
    pub visuals: Vec<Visual>,
    pub atom_visual: Vec<Option<VisualId>>,
    pub root_visual: VisualId,
    pub window: bool,
    pub window_atom: AtomId,
    pub bricks: Vec<Brick>,
    pub courses: Vec<Course>,
    pub wall: Wall,
    pub aligns: Vec<Alignment>,
    pub borders: Vec<Option<Border>>,
    pub text_borders: Vec<Option<TextBorder>>,
    pub carets: Vec<Option<Caret>>,
    pub drawings: Vec<Option<Drawing>>,
    pub tasks: Vec<Option<Task>>,
    pub queue: BinaryHeap<QueueEntry>,
    pub task_seq: u64,
    pub idle_lay_bricks: Option<TaskId>,
    pub hover_idle: Option<TaskId>,
    pub timer_requested: bool,
    pub iteration_timer: bool,
    pub iteration_pending: bool,
    pub edge: f64,
    pub wall_usage: (f64, f64),
    pub transverse_edge: f64,
    pub scroll: f64,
    pub scroll_start: f64,
    pub scroll_end: f64,
    pub pending_scroll: Option<f64>,
    pub hover_brick: Option<BrickId>,
    pub hoverables: Vec<Option<Hoverable>>,
    pub hover: Option<HoverableId>,
    pub cursors: Vec<Option<Cursor>>,
    pub cursor: Option<CursorId>,
    pub select_token: u64,
    pub drag_select: Option<DragSelect>,
    pub key_pending: Vec<KeyStroke>,
}

impl Context {
    pub fn context_new(
        syntax: Rc<Syntax>,
        document: Rc<Document>,
        config: ContextConfig,
        mut display: Box<dyn Display>,
        environment: Box<dyn Environment>,
        converse_size: f64,
        transverse_size: f64,
    ) -> Context {
        display.display_set_background(&syntax.spec_root.background);
        let syntax_for_stylist = syntax.clone();
        let to_pixels = display.display_to_pixels(syntax.spec_root.display_unit);
        let from_pixels_to_mm = 1. / display.display_to_pixels(crate::spec::SpecDisplayUnit::Mm);
        let background_layer = display.display_layer(DisplayLayer::Background);
        let text_layer = display.display_layer(DisplayLayer::Text);
        let overlay_layer = display.display_layer(DisplayLayer::Overlay);
        let mut c = Context {
            atom_visual: vec![
                None;
                document.atoms.len()
            ],
            syntax: syntax,
            document: document,
            config: config,
            display: display,
            environment: environment,
            stylist: Rc::new(StylistDirect { syntax: syntax_for_stylist }),
            to_pixels: to_pixels,
            from_pixels_to_mm: from_pixels_to_mm,
            background_layer: background_layer,
            text_layer: text_layer,
            overlay_layer: overlay_layer,
            visuals: vec![],
            root_visual: 0,
            window: false,
            window_atom: 0,
            bricks: vec![],
            courses: vec![],
            wall: Wall::default(),
            aligns: vec![],
            borders: vec![],
            text_borders: vec![],
            carets: vec![],
            drawings: vec![],
            tasks: vec![],
            queue: BinaryHeap::new(),
            task_seq: 0,
            idle_lay_bricks: None,
            hover_idle: None,
            timer_requested: false,
            iteration_timer: false,
            iteration_pending: false,
            edge: 0.,
            wall_usage: (0., 0.),
            transverse_edge: 0.,
            scroll: 0.,
            scroll_start: 0.,
            scroll_end: 0.,
            pending_scroll: None,
            hover_brick: None,
            hoverables: vec![],
            hover: None,
            cursors: vec![],
            cursor: None,
            select_token: 0,
            drag_select: None,
            key_pending: vec![],
        };
        c.edge = c.edge_from_converse_size(converse_size);
        c.wall.mod_old_edge = c.edge;
        c.transverse_edge = transverse_size;
        let root = c.document.root;
        c.window_atom = root;
        if c.config.start_windowed {
            c.window = true;
        }
        c.root_visual = c.visual_ensure_atom(root, None, 0, 0);
        let cornerstone =
            c.visual_create_or_get_cornerstone_candidate(c.root_visual).expect("root produced no brick");
        c.wall_set_cornerstone(cornerstone, None, None);
        c.trigger_idle_lay_bricks_outward();
        return c;
    }

    fn edge_from_converse_size(&self, converse_size: f64) -> f64 {
        let pad = &self.syntax.spec_root.pad;
        return f64::max(0., converse_size - pad.converse_start - pad.converse_end);
    }

    pub fn context_resize(&mut self, converse_size: f64, transverse_size: f64) {
        let old_edge = self.edge;
        self.edge = self.edge_from_converse_size(converse_size);
        if self.edge != old_edge {
            self.wall_converse_edge_changed(old_edge, self.edge);
        }
        if transverse_size != self.transverse_edge {
            self.transverse_edge = transverse_size;
            self.scroll_visible();
        }
    }

    pub fn context_scrolled(&mut self, scroll: f64) {
        self.scroll = scroll;
    }

    pub fn atom_id_of_visual(&self, visual: VisualId) -> AtomId {
        let Visual { kind: crate::visual::VisualKind::Atom(a), .. } = &self.visuals[visual] else {
            panic!("visual {} is not an atom", visual);
        };
        return a.atom;
    }

    pub fn atom_is_subtree(&self, subtree: AtomId, supertree: AtomId) -> bool {
        let mut at = subtree;
        loop {
            if at == supertree {
                return true;
            }
            let Some(parent) = &self.document.document_atom(at).parent else {
                return false;
            };
            at = parent.atom;
        }
    }

    pub fn window_adjust_minimal_to(&mut self, atom: AtomId) {
        if self.atom_is_subtree(self.window_atom, atom) {
            self.window_to_supertree(atom);
            return;
        }
        let mut next_window = atom;
        let mut depth = 0i64;
        loop {
            if next_window == self.window_atom {
                return;
            }
            let Some(parent) = &self.document.document_atom(next_window).parent else {
                break;
            };
            let parent_atom = parent.atom;
            depth += self.syntax.syntax_type(self.document.document_atom(next_window).type_).depth_score;
            if depth >= self.config.ellipsize_threshold {
                break;
            }
            next_window = parent_atom;
        }
        self.window_to_nonsupertree(next_window);
    }

    pub fn window_clear(&mut self) {
        self.window = false;
        let root = self.document.root;
        self.window_atom = root;
        self.root_visual = self.visual_ensure_atom(root, None, 0, 0);
    }

    fn window_to_supertree(&mut self, supertree: AtomId) {
        self.window = true;
        self.window_atom = supertree;
        self.root_visual = self.visual_ensure_atom(supertree, None, 0, 0);
    }

    fn window_to_nonsupertree(&mut self, tree: AtomId) {
        self.window = true;
        let old = self.root_visual;
        self.window_atom = tree;
        let visual = self.visual_ensure_atom(tree, None, 0, 0);
        self.root_visual = visual;
        if old != visual {
            self.visual_uproot(old, Some(visual));
        }
    }

    pub fn window_exact(&mut self, atom: AtomId) {
        if self.atom_is_subtree(self.window_atom, atom) {
            self.window_to_supertree(atom);
        } else {
            self.window_to_nonsupertree(atom);
        }
    }

    pub fn context_apply_scroll(&mut self) {
        self.pending_scroll = Some(self.scroll);
    }

    pub fn scroll_visible(&mut self) {
        let pad = &self.syntax.spec_root.pad;
        let minimum = self.scroll_start - self.wall.bedding_before - pad.transverse_start;
        let maximum = self.scroll_end + self.wall.bedding_after + pad.transverse_end;
        let max_diff = maximum - self.transverse_edge - self.scroll;
        let mut new_scroll = None;
        if minimum < self.scroll {
            new_scroll = Some(minimum);
        } else if max_diff > 0. {
            new_scroll = Some(f64::min(self.scroll + max_diff, minimum));
        }
        if let Some(s) = new_scroll {
            self.scroll = s;
            self.context_apply_scroll();
        }
    }
}
