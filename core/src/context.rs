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
use crate::document::{
    AtomId,
    Document,
};
use crate::iteration::{
    QueueEntry,
    Task,
};
use crate::measure::Measure;
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
}

impl Default for ContextConfig {
    fn default() -> Self {
        return ContextConfig {
            lay_brick_batch_size: 10,
            retry_expand_factor: 1.25,
        };
    }
}

pub struct Context {
    pub syntax: Rc<Syntax>,
    pub document: Rc<Document>,
    pub config: ContextConfig,
    pub measure: Box<dyn Measure>,
    pub visuals: Vec<Visual>,
    pub atom_visual: Vec<Option<VisualId>>,
    pub root_visual: VisualId,
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
    pub clipboard: Option<String>,
}

impl Context {
    pub fn context_new(
        syntax: Rc<Syntax>,
        document: Rc<Document>,
        config: ContextConfig,
        measure: Box<dyn Measure>,
        converse_size: f64,
        transverse_size: f64,
    ) -> Context {
        let mut c = Context {
            atom_visual: vec![
                None;
                document.atoms.len()
            ],
            syntax: syntax,
            document: document,
            config: config,
            measure: measure,
            visuals: vec![],
            root_visual: 0,
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
            clipboard: None,
        };
        c.edge = c.edge_from_converse_size(converse_size);
        c.wall.mod_old_edge = c.edge;
        c.transverse_edge = transverse_size;
        let root = c.document.root;
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

    pub fn scroll_visible(&mut self) {
        let pad = &self.syntax.spec_root.pad;
        let minimum = self.scroll_start - pad.transverse_start;
        let maximum = self.scroll_end + pad.transverse_end;
        let max_diff = maximum - self.transverse_edge - self.scroll;
        let mut new_scroll = None;
        if minimum < self.scroll {
            new_scroll = Some(minimum);
        } else if max_diff > 0. {
            new_scroll = Some(f64::min(self.scroll + max_diff, minimum));
        }
        if let Some(s) = new_scroll {
            self.scroll = s;
            self.pending_scroll = Some(s);
        }
    }
}
