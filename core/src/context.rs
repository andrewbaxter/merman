use {
    crate::{
        alignment::Alignment,
        attachment::{
            Border,
            Caret,
            Drawing,
            Mark,
            TextBorder,
        },
        cursor::{
            Cursor,
            DragSelect,
            Hoverable,
        },
        display::{
            Display,
            DisplayLayer,
            DisplayNodeId,
        },
        document::{
            AtomId,
            Document,
        },
        edit::{
            EditBatch,
            EditUnique,
        },
        environment::Environment,
        gap::GapChoices,
        iteration::{
            QueueEntry,
            Task,
        },
        keys::{
            KeyStroke,
            Keymap,
        },
        patch::Patch,
        stylist::{
            Stylist,
            StylistDirect,
        },
        syntax::Syntax,
        visual::Visual,
        wall::{
            Brick,
            Course,
            Wall,
        },
    },
    std::{
        collections::{
            BinaryHeap,
            HashMap,
            HashSet,
        },
        rc::Rc,
    },
};

pub type AlignId = usize;
pub type BorderId = usize;
pub type BrickId = usize;
pub type CaretId = usize;

pub struct Context {
    pub aligns: Vec<Alignment>,
    pub atom_marks: HashMap<AtomId, Vec<MarkId>>,
    pub atom_visual: Vec<Option<VisualId>>,
    pub background_layer: DisplayNodeId,
    pub borders: Vec<Option<Border>>,
    pub bricks: Vec<Brick>,
    pub carets: Vec<Option<Caret>>,
    pub config: ContextConfig,
    pub courses: Vec<Course>,
    pub cursor: Option<CursorId>,
    pub cursors: Vec<Option<Cursor>>,
    pub details: Option<(DisplayNodeId, usize, f64)>,
    pub display: Box<dyn Display>,
    pub document: Document,
    pub drag_select: Option<DragSelect>,
    pub drawings: Vec<Option<Drawing>>,
    pub edge: f64,
    pub edit_last: Option<(EditUnique, f64)>,
    pub edit_outbox: Vec<EditBatch>,
    pub edit_patches: Option<Vec<Patch>>,
    pub environment: Box<dyn Environment>,
    pub gap_choices: Option<GapChoices>,
    pub hover: Option<HoverableId>,
    pub hover_brick: Option<BrickId>,
    pub hover_idle: Option<TaskId>,
    pub hoverables: Vec<Option<Hoverable>>,
    pub idle_lay_bricks: Option<TaskId>,
    pub ids_next: i64,
    pub ids_used: HashSet<i64>,
    pub iteration_pending: bool,
    pub iteration_timer: bool,
    pub key_pending: Vec<KeyStroke>,
    pub marks: Vec<Option<Mark>>,
    pub overlay_layer: DisplayNodeId,
    pub queue: BinaryHeap<QueueEntry>,
    pub root_visual: VisualId,
    pub scroll: f64,
    pub scroll_end: f64,
    pub scroll_follow: bool,
    pub scroll_start: f64,
    pub select_token: u64,
    pub stylist: Rc<dyn Stylist>,
    pub syntax: Rc<Syntax>,
    pub task_seq: u64,
    pub tasks: Vec<Option<Task>>,
    pub text_borders: Vec<Option<TextBorder>>,
    pub text_layer: DisplayNodeId,
    pub timer_requested: bool,
    pub transverse_edge: f64,
    pub visuals: Vec<Visual>,
    pub wall: Wall,
    pub wall_usage: (f64, f64),
    pub window: bool,
    pub window_atom: AtomId,
}

impl Context {
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

    pub fn context_apply_scroll(&mut self) {
        let converse_pad = self.syntax.spec_root.pad.converse_start.round();
        let scroll = self.scroll.round();
        let animate = self.config.animate_course_placement;
        for layer in [self.background_layer, self.text_layer, self.overlay_layer] {
            self.display.node_set_position(layer, converse_pad, -scroll, animate);
        }
        self.wall_view_changed();
    }

    pub fn context_new(
        syntax: Rc<Syntax>,
        document: Document,
        config: ContextConfig,
        mut display: Box<dyn Display>,
        environment: Box<dyn Environment>,
        converse_size: f64,
        transverse_size: f64,
    ) -> Context {
        display.display_set_background(&syntax.spec_root.background);
        let syntax_for_stylist = syntax.clone();
        let background_layer = display.display_layer(DisplayLayer::Background);
        let text_layer = display.display_layer(DisplayLayer::Text);
        let overlay_layer = display.display_layer(DisplayLayer::Overlay);
        let mut c = Context {
            atom_marks: HashMap::new(),
            atom_visual: vec![
                None;
                document.atoms.len()
            ],
            marks: vec![],
            syntax: syntax,
            document: document,
            config: config,
            display: display,
            environment: environment,
            stylist: Rc::new(StylistDirect { syntax: syntax_for_stylist }),
            background_layer: background_layer,
            text_layer: text_layer,
            overlay_layer: overlay_layer,
            visuals: vec![],
            root_visual: 0,
            window: false,
            window_atom: 0,
            details: None,
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
            scroll_follow: true,
            hover_brick: None,
            hoverables: vec![],
            hover: None,
            cursors: vec![],
            cursor: None,
            select_token: 0,
            drag_select: None,
            key_pending: vec![],
            edit_last: None,
            edit_outbox: vec![],
            edit_patches: None,
            gap_choices: None,
            ids_next: 0,
            ids_used: HashSet::new(),
        };
        let root = c.document.root;
        c.ids_take_existing(root);
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
        c.context_apply_scroll();
        c.trigger_idle_lay_bricks_outward();
        return c;
    }

    pub fn context_resize(&mut self, converse_size: f64, transverse_size: f64) {
        let old_edge = self.edge;
        self.edge = self.edge_from_converse_size(converse_size);
        if self.edge != old_edge {
            self.wall_converse_edge_changed(old_edge, self.edge);
            self.details_place();
        }
        if transverse_size != self.transverse_edge {
            self.transverse_edge = transverse_size;
            self.scroll_visible();
            self.wall_view_changed();
        }
    }

    pub fn context_scroll_by(&mut self, delta: f64) {
        self.scroll_follow = false;
        self.scroll += delta;
        self.context_apply_scroll();
    }

    fn edge_from_converse_size(&self, converse_size: f64) -> f64 {
        let pad = &self.syntax.spec_root.pad;
        return f64::max(0., converse_size - pad.converse_start - pad.converse_end);
    }

    pub fn scroll_visible(&mut self) {
        if !self.scroll_follow {
            return;
        }
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

    pub fn window_exact(&mut self, atom: AtomId) {
        if self.atom_is_subtree(self.window_atom, atom) {
            self.window_to_supertree(atom);
        } else {
            self.window_to_nonsupertree(atom);
        }
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

    fn window_to_supertree(&mut self, supertree: AtomId) {
        self.window = true;
        self.window_atom = supertree;
        self.root_visual = self.visual_ensure_atom(supertree, None, 0, 0);
    }
}

pub struct ContextConfig {
    pub animate_course_placement: bool,
    pub editable: bool,
    pub ellipsize_threshold: i64,
    pub keys: Keymap,
    pub lay_beyond_view: f64,
    pub lay_brick_batch_size: usize,
    pub retry_expand_factor: f64,
    pub scroll_alot_factor: f64,
    pub scroll_factor: f64,
    pub start_windowed: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        return ContextConfig {
            lay_brick_batch_size: 10,
            lay_beyond_view: 1.,
            retry_expand_factor: 1.25,
            ellipsize_threshold: i64::MAX,
            animate_course_placement: false,
            editable: false,
            start_windowed: false,
            scroll_factor: 0.1,
            scroll_alot_factor: 0.8,
            keys: Keymap::default(),
        };
    }
}

pub type CourseId = usize;
pub type CursorId = usize;
pub type DrawingId = usize;
pub type HoverableId = usize;
pub type MarkId = usize;
pub type TaskId = usize;
pub type TextBorderId = usize;

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

pub type VisualId = usize;
