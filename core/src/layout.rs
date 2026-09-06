//! Lays bricks into courses (lines) within a converse edge, incrementally
//! (ported from merman's wall/course iteration tasks).
//!
//! Every brick is placed once. A course that overflows the edge is compacted:
//! the lowest precedence, shallowest atom on it is marked compact so its
//! `compact` split bricks start new lines; when no atom is left the last
//! primitive on the line is soft wrapped. When the edge grows (or a course gets
//! much shorter than it was) courses are expanded: the highest precedence,
//! deepest compact atom whose bricks would fit is un-compacted. Alignments
//! position the first aligned brick of a course and are recomputed as their
//! member courses change. Layout state persists across edge changes so only
//! affected courses are touched.
use crate::measure::{measure_index_at_converse, measure_line_before_or_at, Measure};
use crate::spec::SpecSplit;
use crate::syntax::{StyleId, Syntax};
use crate::visual::{
    visual_text_metrics, AlignId, AlignKind, Brick, BrickId, BrickPrimitive, PrimId, Visual,
    VisualAtomId,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

pub type CourseId = usize;

pub struct LayoutConfig {
    /// Expansion is retried when a course shrinks below its previous length
    /// divided by this, and when the edge grows past its old value times this.
    pub retry_expand_factor: f64,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        return LayoutConfig {
            retry_expand_factor: 1.25,
        };
    }
}

struct BrickState {
    brick: Brick,
    course: CourseId,
    index: usize,
    converse: f64,
    /// Converse the brick would have without alignment.
    pre_align: f64,
    alive: bool,
}

struct Course {
    bricks: Vec<BrickId>,
    prev: Option<CourseId>,
    next: Option<CourseId>,
    alignment: Option<AlignId>,
    /// Pre-alignment converse of the aligned brick.
    aligned_pre: f64,
    ascent: f64,
    descent: f64,
    end: f64,
    /// Longest the course has been since it was last expanded.
    last_expand_check: f64,
    transverse: f64,
    alive: bool,
    queued_place: bool,
    queued_overflow: bool,
    queued_expand: bool,
}

struct AlignState {
    kind: AlignKind,
    converse: f64,
    derived: Vec<AlignId>,
    /// Courses whose first aligned brick uses this alignment.
    members: HashSet<CourseId>,
}

/// State of converse placement along a course.
#[derive(Default)]
struct CalcConverse {
    converse: f64,
    pre_align: f64,
    alignment: Option<AlignId>,
}

pub struct Layout {
    syntax: Rc<Syntax>,
    visual: Rc<Visual>,
    config: LayoutConfig,
    pub edge: f64,
    /// Edge at the last compaction/expansion trigger (merman's `modOldValue`).
    mod_old_edge: f64,
    compact: Vec<bool>,
    bricks: Vec<BrickState>,
    courses: Vec<Course>,
    head: CourseId,
    aligns: Vec<AlignState>,
    prim_lines: HashMap<BrickPrimitive, Vec<BrickId>>,
    place_queue: VecDeque<CourseId>,
    overflow_queue: VecDeque<CourseId>,
    expand_queue: VecDeque<CourseId>,
    work: usize,
}

pub struct Row {
    pub transverse: f64,
    pub ascent: f64,
    pub descent: f64,
    pub bricks: Vec<RowBrick>,
}

pub struct RowBrick {
    /// Start of the brick (text starts `pad_before` further along).
    pub converse: f64,
    pub width: f64,
    pub pad_before: f64,
    pub ascent: f64,
    pub descent: f64,
    pub text: String,
    pub style: StyleId,
}

pub struct Rows {
    pub rows: Vec<Row>,
    pub width: f64,
    pub height: f64,
}

impl Layout {
    pub fn layout_build(
        syntax: Rc<Syntax>,
        visual: Rc<Visual>,
        config: LayoutConfig,
        edge: f64,
        measure: &mut dyn Measure,
    ) -> Layout {
        let mut aligns: Vec<AlignState> = visual
            .aligns
            .iter()
            .map(|a| AlignState {
                kind: match &a.kind {
                    AlignKind::Relative {
                        base,
                        offset,
                        collapse,
                    } => AlignKind::Relative {
                        base: *base,
                        offset: *offset,
                        collapse: *collapse,
                    },
                    AlignKind::Concensus => AlignKind::Concensus,
                },
                converse: 0.,
                derived: vec![],
                members: HashSet::new(),
            })
            .collect();
        for i in 0..aligns.len() {
            if let AlignKind::Relative {
                base,
                offset,
                collapse,
            } = aligns[i].kind
            {
                let base_converse = match base {
                    Some(b) => {
                        // Bases live in ancestor atoms, which are created first.
                        assert!(b < i, "alignment base created after derived alignment");
                        aligns[b].derived.push(i);
                        aligns[b].converse
                    }
                    None => 0.,
                };
                aligns[i].converse = base_converse + if collapse { 0. } else { offset };
            }
        }
        let mut l = Layout {
            edge,
            mod_old_edge: f64::MAX,
            compact: vec![false; visual.atoms.len()],
            bricks: visual
                .bricks
                .iter()
                .map(|b| BrickState {
                    brick: b.clone(),
                    course: 0,
                    index: 0,
                    converse: 0.,
                    pre_align: 0.,
                    alive: true,
                })
                .collect(),
            courses: vec![],
            head: 0,
            aligns,
            prim_lines: HashMap::new(),
            place_queue: VecDeque::new(),
            overflow_queue: VecDeque::new(),
            expand_queue: VecDeque::new(),
            work: 0,
            syntax,
            visual: visual.clone(),
            config,
        };
        for (i, b) in visual.bricks.iter().enumerate() {
            if let Some(p) = b.primitive {
                l.prim_lines.entry(p).or_default().push(i);
            }
        }
        let mut current = l.new_course(None);
        l.head = current;
        for i in 0..l.bricks.len() {
            if i > 0 && l.is_split(i) {
                current = l.new_course(Some(current));
            }
            l.append_to_course(current, i);
        }
        let mut c = Some(l.head);
        while let Some(id) = c {
            l.queue_place(id);
            c = l.courses[id].next;
        }
        l.layout_set_edge(edge, measure);
        return l;
    }

    /// The converse space changed: compact or expand as needed.
    pub fn layout_set_edge(&mut self, edge: f64, measure: &mut dyn Measure) {
        self.edge = edge;
        if edge < self.mod_old_edge {
            let mut c = Some(self.head);
            while let Some(id) = c {
                self.queue_overflow(id);
                c = self.courses[id].next;
            }
            self.mod_old_edge = edge;
        } else if edge > self.mod_old_edge * self.config.retry_expand_factor {
            let mut c = Some(self.head);
            while let Some(id) = c {
                self.queue_expand(id);
                c = self.courses[id].next;
            }
            self.mod_old_edge = edge;
        }
        self.flush(measure);
    }

    /// Placed lines in order.
    pub fn layout_rows(&self) -> Rows {
        let mut rows = vec![];
        let mut width: f64 = 0.;
        let mut height: f64 = 0.;
        let mut c = Some(self.head);
        while let Some(id) = c {
            let course = &self.courses[id];
            width = width.max(course.end);
            height = course.transverse + course.ascent + course.descent;
            rows.push(Row {
                transverse: course.transverse,
                ascent: course.ascent,
                descent: course.descent,
                bricks: course
                    .bricks
                    .iter()
                    .filter_map(|b| {
                        let bs = &self.bricks[*b];
                        let text = bs.brick.text.as_ref()?;
                        return Some(RowBrick {
                            converse: bs.converse,
                            width: bs.brick.width,
                            pad_before: bs.brick.pad_before,
                            ascent: bs.brick.ascent,
                            descent: bs.brick.descent,
                            text: text.clone(),
                            style: bs.brick.style,
                        });
                    })
                    .collect(),
            });
            c = course.next;
        }
        return Rows {
            rows,
            width,
            height,
        };
    }

    /// Run queued work in merman's priority order: placement, then course
    /// compaction, then expansion (which only proceeds while nothing else is
    /// pending).
    fn flush(&mut self, measure: &mut dyn Measure) {
        let mut expanded = HashSet::new();
        loop {
            self.process_places();
            if let Some(c) = self.overflow_queue.pop_front() {
                self.courses[c].queued_overflow = false;
                if self.courses[c].alive {
                    self.course_compact(c, measure);
                }
                continue;
            }
            if let Some(c) = self.expand_queue.pop_front() {
                self.courses[c].queued_expand = false;
                if self.courses[c].alive && self.expand_step(c, &mut expanded, measure) {
                    // Like merman's wall expand task stepping back after the
                    // course count changes, revisit this course and the one before.
                    if self.courses[c].alive {
                        self.queue_expand_front(c);
                        if let Some(p) = self.courses[c].prev {
                            self.queue_expand_front(p);
                        }
                    }
                }
                continue;
            }
            break;
        }
        self.assign_transverse();
    }

    fn new_course(&mut self, after: Option<CourseId>) -> CourseId {
        let id = self.courses.len();
        let next = after.and_then(|a| self.courses[a].next);
        self.courses.push(Course {
            bricks: vec![],
            prev: after,
            next,
            alignment: None,
            aligned_pre: 0.,
            ascent: 0.,
            descent: 0.,
            end: 0.,
            last_expand_check: 0.,
            transverse: 0.,
            alive: true,
            queued_place: false,
            queued_overflow: false,
            queued_expand: false,
        });
        if let Some(a) = after {
            self.courses[a].next = Some(id);
        }
        if let Some(n) = next {
            self.courses[n].prev = Some(id);
        }
        return id;
    }

    fn remove_course(&mut self, c: CourseId) {
        let (prev, next) = (self.courses[c].prev, self.courses[c].next);
        if let Some(p) = prev {
            self.courses[p].next = next;
        } else {
            self.head = next.expect("removing the only course");
        }
        if let Some(n) = next {
            self.courses[n].prev = prev;
        }
        self.courses[c].alive = false;
        if let Some(a) = self.courses[c].alignment.take() {
            self.aligns[a].members.remove(&c);
            self.align_members_changed(a);
        }
    }

    fn append_to_course(&mut self, c: CourseId, b: BrickId) {
        let index = self.courses[c].bricks.len();
        self.courses[c].bricks.push(b);
        self.bricks[b].course = c;
        self.bricks[b].index = index;
    }

    fn renumber(&mut self, c: CourseId, from: usize) {
        for i in from..self.courses[c].bricks.len() {
            let b = self.courses[c].bricks[i];
            self.bricks[b].course = c;
            self.bricks[b].index = i;
        }
    }

    fn is_split_with(&self, b: BrickId, compact: bool) -> bool {
        match self.bricks[b].brick.split {
            SpecSplit::Never => return false,
            SpecSplit::Always => return true,
            SpecSplit::Compact => return compact,
        }
    }

    fn is_split(&self, b: BrickId) -> bool {
        return self.is_split_with(b, self.compact[self.bricks[b].brick.owner]);
    }

    fn brick_alignment(&self, b: BrickId) -> Option<AlignId> {
        if self.is_split(b) {
            return self.bricks[b].brick.split_align;
        } else {
            return self.bricks[b].brick.align;
        }
    }

    fn queue_place(&mut self, c: CourseId) {
        if self.courses[c].queued_place || !self.courses[c].alive {
            return;
        }
        self.courses[c].queued_place = true;
        self.place_queue.push_back(c);
    }

    fn queue_overflow(&mut self, c: CourseId) {
        if self.courses[c].queued_overflow || !self.courses[c].alive {
            return;
        }
        self.courses[c].queued_overflow = true;
        self.overflow_queue.push_back(c);
    }

    fn queue_expand(&mut self, c: CourseId) {
        if self.courses[c].queued_expand || !self.courses[c].alive {
            return;
        }
        self.courses[c].queued_expand = true;
        self.expand_queue.push_back(c);
    }

    fn queue_expand_front(&mut self, c: CourseId) {
        if self.courses[c].queued_expand || !self.courses[c].alive {
            return;
        }
        self.courses[c].queued_expand = true;
        self.expand_queue.push_front(c);
    }

    fn process_places(&mut self) {
        while let Some(c) = self.place_queue.pop_front() {
            self.courses[c].queued_place = false;
            if !self.courses[c].alive {
                continue;
            }
            self.place(c);
        }
    }

    /// Advance placement over a brick: returns (converse, pre-align converse) of
    /// the brick.
    fn calc_advance(&self, calc: &mut CalcConverse, b: BrickId) -> (f64, f64) {
        let mut out = calc.converse;
        let pre = calc.pre_align;
        if calc.alignment.is_none() {
            if let Some(a) = self.brick_alignment(b) {
                calc.alignment = Some(a);
                out = out.max(self.aligns[a].converse);
            }
        }
        let width = self.bricks[b].brick.width;
        calc.pre_align += width;
        calc.converse = out + width;
        return (out, pre);
    }

    fn place(&mut self, c: CourseId) {
        self.work += 1;
        assert!(
            self.work < 40 * self.bricks.len() + 1_000_000,
            "layout did not converge"
        );
        let mut ascent: f64 = 0.;
        let mut descent: f64 = 0.;
        let mut calc = CalcConverse::default();
        let mut aligned_pre = 0.;
        let bricks = self.courses[c].bricks.clone();
        for b in bricks {
            {
                let bs = &self.bricks[b];
                ascent = ascent.max(bs.brick.ascent);
                descent = descent.max(bs.brick.descent);
            }
            let before = calc.alignment;
            let (converse, pre) = self.calc_advance(&mut calc, b);
            if before.is_none() && calc.alignment.is_some() {
                aligned_pre = pre;
            }
            let bs = &mut self.bricks[b];
            bs.pre_align = pre;
            bs.converse = converse;
        }
        let converse = calc.converse;
        let course_align = calc.alignment;
        {
            let course = &mut self.courses[c];
            course.ascent = ascent;
            course.descent = descent;
            course.end = converse;
            course.aligned_pre = aligned_pre;
        }
        let old = self.courses[c].alignment;
        if old != course_align {
            if let Some(o) = old {
                self.aligns[o].members.remove(&c);
                self.align_members_changed(o);
            }
            self.courses[c].alignment = course_align;
            if let Some(n) = course_align {
                self.aligns[n].members.insert(c);
                self.align_members_changed(n);
            }
        } else if let Some(n) = course_align {
            if matches!(self.aligns[n].kind, AlignKind::Concensus) {
                self.align_members_changed(n);
            }
        }
        if converse > self.edge {
            self.queue_overflow(c);
        }
        let factor = self.config.retry_expand_factor;
        if converse * factor < self.courses[c].last_expand_check {
            self.queue_expand(c);
        }
        if converse > self.courses[c].last_expand_check {
            self.courses[c].last_expand_check = converse;
        }
    }

    /// The set of courses using the alignment changed; update its position if
    /// that depends on membership.
    fn align_members_changed(&mut self, a: AlignId) {
        let new = match self.aligns[a].kind {
            AlignKind::Relative {
                base,
                offset,
                collapse,
            } => {
                if !collapse {
                    return;
                }
                let base_converse = base.map(|b| self.aligns[b].converse).unwrap_or(0.);
                base_converse
                    + if self.aligns[a].members.is_empty() {
                        0.
                    } else {
                        offset
                    }
            }
            AlignKind::Concensus => {
                let mut max: f64 = 0.;
                for m in &self.aligns[a].members {
                    max = max.max(self.courses[*m].aligned_pre);
                }
                max
            }
        };
        if new != self.aligns[a].converse {
            self.aligns[a].converse = new;
            self.align_changed(a);
        }
    }

    /// The alignment moved: re-place its members and update derived
    /// alignments.
    fn align_changed(&mut self, a: AlignId) {
        let members: Vec<CourseId> = self.aligns[a].members.iter().copied().collect();
        for m in members {
            self.queue_place(m);
        }
        let derived = self.aligns[a].derived.clone();
        for d in derived {
            let AlignKind::Relative {
                base: _,
                offset,
                collapse,
            } = self.aligns[d].kind
            else {
                panic!("derived alignment is not relative");
            };
            let new = self.aligns[a].converse
                + if collapse && self.aligns[d].members.is_empty() {
                    0.
                } else {
                    offset
                };
            if new != self.aligns[d].converse {
                self.aligns[d].converse = new;
                self.align_changed(d);
            }
        }
    }

    /// A brick's split state or size changed; fix up courses around it.
    fn brick_changed(&mut self, b: BrickId) {
        let (c, idx) = (self.bricks[b].course, self.bricks[b].index);
        if idx > 0 && self.is_split(b) {
            self.break_course(c, idx);
        } else if idx == 0 && !self.is_split(b) && self.courses[c].prev.is_some() {
            self.join_previous(c);
        } else {
            self.queue_place(c);
        }
    }

    fn break_course(&mut self, c: CourseId, idx: usize) {
        let moved: Vec<BrickId> = self.courses[c].bricks.drain(idx..).collect();
        let n = self.new_course(Some(c));
        self.courses[n].bricks = moved;
        self.renumber(n, 0);
        self.queue_place(c);
        self.queue_place(n);
    }

    fn join_previous(&mut self, c: CourseId) {
        let p = self.courses[c].prev.expect("joining first course");
        let moved = std::mem::take(&mut self.courses[c].bricks);
        let start = self.courses[p].bricks.len();
        self.courses[p].bricks.extend(moved);
        self.renumber(p, start);
        self.remove_course(c);
        self.queue_place(p);
    }

    fn remove_brick(&mut self, b: BrickId) {
        let (c, idx) = (self.bricks[b].course, self.bricks[b].index);
        self.bricks[b].alive = false;
        self.courses[c].bricks.remove(idx);
        self.renumber(c, idx);
        if let Some(p) = self.courses[c].prev {
            self.queue_expand(p);
        }
        if let Some(n) = self.courses[c].next {
            self.queue_expand(n);
        }
        if self.courses[c].bricks.is_empty() {
            self.remove_course(c);
        } else if idx == 0
            && self.courses[c].prev.is_some()
            && !self.is_split(self.courses[c].bricks[0])
        {
            self.join_previous(c);
        } else {
            self.queue_place(c);
        }
    }

    fn insert_brick_after(&mut self, after: BrickId, b: BrickId) {
        let (c, idx) = (self.bricks[after].course, self.bricks[after].index);
        self.courses[c].bricks.insert(idx + 1, b);
        self.renumber(c, idx + 1);
        self.brick_changed(b);
    }

    /// Merman's per-course compact task: keep compacting until the course fits
    /// or nothing is left to try. Each primitive is reflowed at most once.
    fn course_compact(&mut self, c: CourseId, measure: &mut dyn Measure) {
        let mut skip_prims: HashSet<PrimId> = HashSet::new();
        loop {
            if !self.courses[c].alive {
                break;
            }
            if !self.compact_step(c, &mut skip_prims, measure) {
                break;
            }
            self.process_places();
        }
    }

    /// Make one change that reduces the course's length. Returns false if the
    /// course fits or nothing can be done.
    fn compact_step(
        &mut self,
        c: CourseId,
        skip_prims: &mut HashSet<PrimId>,
        measure: &mut dyn Measure,
    ) -> bool {
        let mut candidates: Vec<VisualAtomId> = vec![];
        let mut seen = HashSet::new();
        let mut last_prim: Option<PrimId> = None;
        let mut converse = 0.;
        for b in self.courses[c].bricks.clone() {
            let bs = &self.bricks[b];
            if let Some(p) = bs.brick.primitive {
                if !skip_prims.contains(&p.prim) {
                    last_prim = Some(p.prim);
                }
            }
            let owner = bs.brick.owner;
            if !self.compact[owner] && seen.insert(owner) {
                candidates.push(owner);
            }
            converse = bs.converse + bs.brick.width;
            if !candidates.is_empty() && converse > self.edge {
                break;
            }
        }
        if converse <= self.edge {
            return false;
        }
        if candidates.is_empty() {
            match last_prim {
                Some(p) => {
                    self.primitive_reflow(p, measure);
                    skip_prims.insert(p);
                    return true;
                }
                None => return false,
            }
        }
        let mut best = candidates[0];
        for a in candidates.iter().skip(1) {
            if self.compact_before(*a, best) {
                best = *a;
            }
        }
        self.compact_atom(best);
        return true;
    }

    /// Whether `a` is compacted before `b`: lower precedence first, then
    /// shallower.
    fn compact_before(&self, a: VisualAtomId, b: VisualAtomId) -> bool {
        let (va, vb) = (&self.visual.atoms[a], &self.visual.atoms[b]);
        return (va.precedence, va.depth_score) < (vb.precedence, vb.depth_score);
    }

    /// Whether `a` is expanded strictly before `b`: the reverse of compaction
    /// order.
    fn expand_before(&self, a: VisualAtomId, b: VisualAtomId) -> bool {
        let (va, vb) = (&self.visual.atoms[a], &self.visual.atoms[b]);
        return (va.precedence, va.depth_score) > (vb.precedence, vb.depth_score);
    }

    /// Whether `a` is expanded before or together with `b` (merman's inclusive
    /// `isOrdered`).
    fn expand_ordered(&self, a: VisualAtomId, b: VisualAtomId) -> bool {
        let (va, vb) = (&self.visual.atoms[a], &self.visual.atoms[b]);
        return (va.precedence, va.depth_score) >= (vb.precedence, vb.depth_score);
    }

    fn compact_atom(&mut self, a: VisualAtomId) {
        self.compact[a] = true;
        self.atom_split_changed(a);
    }

    fn atom_split_changed(&mut self, a: VisualAtomId) {
        for b in self.visual.atoms[a].bricks.clone() {
            if self.bricks[b].alive && self.bricks[b].brick.split == SpecSplit::Compact {
                self.brick_changed(b);
            }
        }
    }

    /// Bricks of the atom's own symbols and primitives (including wrapped
    /// continuation lines), in course order.
    fn leaf_bricks(&self, a: VisualAtomId) -> Vec<BrickId> {
        let mut out = vec![];
        for b in &self.visual.atoms[a].bricks {
            match self.bricks[*b].brick.primitive {
                Some(p) => out.extend(self.prim_lines[&p].iter().copied()),
                None => out.push(*b),
            }
        }
        return out;
    }

    /// Whether any hard line of the primitive has been soft wrapped.
    fn soft_wrapped(&self, prim: PrimId) -> bool {
        for h in 0..self.visual.prims[prim].hard_lines {
            if self.prim_lines[&BrickPrimitive { prim, hard_line: h }].len() > 1 {
                return true;
            }
        }
        return false;
    }

    fn line_count(&self, prim: PrimId) -> usize {
        let mut n = 0;
        for h in 0..self.visual.prims[prim].hard_lines {
            n += self.prim_lines[&BrickPrimitive { prim, hard_line: h }].len();
        }
        return n;
    }

    /// Merman's per-course expand task step. Returns whether anything changed.
    fn expand_step(
        &mut self,
        c: CourseId,
        expanded: &mut HashSet<VisualAtomId>,
        measure: &mut dyn Measure,
    ) -> bool {
        // Find the next atom that can be expanded
        let mut top: Option<VisualAtomId> = None;
        for b in &self.courses[c].bricks {
            let owner = self.bricks[*b].brick.owner;
            if !self.compact[owner] {
                continue;
            }
            match top {
                Some(t) if !self.expand_before(owner, t) => {}
                _ => top = Some(owner),
            }
        }
        let Some(top) = top else {
            return false;
        };
        // Check that all parents are either expanded or have lower expand priority
        {
            let mut at = top;
            while let Some(p) = &self.visual.atoms[at].parent {
                at = p.atom;
                if self.compact[at] && self.expand_ordered(at, top) {
                    return false;
                }
            }
        }
        // 1. Check that this has the highest priority on all shared courses + courses
        // with child bricks
        // 2. Make sure any soft-wrapped own/child bricks are un-soft-wrapped before
        // deeper expanding
        {
            let Some((first, mut last)) = self.visual.atoms[top].subtree_bricks else {
                return false;
            };
            if let Some(p) = self.bricks[last].brick.primitive {
                last = *self.prim_lines[&p].last().unwrap();
            }
            let first_course = self.bricks[first].course;
            let last_course = self.bricks[last].course;
            let mut course = Some(first_course);
            while let Some(ci) = course {
                for b in self.courses[ci].bricks.clone() {
                    let bs = &self.bricks[b];
                    if let Some(p) = bs.brick.primitive {
                        if self.soft_wrapped(p.prim) {
                            let old_lines = self.line_count(p.prim);
                            self.primitive_reflow(p.prim, measure);
                            return old_lines != self.line_count(p.prim);
                        }
                    }
                    let atom = bs.brick.owner;
                    if !self.compact[atom] || atom == top || self.expand_ordered(top, atom) {
                        continue;
                    }
                    return false;
                }
                if ci == last_course {
                    break;
                }
                course = self.courses[ci].next;
            }
        }
        // Check if we actually can expand
        if !self.can_expand(top) {
            return false;
        }
        // Avoid bouncing
        if !expanded.insert(top) {
            return false;
        }
        self.compact[top] = false;
        self.atom_split_changed(top);
        self.courses[c].last_expand_check = 0.;
        return true;
    }

    /// Simulate the atom's own bricks un-splitting and check the affected
    /// courses stay within the edge.
    fn can_expand(&self, top: VisualAtomId) -> bool {
        let leaf_bricks = self.leaf_bricks(top);
        let mut course: Option<CourseId> = None;
        let mut course_last_brick = 0;
        let mut calc = CalcConverse::default();
        for b in leaf_bricks {
            let bc = self.bricks[b].course;
            // Line changed (or first brick)
            if Some(bc) != course {
                // If jumping to a new non-consecutive line and unbreaking, reset to calculate
                // previous line
                let unsplit = self.bricks[b].index == 0
                    && !self.is_split_with(b, false)
                    && self.courses[bc].prev.is_some();
                if unsplit {
                    if course.is_some() && self.courses[bc].prev == course {
                        // On next line, continue calculation
                    } else {
                        // Reset to previous line to calculate unsplit
                        calc = CalcConverse::default();
                        course = self.courses[bc].prev;
                        course_last_brick = 0;
                    }
                }
                // Finish previous line, see if it goes over
                if let Some(ci) = course {
                    while course_last_brick < self.courses[ci].bricks.len() {
                        let b1 = self.courses[ci].bricks[course_last_brick];
                        self.calc_advance(&mut calc, b1);
                        if calc.converse > self.edge {
                            return false;
                        }
                        course_last_brick += 1;
                    }
                }
                course_last_brick = 0;
                course = Some(bc);
                if !unsplit {
                    calc = CalcConverse::default();
                }
            }
            // Sum bricks leading up to modified brick
            let ci = course.unwrap();
            while course_last_brick < self.bricks[b].index {
                let b1 = self.courses[ci].bricks[course_last_brick];
                self.calc_advance(&mut calc, b1);
                if calc.converse > self.edge {
                    return false;
                }
                course_last_brick += 1;
            }
            // Sum modified brick
            self.calc_advance(&mut calc, b);
            if calc.converse > self.edge {
                return false;
            }
            course_last_brick += 1;
        }
        // Finish final line
        if let Some(ci) = course {
            while course_last_brick < self.courses[ci].bricks.len() {
                let b1 = self.courses[ci].bricks[course_last_brick];
                self.calc_advance(&mut calc, b1);
                if calc.converse > self.edge {
                    return false;
                }
                course_last_brick += 1;
            }
        }
        return true;
    }

    /// Merman's `primitiveReflow`: re-wrap each hard line group that overflows
    /// or has become comfortably short.
    fn primitive_reflow(&mut self, prim: PrimId, measure: &mut dyn Measure) {
        let factor = self.config.retry_expand_factor;
        let mut any_over = false;
        let mut all_under = true;
        for h in (0..self.visual.prims[prim].hard_lines).rev() {
            let key = BrickPrimitive { prim, hard_line: h };
            for b in self.prim_lines[&key].clone().into_iter().rev() {
                let bs = &self.bricks[b];
                let edge = bs.converse + bs.brick.width;
                if !any_over && edge > self.edge {
                    any_over = true;
                }
                if edge <= self.edge && edge * factor >= self.edge {
                    all_under = false;
                }
            }
            if any_over || all_under {
                self.resplit_one(key, measure);
                any_over = false;
                all_under = true;
            }
        }
    }

    /// Merman's `resplitOne`: wrap one hard line's text to fit the edge from
    /// its current position. Returns whether anything changed.
    fn resplit_one(&mut self, key: BrickPrimitive, measure: &mut dyn Measure) -> bool {
        let line_bricks = self.prim_lines[&key].clone();
        let first = line_bricks[0];
        let old: Vec<String> = line_bricks
            .iter()
            .map(|lb| self.bricks[*lb].brick.text.clone().unwrap())
            .collect();
        let full: String = old.concat();
        let visual = self.visual.clone();
        let prim = &visual.prims[key.prim];
        let font = self.syntax.syntax_style(prim.style).font.clone();
        let soft_start = prim
            .soft_split_align
            .map(|a| self.aligns[a].converse)
            .unwrap_or(0.);
        let mut pieces: Vec<String> = vec![];
        let mut remaining: &str = &full;
        let mut piece_index = 0;
        loop {
            // Wrap text into existing lines (from their current positions), then new ones
            let converse = match line_bricks.get(piece_index) {
                Some(lb) => self.bricks[*lb].converse,
                None => soft_start,
            };
            let width = measure.measure_width(&font, remaining);
            let split = if converse < self.edge && converse + width > self.edge {
                let under =
                    measure_index_at_converse(measure, &font, remaining, self.edge - converse);
                if under == remaining.len() {
                    under
                } else {
                    let mut s = measure_line_before_or_at(remaining, under);
                    if s == 0 {
                        s = under;
                    }
                    if remaining[..s].chars().count() < 4 {
                        // Compact limit: not worth wrapping
                        remaining.len()
                    } else {
                        s
                    }
                }
            } else {
                remaining.len()
            };
            pieces.push(remaining[..split].to_string());
            remaining = &remaining[split..];
            piece_index += 1;
            if remaining.is_empty() {
                break;
            }
        }
        if pieces == old {
            return false;
        }
        for lb in &line_bricks[1..] {
            self.remove_brick(*lb);
        }
        self.prim_lines.get_mut(&key).unwrap().truncate(1);
        let (owner, style_id, soft_align) = (prim.owner, prim.style, prim.soft_split_align);
        {
            let m = visual_text_metrics(&self.syntax, measure, style_id, &pieces[0]);
            let bs = &mut self.bricks[first];
            bs.brick.text = Some(pieces[0].clone());
            bs.brick.width = m.width;
            let c = bs.course;
            self.queue_place(c);
        }
        let mut after = first;
        for piece in pieces.into_iter().skip(1) {
            let m = visual_text_metrics(&self.syntax, measure, style_id, &piece);
            let id = self.bricks.len();
            self.bricks.push(BrickState {
                brick: Brick {
                    owner,
                    text: Some(piece),
                    style: style_id,
                    split: SpecSplit::Always,
                    align: None,
                    split_align: soft_align,
                    width: m.width,
                    ascent: m.ascent,
                    descent: m.descent,
                    pad_before: m.pad_before,
                    primitive: Some(key),
                },
                course: 0,
                index: 0,
                converse: 0.,
                pre_align: 0.,
                alive: true,
            });
            self.prim_lines.get_mut(&key).unwrap().push(id);
            self.insert_brick_after(after, id);
            after = id;
        }
        return true;
    }

    fn assign_transverse(&mut self) {
        let stride = self.syntax.spec_root.course_transverse_stride;
        let mut t = 0.;
        let mut c = Some(self.head);
        while let Some(id) = c {
            let course = &mut self.courses[id];
            course.transverse = t;
            t += if stride > 0. {
                stride
            } else {
                course.ascent + course.descent
            };
            c = course.next;
        }
    }
}
