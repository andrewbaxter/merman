//! Bricks, courses (lines) and the wall (merman `wall/`), including the
//! per-course place/compact/expand tasks and the wall adjust/compact/expand
//! tasks.
use crate::attachment::AttachmentRef;
use crate::context::{AlignId, BrickId, Context, CourseId, TaskId, Vector, VisualId};
use crate::iteration::{IterationContext, TaskKind, P};
use crate::spec::SpecSplit;
use crate::syntax::StyleId;
use crate::visual::VisualKind;
use crate::wall::BrickInter::Line;
use std::collections::HashSet;

pub enum BrickKind {
    Text { text: String, style: StyleId },
    Empty { ascent: f64, descent: f64, span: f64 },
}

/// The visual a brick belongs to (merman `BrickInterface`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrickInter {
    Symbol(VisualId),
    Line(VisualId, usize),
    ArrayEmpty(VisualId),
}

impl BrickInter {
    pub fn visual(&self) -> VisualId {
        match self {
            BrickInter::Symbol(v) | BrickInter::Line(v, _) | BrickInter::ArrayEmpty(v) => return *v,
        }
    }
}

pub struct Brick {
    pub kind: BrickKind,
    pub inter: BrickInter,
    pub split: SpecSplit,
    pub align_id: Option<AlignId>,
    pub split_align_id: Option<AlignId>,
    /// Alignment in effect (depends on whether the brick is split).
    pub alignment: Option<AlignId>,
    pub course: Option<CourseId>,
    pub index: usize,
    pub converse: f64,
    pub converse_span: f64,
    /// Used to recalc alignment min when a brick is removed from alignment.
    pub pre_align_converse: f64,
    pub ascent: f64,
    pub descent: f64,
    /// Offset of the text within the brick.
    pub pad_before: f64,
    pub attachments: Vec<AttachmentRef>,
    pub alive: bool,
}

pub struct Course {
    pub index: usize,
    pub transverse_start: f64,
    pub ascent: f64,
    pub descent: f64,
    pub children: Vec<BrickId>,
    pub alignment: Option<AlignId>,
    pub alignment_brick: Option<BrickId>,
    pub last_expand_check_converse: f64,
    pub idle_place: Option<TaskId>,
    pub idle_compact: Option<TaskId>,
    pub idle_expand: Option<TaskId>,
    pub alive: bool,
}

#[derive(Default)]
pub struct Wall {
    pub children: Vec<CourseId>,
    /// Cornerstone may be None. Cornerstone course is only None in transition.
    pub cornerstone: Option<BrickId>,
    pub cornerstone_course: Option<CourseId>,
    pub idle_adjust: Option<TaskId>,
    pub idle_compact: Option<TaskId>,
    pub idle_expand: Option<TaskId>,
    /// Edge at the last compaction/expansion trigger (merman's `modOldValue`).
    pub mod_old_edge: f64,
}

/// State of layout on a single course.
#[derive(Default)]
struct CalcCourseConverse {
    converse: f64,
    alignment: Option<AlignId>,
    pre_align_converse: f64,
}

impl Context {
    // ---- Bricks ------------------------------------------------------------

    pub fn brick_new(
        &mut self,
        kind: BrickKind,
        inter: BrickInter,
        split: SpecSplit,
        align_id: Option<AlignId>,
        split_align_id: Option<AlignId>,
    ) -> BrickId {
        let id = self.bricks.len();
        self.bricks.push(Brick {
            kind,
            inter,
            split,
            align_id,
            split_align_id,
            alignment: None,
            course: None,
            index: 0,
            converse: 0.,
            converse_span: 0.,
            pre_align_converse: 0.,
            ascent: 0.,
            descent: 0.,
            pad_before: 0.,
            attachments: vec![],
            alive: true,
        });
        self.brick_recalculate_size(id);
        self.brick_layout_properties_changed(id);
        return id;
    }

    fn brick_recalculate_size(&mut self, b: BrickId) {
        let unprintable = self.syntax.spec_root.unprintable.clone();
        match &mut self.bricks[b].kind {
            BrickKind::Text { text, style } => {
                let mut shown = String::with_capacity(text.len());
                for c in text.chars() {
                    if c.is_control() {
                        shown.push_str(&unprintable);
                    } else {
                        shown.push(c);
                    }
                }
                *text = shown;
                let style = self.syntax.syntax_style(*style);
                let font = self.measure.measure_metrics(&style.font);
                let width = self.measure.measure_width(&style.font, text);
                let brick = &mut self.bricks[b];
                brick.converse_span = width + style.padding.converse_start + style.padding.converse_end;
                brick.ascent = style.ascent.unwrap_or(font.ascent) + style.padding.transverse_start;
                brick.descent = style.descent.unwrap_or(font.descent) + style.padding.transverse_end;
                brick.pad_before = style.padding.converse_start;
            }
            BrickKind::Empty {
                ascent,
                descent,
                span,
            } => {
                let (a, d, s) = (*ascent, *descent, *span);
                let brick = &mut self.bricks[b];
                brick.converse_span = s;
                brick.ascent = a;
                brick.descent = d;
                brick.pad_before = 0.;
            }
        }
    }

    pub fn brick_set_text(&mut self, b: BrickId, text: String) {
        if let BrickKind::Text { text: t, .. } = &mut self.bricks[b].kind {
            *t = text;
        }
        self.brick_recalculate_size(b);
        self.brick_layout_properties_changed(b);
    }

    pub fn brick_text(&self, b: BrickId) -> &str {
        match &self.bricks[b].kind {
            BrickKind::Text { text, .. } => return text,
            BrickKind::Empty { .. } => return "",
        }
    }

    /// Whether the brick's atom is compact (merman `atomVisual().compact`).
    fn brick_owner_compact(&self, b: BrickId) -> bool {
        let leaf = self.bricks[b].inter.visual();
        let Some(atom) = self.visual_containing_atom(leaf) else {
            return false;
        };
        return self.visual_atom(atom).compact;
    }

    pub fn brick_is_split_with(&self, b: BrickId, compact: bool) -> bool {
        if let Line(_, i) = self.bricks[b].inter {
            if i > 0 {
                return true;
            }
        }
        match self.bricks[b].split {
            SpecSplit::Never => return false,
            SpecSplit::Compact => return compact,
            SpecSplit::Always => return true,
        }
    }

    pub fn brick_is_split(&self, b: BrickId) -> bool {
        return self.brick_is_split_with(b, self.brick_owner_compact(b));
    }

    pub fn brick_get_converse(&self, b: BrickId) -> f64 {
        return self.bricks[b].converse;
    }

    pub fn brick_converse_edge(&self, b: BrickId) -> f64 {
        return self.bricks[b].converse + self.bricks[b].converse_span;
    }

    /// Converse of the character at `index` within a text brick.
    pub fn brick_text_get_converse_offset(&mut self, b: BrickId, index: usize) -> f64 {
        let (text, style, pad) = match &self.bricks[b].kind {
            BrickKind::Text { text, style } => (text.clone(), *style, self.bricks[b].pad_before),
            BrickKind::Empty { .. } => return 0.,
        };
        let font = self.syntax.syntax_style(style).font.clone();
        let index = index.min(text.len());
        return pad + self.measure.measure_width(&font, &text[..index]);
    }

    /// Nearest character index to a point within a text brick.
    pub fn brick_text_get_under(&mut self, b: BrickId, point: Vector) -> usize {
        let (text, style) = match &self.bricks[b].kind {
            BrickKind::Text { text, style } => (text.clone(), *style),
            BrickKind::Empty { .. } => return 0,
        };
        let font = self.syntax.syntax_style(style).font.clone();
        let converse = point.converse - self.bricks[b].converse - self.bricks[b].pad_before;
        return crate::measure::measure_index_at_converse(&mut *self.measure, &font, &text, converse);
    }

    pub fn brick_set_converse(&mut self, b: BrickId, min_converse: f64, converse: f64) {
        let brick = &mut self.bricks[b];
        brick.pre_align_converse = min_converse;
        brick.converse = converse;
    }

    /// Call when a layout property of the brick has changed (size, alignment).
    pub fn brick_layout_properties_changed(&mut self, b: BrickId) {
        let alignment = if self.brick_is_split(b) {
            self.bricks[b].split_align_id
        } else {
            self.bricks[b].align_id
        };
        self.bricks[b].alignment = alignment;
        if let Some(c) = self.bricks[b].course {
            let index = self.bricks[b].index;
            self.course_changed(c, index);
        }
    }

    pub fn brick_add_after(&mut self, b: BrickId, brick: BrickId) {
        let c = self.bricks[b].course.expect("brick not in a course");
        let index = self.bricks[b].index;
        if self.brick_is_split(brick) {
            let next = self.course_break(c, index + 1);
            self.course_add(next, 0, vec![brick]);
        } else {
            self.course_add(c, index + 1, vec![brick]);
        }
    }

    pub fn brick_add_before(&mut self, b: BrickId, brick: BrickId) {
        let c = self.bricks[b].course.expect("brick not in a course");
        let index = self.bricks[b].index;
        if self.brick_is_split(b) {
            if self.courses[c].index == 0 {
                self.course_add(c, 0, vec![brick]);
                self.course_break(c, 1);
            } else {
                let previous = self.wall.children[self.courses[c].index - 1];
                let insert_index = self.courses[previous].children.len();
                self.course_add(previous, insert_index, vec![brick]);
                if self.brick_is_split(brick) {
                    self.course_break(previous, insert_index);
                }
            }
        } else if index > 0 && self.brick_is_split(brick) {
            let next = self.course_break(c, index);
            self.course_add(next, 0, vec![brick]);
        } else {
            self.course_add(c, index, vec![brick]);
        }
    }

    pub fn brick_previous(&self, b: BrickId) -> Option<BrickId> {
        let c = self.bricks[b].course?;
        let index = self.bricks[b].index;
        if index == 0 {
            let ci = self.courses[c].index;
            if ci == 0 {
                return None;
            }
            return self.courses[self.wall.children[ci - 1]].children.last().copied();
        }
        return Some(self.courses[c].children[index - 1]);
    }

    pub fn brick_next(&self, b: BrickId) -> Option<BrickId> {
        let c = self.bricks[b].course?;
        let index = self.bricks[b].index;
        if index + 1 == self.courses[c].children.len() {
            let ci = self.courses[c].index;
            if ci + 1 == self.wall.children.len() {
                return None;
            }
            return self.courses[self.wall.children[ci + 1]].children.first().copied();
        }
        return Some(self.courses[c].children[index + 1]);
    }

    pub fn brick_add_attachment(&mut self, b: BrickId, attachment: AttachmentRef) {
        self.bricks[b].attachments.push(attachment);
        let converse = self.brick_get_converse(b);
        self.attachment_set_converse(attachment, converse);
        if let Some(c) = self.bricks[b].course {
            let (start, ascent, descent) = {
                let course = &self.courses[c];
                (course.transverse_start, course.ascent, course.descent)
            };
            self.attachment_set_transverse(attachment, start);
            self.attachment_set_baseline_transverse(attachment, start + ascent);
            self.attachment_set_transverse_span(attachment, ascent, descent);
        }
    }

    pub fn brick_remove_attachment(&mut self, b: BrickId, attachment: AttachmentRef) {
        self.bricks[b].attachments.retain(|a| *a != attachment);
    }

    fn brick_destroyed(&mut self, b: BrickId) {
        let inter = self.bricks[b].inter;
        self.brick_inter_destroyed(inter);
        if let Some(a) = self.bricks[b].alignment {
            self.alignment_remove_brick(a, b);
        }
    }

    pub fn brick_destroy(&mut self, b: BrickId) {
        for a in self.bricks[b].attachments.clone() {
            self.attachment_destroy(a);
        }
        if let Some(c) = self.bricks[b].course {
            let index = self.bricks[b].index;
            self.course_remove_from_system(c, index);
        }
        self.brick_destroyed(b);
        self.bricks[b].alive = false;
    }

    // ---- Courses -----------------------------------------------------------

    pub fn course_new(&mut self, transverse_start: f64) -> CourseId {
        let id = self.courses.len();
        self.courses.push(Course {
            index: 0,
            transverse_start,
            ascent: 0.,
            descent: 0.,
            children: vec![],
            alignment: None,
            alignment_brick: None,
            last_expand_check_converse: 0.,
            idle_place: None,
            idle_compact: None,
            idle_expand: None,
            alive: true,
        });
        return id;
    }

    pub fn course_transverse_edge(&self, c: CourseId) -> f64 {
        let course = &self.courses[c];
        return course.transverse_start + course.ascent + course.descent;
    }

    pub fn course_transverse_span(&self, c: CourseId) -> f64 {
        return self.courses[c].ascent + self.courses[c].descent;
    }

    fn course_set_transverse(&mut self, c: CourseId, transverse: f64) {
        self.courses[c].transverse_start = transverse;
        let ascent = self.courses[c].ascent;
        for child in self.courses[c].children.clone() {
            for a in self.bricks[child].attachments.clone() {
                self.attachment_set_transverse(a, transverse);
                self.attachment_set_baseline_transverse(a, transverse + ascent);
            }
        }
    }

    pub fn course_changed(&mut self, c: CourseId, at: usize) {
        let brick = self.courses[c].children[at];
        if at > 0 && self.brick_is_split(brick) {
            self.course_break(c, at);
            return;
        } else if at == 0 && !self.brick_is_split(brick) && self.courses[c].index > 0 {
            self.course_join_previous(c);
            return;
        }
        let task = self.course_get_idle_place(c);
        if let Some(TaskKind::CoursePlace { changed, .. }) = self.task_kind_mut(task) {
            changed.insert(brick);
        }
    }

    fn course_join_previous(&mut self, c: CourseId) {
        for child in self.courses[c].children.clone() {
            if self.courses[c].alignment_brick == Some(child) {
                let a = self.courses[c].alignment.unwrap();
                self.alignment_remove_brick(a, child);
                self.courses[c].alignment = None;
                self.courses[c].alignment_brick = None;
            }
        }
        let reset_cornerstone = self.wall.cornerstone_course == Some(c);
        let previous = self.wall.children[self.courses[c].index - 1];
        let children = std::mem::take(&mut self.courses[c].children);
        let at = self.courses[previous].children.len();
        self.course_add(previous, at, children);
        self.course_destroy_inner(c);
        if reset_cornerstone {
            let cornerstone = self.wall.cornerstone;
            self.wall_set_cornerstone_existing(cornerstone);
        }
    }

    pub fn course_break(&mut self, c: CourseId, index: usize) -> CourseId {
        assert!(index != 0, "breaking course at 0");
        let mut reset_cornerstone = false;
        let next = self.course_new(self.courses[c].transverse_start + self.course_transverse_span(c));
        let ci = self.courses[c].index;
        self.wall_add(ci + 1, vec![next]);
        if index < self.courses[c].children.len() {
            let transplant: Vec<BrickId> = self.courses[c].children.drain(index..).collect();
            let task = self.course_get_idle_place(c);
            for brick in &transplant {
                if self.courses[c].alignment_brick == Some(*brick) {
                    let a = self.courses[c].alignment.unwrap();
                    self.alignment_remove_brick(a, *brick);
                    self.courses[c].alignment = None;
                    self.courses[c].alignment_brick = None;
                }
                let (ascent, descent) = (self.bricks[*brick].ascent, self.bricks[*brick].descent);
                if let Some(TaskKind::CoursePlace {
                    changed,
                    remove_max_ascent,
                    remove_max_descent,
                    ..
                }) = self.task_kind_mut(task)
                {
                    *remove_max_ascent = remove_max_ascent.max(ascent);
                    *remove_max_descent = remove_max_descent.max(descent);
                    changed.remove(brick);
                }
                if self.wall.cornerstone == Some(*brick) {
                    reset_cornerstone = true;
                }
            }
            self.course_add(next, 0, transplant);
        }
        if reset_cornerstone {
            let cornerstone = self.wall.cornerstone;
            self.wall_set_cornerstone_existing(cornerstone);
        }
        return next;
    }

    pub fn course_add(&mut self, c: CourseId, at: usize, bricks: Vec<BrickId>) {
        assert!(!bricks.is_empty(), "adding no bricks");
        let count = bricks.len();
        for (i, b) in bricks.iter().enumerate() {
            self.courses[c].children.insert(at + i, *b);
            self.bricks[*b].course = Some(c);
            self.bricks[*b].index = at + i;
        }
        self.course_renumber(c, at + count);
        let (start, ascent, descent) = {
            let course = &self.courses[c];
            (course.transverse_start, course.ascent, course.descent)
        };
        for b in &bricks {
            for a in self.bricks[*b].attachments.clone() {
                self.attachment_set_transverse(a, start);
                self.attachment_set_baseline_transverse(a, start + ascent);
                self.attachment_set_transverse_span(a, ascent, descent);
            }
        }
        let task = self.course_get_idle_place(c);
        if let Some(TaskKind::CoursePlace { changed, .. }) = self.task_kind_mut(task) {
            changed.extend(bricks);
        }
    }

    fn course_remove_from_system(&mut self, c: CourseId, at: usize) {
        let brick = self.courses[c].children[at];
        if self.courses[c].alignment_brick == Some(brick) {
            let a = self.courses[c].alignment.unwrap();
            self.alignment_remove_brick(a, brick);
            self.courses[c].alignment = None;
            self.courses[c].alignment_brick = None;
        }
        if self.wall.cornerstone == Some(brick) {
            self.wall.cornerstone = None;
        }
        if self.hover_brick == Some(brick) {
            self.clear_hover();
        }
        self.bricks[brick].course = None;
        self.bricks[brick].index = 0;
        self.courses[c].children.remove(at);
        let ci = self.courses[c].index;
        if ci >= 1 {
            let prev = self.wall.children[ci - 1];
            self.course_get_idle_expand(prev);
        }
        if ci + 1 < self.wall.children.len() {
            let next = self.wall.children[ci + 1];
            self.course_get_idle_expand(next);
        }
        if self.courses[c].children.is_empty() {
            self.course_destroy_inner(c);
        } else if at == 0 && ci > 0 {
            self.course_join_previous(c);
        } else {
            self.course_renumber(c, at);
            let (ascent, descent) = (self.bricks[brick].ascent, self.bricks[brick].descent);
            let task = self.course_get_idle_place(c);
            if let Some(TaskKind::CoursePlace {
                changed,
                remove_max_ascent,
                remove_max_descent,
                ..
            }) = self.task_kind_mut(task)
            {
                *remove_max_ascent = remove_max_ascent.max(ascent);
                *remove_max_descent = remove_max_descent.max(descent);
                changed.remove(&brick);
            }
        }
    }

    fn course_destroy_inner(&mut self, c: CourseId) {
        for t in [
            self.courses[c].idle_place,
            self.courses[c].idle_compact,
            self.courses[c].idle_expand,
        ]
        .into_iter()
        .flatten()
        {
            self.task_destroy(t);
        }
        let index = self.courses[c].index;
        self.wall_remove(index);
        self.courses[c].alive = false;
    }

    pub fn course_destroy(&mut self, c: CourseId) {
        while let Some(last) = self.courses[c].children.last().copied() {
            self.brick_destroy(last);
        }
    }

    fn course_renumber(&mut self, c: CourseId, at: usize) {
        for i in at..self.courses[c].children.len() {
            let b = self.courses[c].children[i];
            self.bricks[b].index = i;
        }
    }

    fn course_get_idle_place(&mut self, c: CourseId) -> TaskId {
        if let Some(t) = self.courses[c].idle_place {
            return t;
        }
        let t = self.add_iteration(
            TaskKind::CoursePlace {
                course: c,
                changed: HashSet::new(),
                remove_max_ascent: 0.,
                remove_max_descent: 0.,
            },
            P::COURSE_PLACE,
        );
        self.courses[c].idle_place = Some(t);
        return t;
    }

    fn course_get_idle_compact(&mut self, c: CourseId) {
        if self.courses[c].idle_compact.is_some() {
            return;
        }
        let t = self.add_iteration(
            TaskKind::CourseCompact {
                course: c,
                skip: HashSet::new(),
                skip_primitives: HashSet::new(),
            },
            P::COURSE_COMPACT,
        );
        self.courses[c].idle_compact = Some(t);
    }

    pub fn course_get_idle_expand(&mut self, c: CourseId) {
        if self.courses[c].idle_expand.is_some() {
            return;
        }
        let t = self.add_iteration(TaskKind::CourseExpand { course: c }, P::COURSE_EXPAND);
        self.courses[c].idle_expand = Some(t);
    }

    /// When adjusting bricks in a course, calculate the state after the current
    /// brick and return (converse, min converse without alignment).
    fn calculate_next_brick_advance(&self, calc: &mut CalcCourseConverse, brick: BrickId) -> (f64, f64) {
        let mut out = calc.converse;
        let out1 = calc.pre_align_converse;
        let b = &self.bricks[brick];
        if calc.alignment.is_none() {
            if let Some(a) = b.alignment {
                calc.alignment = Some(a);
                if self.aligns[a].converse > out {
                    out = self.aligns[a].converse;
                }
            }
        }
        calc.pre_align_converse += b.converse_span;
        calc.converse = out + b.converse_span;
        return (out, out1);
    }

    /// Merman `Course.IterationPlaceTask`.
    pub fn run_course_place(&mut self, task: TaskId, c: CourseId) -> bool {
        let (changed, remove_max_ascent, remove_max_descent) = match self.task_kind_mut(task) {
            Some(TaskKind::CoursePlace {
                changed,
                remove_max_ascent,
                remove_max_descent,
                ..
            }) => (changed.clone(), *remove_max_ascent, *remove_max_descent),
            _ => unreachable!(),
        };
        // Update transverse space
        let mut new_ascent = false;
        let mut new_descent = false;
        for brick in &changed {
            if self.bricks[*brick].ascent > self.courses[c].ascent {
                self.courses[c].ascent = self.bricks[*brick].ascent;
                new_ascent = true;
            }
            if self.bricks[*brick].descent > self.courses[c].descent {
                self.courses[c].descent = self.bricks[*brick].descent;
                new_descent = true;
            }
        }
        if !(new_ascent && new_descent)
            && remove_max_ascent == self.courses[c].ascent
            && remove_max_descent == self.courses[c].descent
        {
            let mut ascent: f64 = 0.;
            let mut descent: f64 = 0.;
            for brick in &self.courses[c].children {
                ascent = ascent.max(self.bricks[*brick].ascent);
                descent = descent.max(self.bricks[*brick].descent);
            }
            self.courses[c].ascent = ascent;
            self.courses[c].descent = descent;
            new_ascent = true;
            new_descent = true;
        }
        let (ascent, descent) = (self.courses[c].ascent, self.courses[c].descent);
        let targets: Vec<BrickId> = if new_ascent || new_descent {
            self.courses[c].children.clone()
        } else {
            changed.iter().copied().collect()
        };
        for b in targets {
            for a in self.bricks[b].attachments.clone() {
                self.attachment_set_transverse_span(a, ascent, descent);
            }
        }
        // Do converse placement
        let mut calc = CalcCourseConverse::default();
        for index in 0..self.courses[c].children.len() {
            let brick = self.courses[c].children[index];
            if calc.alignment.is_none() {
                if let Some(a) = self.bricks[brick].alignment {
                    if let Some(old) = self.courses[c].alignment {
                        let old_brick = self.courses[c].alignment_brick.unwrap();
                        self.alignment_remove_brick(old, old_brick);
                    }
                    self.courses[c].alignment = Some(a);
                    self.courses[c].alignment_brick = Some(brick);
                    self.alignment_add_brick(a, brick);
                    self.alignment_feedback(a, calc.converse);
                }
            }
            let (converse, min_converse) = self.calculate_next_brick_advance(&mut calc, brick);
            self.brick_set_converse(brick, min_converse, converse);
            for a in self.bricks[brick].attachments.clone() {
                self.attachment_set_converse(a, converse);
            }
        }
        if calc.alignment.is_none() {
            if let Some(a) = self.courses[c].alignment {
                let old_brick = self.courses[c].alignment_brick.unwrap();
                self.alignment_remove_brick(a, old_brick);
            }
        }
        if calc.converse > self.edge {
            self.course_get_idle_compact(c);
        }
        if calc.converse * self.config.retry_expand_factor < self.courses[c].last_expand_check_converse {
            self.course_get_idle_expand(c);
        }
        if calc.converse > self.courses[c].last_expand_check_converse {
            self.courses[c].last_expand_check_converse = calc.converse;
        }
        // Propagate changes up
        if new_ascent || new_descent {
            let index = self.courses[c].index;
            self.wall_adjust(index);
        }
        return false;
    }

    /// Merman's `compactComparator` as a key: greater `spacePriority`
    /// (-precedence) first, then lesser depth score.
    fn compact_key(&self, a: VisualId) -> (i64, i64) {
        let va = self.visual_atom(a);
        let precedence = self.syntax.syntax_type(va.type_).precedence;
        return (precedence, va.depth_score);
    }

    /// Whether `a` is compacted strictly before `b`: lower precedence first,
    /// then shallower.
    fn compact_before(&self, a: VisualId, b: VisualId) -> bool {
        return self.compact_key(a) < self.compact_key(b);
    }

    /// Merman's inclusive `isOrdered(expandComparator, a, b)`: `a` expands
    /// before or together with `b` (expansion order is the reverse of
    /// compaction order).
    fn expand_ordered(&self, a: VisualId, b: VisualId) -> bool {
        return self.compact_key(a) >= self.compact_key(b);
    }

    /// Whether `a` expands strictly before `b`.
    fn expand_before(&self, a: VisualId, b: VisualId) -> bool {
        return self.compact_key(a) > self.compact_key(b);
    }

    /// One step of merman's `Course.IterationCompactTask`. Returns whether to
    /// run again.
    pub fn course_compact_step(
        &mut self,
        c: CourseId,
        skip: &mut HashSet<VisualId>,
        skip_primitives: &mut HashSet<VisualId>,
    ) -> bool {
        // Find highest priority brick in this course
        let mut priorities: Vec<VisualId> = vec![];
        let mut last_primitive: Option<VisualId> = None;
        let mut converse = 0.;
        for index in 0..self.courses[c].children.len() {
            let brick = self.courses[c].children[index];
            let leaf = self.bricks[brick].inter.visual();
            let Some(atom_visual) = self.visual_containing_atom(leaf) else {
                continue;
            };
            if let VisualKind::Primitive(_) = &self.visuals[leaf].kind {
                if !skip_primitives.contains(&leaf) {
                    last_primitive = Some(leaf);
                }
            }
            if !self.visual_atom(atom_visual).compact && !skip.contains(&atom_visual) {
                priorities.push(atom_visual);
            }
            converse = self.brick_converse_edge(brick);
            if !priorities.is_empty() && converse > self.edge {
                break;
            }
        }
        if converse <= self.edge {
            return false;
        }
        if priorities.is_empty() {
            match last_primitive {
                Some(p) => {
                    self.primitive_reflow(p);
                    skip_primitives.insert(p);
                    return true;
                }
                None => return false,
            }
        }
        let mut top = priorities[0];
        for a in priorities.iter().skip(1) {
            if self.compact_before(*a, top) {
                top = *a;
            }
        }
        self.visual_compact(top);
        skip.insert(top);
        return true;
    }

    pub fn run_course_compact(&mut self, task: TaskId, c: CourseId) -> bool {
        let (mut skip, mut skip_primitives) = match self.task_kind_mut(task) {
            Some(TaskKind::CourseCompact {
                skip,
                skip_primitives,
                ..
            }) => (std::mem::take(skip), std::mem::take(skip_primitives)),
            _ => unreachable!(),
        };
        let out = self.course_compact_step(c, &mut skip, &mut skip_primitives);
        if let Some(TaskKind::CourseCompact {
            skip: s,
            skip_primitives: sp,
            ..
        }) = self.task_kind_mut(task)
        {
            *s = skip;
            *sp = skip_primitives;
        }
        return out;
    }

    /// Merman's `Course.IterationExpandTask.expand`.
    pub fn course_expand_step(&mut self, c: CourseId, iteration: &mut IterationContext) -> bool {
        // Unwrap nodes based on priority: find next atom that can be expanded
        let mut top: Option<VisualId> = None;
        for index in 0..self.courses[c].children.len() {
            let brick = self.courses[c].children[index];
            let leaf = self.bricks[brick].inter.visual();
            let Some(atom_visual) = self.visual_containing_atom(leaf) else {
                continue;
            };
            if self.visual_atom(atom_visual).compact {
                match top {
                    Some(t) if !self.expand_before(atom_visual, t) => {}
                    _ => top = Some(atom_visual),
                }
            }
        }
        let Some(top) = top else {
            return false;
        };
        // Check that all parents are either expanded or have lower expand priority
        {
            let mut parent_atom = top;
            while let Some(p) = self.visual_containing_atom(parent_atom) {
                parent_atom = p;
                if self.expand_ordered(parent_atom, top) && self.visual_atom(parent_atom).compact {
                    return false;
                }
            }
        }
        // 1. Check that this has the highest priority on all shared courses + courses with child
        // bricks
        // 2. Make sure any soft-wrapped own/child bricks are un-soft-wrapped before we do deeper
        // expanding
        {
            let Some(first) = self.visual_get_first_brick(top) else {
                return false;
            };
            let Some(last) = self.visual_get_last_brick(top) else {
                return false;
            };
            let (Some(fc), Some(lc)) = (self.bricks[first].course, self.bricks[last].course) else {
                return false;
            };
            let (fi, li) = (self.courses[fc].index, self.courses[lc].index);
            for course_i in fi..=li {
                let course = self.wall.children[course_i];
                for brick in self.courses[course].children.clone() {
                    let leaf = self.bricks[brick].inter.visual();
                    // Make sure not soft wrapped
                    if let VisualKind::Primitive(_) = &self.visuals[leaf].kind {
                        if self.primitive_soft_wrapped(leaf) {
                            let old_lines = self.visual_primitive(leaf).lines.len();
                            self.primitive_reflow(leaf);
                            let new_lines = self.visual_primitive(leaf).lines.len();
                            return old_lines != new_lines;
                        }
                    }
                    // Make sure priority
                    let Some(atom) = self.visual_containing_atom(leaf) else {
                        continue;
                    };
                    if !self.visual_atom(atom).compact {
                        continue;
                    }
                    if atom == top {
                        continue;
                    }
                    if self.expand_ordered(top, atom) {
                        continue;
                    }
                    return false;
                }
            }
        }
        let mut leaf_bricks = vec![];
        // Check if we actually can expand
        {
            self.visual_get_leaf_bricks(top, &mut leaf_bricks);
            let mut course: Option<CourseId> = None;
            let mut course_last_brick = 0;
            let mut course_calc = CalcCourseConverse::default();
            for brick in &leaf_bricks {
                let Some(bc) = self.bricks[*brick].course else {
                    continue;
                };
                // Line changed (or first brick)
                if Some(bc) != course {
                    // If jumping to a new non-consecutive line and unbreaking, reset to calculate
                    // previous line
                    let unsplit = self.bricks[*brick].index == 0
                        && !self.brick_is_split_with(*brick, false)
                        && self.courses[bc].index > 0;
                    if unsplit {
                        if course.is_some() && self.courses[bc].index - 1 == self.courses[course.unwrap()].index {
                            // On next line, continue calculation
                        } else {
                            // Reset to previous line to calculate unsplit
                            course_calc = CalcCourseConverse::default();
                            course = Some(self.wall.children[self.courses[bc].index - 1]);
                            course_last_brick = 0;
                        }
                    }
                    // Finish previous line, see if it goes over
                    if let Some(cc) = course {
                        while course_last_brick < self.courses[cc].children.len() {
                            let b1 = self.courses[cc].children[course_last_brick];
                            self.calculate_next_brick_advance(&mut course_calc, b1);
                            if course_calc.converse > self.edge {
                                return false;
                            }
                            course_last_brick += 1;
                        }
                    }
                    course_last_brick = 0;
                    course = Some(bc);
                    if !unsplit {
                        course_calc = CalcCourseConverse::default();
                    }
                }
                // Sum bricks leading up to modified brick
                let cc = course.unwrap();
                while course_last_brick < self.bricks[*brick].index {
                    let b1 = self.courses[cc].children[course_last_brick];
                    self.calculate_next_brick_advance(&mut course_calc, b1);
                    if course_calc.converse > self.edge {
                        return false;
                    }
                    course_last_brick += 1;
                }
                // Sum modified brick
                self.calculate_next_brick_advance(&mut course_calc, *brick);
                if course_calc.converse > self.edge {
                    return false;
                }
                course_last_brick += 1;
            }
            // Finish final line
            if let Some(cc) = course {
                while course_last_brick < self.courses[cc].children.len() {
                    let b1 = self.courses[cc].children[course_last_brick];
                    self.calculate_next_brick_advance(&mut course_calc, b1);
                    if course_calc.converse > self.edge {
                        return false;
                    }
                    course_last_brick += 1;
                }
            }
        }
        // Avoid bouncing
        if !iteration.expanded.insert(top) {
            return false;
        }
        // Expand
        self.visual_expand(top);
        self.courses[c].last_expand_check_converse = 0.;
        return true;
    }

    // ---- Wall --------------------------------------------------------------

    pub fn wall_clear(&mut self) {
        while let Some(last) = self.wall.children.last().copied() {
            self.course_destroy(last);
        }
        for t in [self.wall.idle_compact, self.wall.idle_expand, self.wall.idle_adjust]
            .into_iter()
            .flatten()
        {
            self.task_destroy(t);
        }
    }

    fn wall_renumber(&mut self, at: usize) {
        for i in at..self.wall.children.len() {
            let c = self.wall.children[i];
            self.courses[c].index = i;
        }
    }

    fn wall_ensure_idle_adjust(&mut self) -> TaskId {
        if let Some(t) = self.wall.idle_adjust {
            return t;
        }
        let t = self.add_iteration(
            TaskKind::WallAdjust {
                forward: usize::MAX,
                backward: isize::MIN,
            },
            P::WALL_ADJUST,
        );
        self.wall.idle_adjust = Some(t);
        return t;
    }

    fn wall_add(&mut self, at: usize, courses: Vec<CourseId>) {
        for (i, c) in courses.iter().enumerate() {
            self.wall.children.insert(at + i, *c);
        }
        self.wall_renumber(at);
        let task = self.wall_ensure_idle_adjust();
        if self.wall.children.len() > 1 {
            if let Some(TaskKind::WallAdjust { forward, backward }) = self.task_kind_mut(task) {
                if *backward >= at as isize {
                    *backward += 1;
                }
                if *forward >= at && *forward < usize::MAX {
                    *forward += 1;
                }
            }
            self.wall_adjust_at(task, at);
        }
    }

    fn wall_remove(&mut self, at: usize) {
        if let Some(cc) = self.wall.cornerstone_course {
            if self.courses[cc].index == at {
                let transverse_span = self.course_transverse_span(cc);
                self.wall.cornerstone_course = None;
                // When removing cornerstone course, immediately move all following courses to fill
                // gap. Otherwise the next course might become the cornerstone causing an
                // aesthetically displeasing transverse walk.
                for at2 in at..self.wall.children.len() {
                    let following = self.wall.children[at2];
                    let t = self.courses[following].transverse_start - transverse_span;
                    self.course_set_transverse(following, t);
                }
            }
        }
        self.wall.children.remove(at);
        if at < self.wall.children.len() {
            self.wall_renumber(at);
            if self.wall.cornerstone_course.is_some() {
                let task = self.wall_ensure_idle_adjust();
                if let Some(TaskKind::WallAdjust { forward, backward }) = self.task_kind_mut(task) {
                    if (at as isize) < *backward {
                        *backward -= 1;
                    }
                    if at < *forward && *forward < usize::MAX {
                        *forward -= 1;
                    }
                }
                self.wall_adjust_at(task, at);
            }
        }
    }

    pub fn wall_idle_compact(&mut self) {
        match self.wall.idle_compact {
            Some(t) => {
                if let Some(TaskKind::WallCompact { at, .. }) = self.task_kind_mut(t) {
                    *at = 0;
                }
            }
            None => {
                let t = self.add_iteration(
                    TaskKind::WallCompact {
                        at: 0,
                        compact: None,
                    },
                    P::WALL_COMPACT,
                );
                self.wall.idle_compact = Some(t);
            }
        }
    }

    pub fn wall_idle_expand(&mut self) {
        match self.wall.idle_expand {
            Some(t) => {
                if let Some(TaskKind::WallExpand { at, .. }) = self.task_kind_mut(t) {
                    *at = 0;
                }
            }
            None => {
                let t = self.add_iteration(
                    TaskKind::WallExpand {
                        at: 0,
                        expanding: None,
                    },
                    P::WALL_EXPAND,
                );
                self.wall.idle_expand = Some(t);
            }
        }
    }

    /// Merman's converse edge listener on the wall.
    pub fn wall_converse_edge_changed(&mut self, _old: f64, new: f64) {
        if new < self.wall.mod_old_edge {
            self.wall_idle_compact();
            self.wall.mod_old_edge = new;
        } else if new > self.wall.mod_old_edge * self.config.retry_expand_factor {
            self.wall_idle_expand();
            self.wall.mod_old_edge = new;
        }
    }

    /// Re-set the cornerstone after courses moved (merman `setCornerstone(cornerstone, null, null)`).
    fn wall_set_cornerstone_existing(&mut self, cornerstone: Option<BrickId>) {
        match cornerstone {
            Some(b) => self.wall_set_cornerstone(b, None, None),
            None => {
                self.wall.cornerstone = None;
                self.wall.cornerstone_course = None;
            }
        }
    }

    /// Place `cornerstone` (a brick, possibly not yet in a course) and lay
    /// bricks outward from it. `find_previous`/`find_next` are bricks to
    /// attach next to if the cornerstone isn't laid yet.
    pub fn wall_set_cornerstone(
        &mut self,
        cornerstone: BrickId,
        find_previous: Option<BrickId>,
        find_next: Option<BrickId>,
    ) {
        self.wall.cornerstone = Some(cornerstone);
        if self.bricks[cornerstone].course.is_none() {
            if let Some(found) = find_previous {
                self.brick_add_after(found, cornerstone);
            } else if let Some(found) = find_next {
                self.brick_add_before(found, cornerstone);
            } else {
                self.wall_clear();
                let course = self.course_new(0.);
                self.wall_add(0, vec![course]);
                self.course_add(course, 0, vec![cornerstone]);
            }
        }
        let course = self.bricks[cornerstone].course.unwrap();
        self.wall.cornerstone_course = Some(course);
        let index = self.courses[course].index;
        self.wall_adjust(index);
        self.cornerstone_changed(cornerstone);
        self.trigger_idle_lay_bricks_before_start(cornerstone);
        self.trigger_idle_lay_bricks_after_end(cornerstone);
    }

    pub fn wall_adjust(&mut self, at: usize) {
        let task = self.wall_ensure_idle_adjust();
        self.wall_adjust_at(task, at);
    }

    fn wall_adjust_at(&mut self, task: TaskId, at: usize) {
        let Some(cc) = self.wall.cornerstone_course else {
            return;
        };
        let ci = self.courses[cc].index;
        if let Some(TaskKind::WallAdjust { forward, backward }) = self.task_kind_mut(task) {
            if at <= ci && (at as isize) > *backward {
                *backward = isize::min(ci as isize - 1, at as isize);
            }
            if at >= ci && at < *forward {
                *forward = usize::max(ci + 1, at);
            }
        }
    }

    /// Merman `Wall.IterationAdjustTask`: place courses outward from the cornerstone.
    pub fn run_wall_adjust(&mut self, task: TaskId) -> bool {
        let Some(cc) = self.wall.cornerstone_course else {
            return false;
        };
        let ci = self.courses[cc].index;
        let (mut forward, mut backward) = match self.task_kind_mut(task) {
            Some(TaskKind::WallAdjust { forward, backward }) => (*forward, *backward),
            _ => unreachable!(),
        };
        let mut modified = false;
        if ci as isize <= backward || ci >= forward {
            backward = ci as isize - 1;
            forward = ci + 1;
        }
        let stride = self.syntax.spec_root.course_transverse_stride;
        if backward >= 0 {
            // Always < children size because of cornerstone
            let child = self.wall.children[backward as usize];
            let preceding = self.wall.children[backward as usize + 1];
            let transverse = self.courses[preceding].transverse_start
                - if stride == 0. {
                    self.course_transverse_span(child)
                } else {
                    stride
                };
            self.course_set_transverse(child, transverse);
            backward -= 1;
            modified = true;
        }
        if forward < self.wall.children.len() {
            // Always > 0 because of cornerstone
            let preceding = self.wall.children[forward - 1];
            let transverse = self.courses[preceding].transverse_start
                + if stride == 0. {
                    self.course_transverse_span(preceding)
                } else {
                    stride
                };
            let child = self.wall.children[forward];
            self.course_set_transverse(child, transverse);
            forward += 1;
            modified = true;
        }
        if let Some(TaskKind::WallAdjust {
            forward: f,
            backward: b,
        }) = self.task_kind_mut(task)
        {
            *f = forward;
            *b = backward;
        }
        return modified;
    }

    /// Merman `Wall.IterationCompactTask`: run a compact task on each course in turn.
    pub fn run_wall_compact(&mut self, task: TaskId) -> bool {
        let (at, compact) = match self.task_kind_mut(task) {
            Some(TaskKind::WallCompact { at, compact }) => (*at, compact.take()),
            _ => unreachable!(),
        };
        if at >= self.wall.children.len() {
            return false;
        }
        let (mut skip, mut skip_primitives) = compact.unwrap_or_default();
        let course = self.wall.children[at];
        let again = self.course_compact_step(course, &mut skip, &mut skip_primitives);
        if let Some(TaskKind::WallCompact { at: a, compact: c }) = self.task_kind_mut(task) {
            if again {
                *c = Some((skip, skip_primitives));
            } else {
                *c = None;
                *a += 1;
            }
        }
        return true;
    }

    /// Merman `Wall.IterationExpandTask`.
    pub fn run_wall_expand(&mut self, task: TaskId, iteration: &mut IterationContext) -> bool {
        let (at, expanding) = match self.task_kind_mut(task) {
            Some(TaskKind::WallExpand { at, expanding }) => (*at, *expanding),
            _ => unreachable!(),
        };
        if at >= self.wall.children.len() {
            return false;
        }
        let mut at = at;
        let course = match expanding {
            Some(c) => c,
            None => {
                let c = self.wall.children[at];
                at += 1;
                c
            }
        };
        let old_course_count = self.wall.children.len();
        let again = self.courses[course].alive && self.course_expand_step(course, iteration);
        if old_course_count != self.wall.children.len() {
            at = at.saturating_sub(2);
        }
        if let Some(TaskKind::WallExpand { at: a, expanding: e }) = self.task_kind_mut(task) {
            *a = at;
            *e = if again { Some(course) } else { None };
        }
        return true;
    }

    /// Transverse extent of the laid courses: (start of first, edge of last).
    pub fn wall_usage(&self) -> Option<(f64, f64)> {
        let first = *self.wall.children.first()?;
        let last = *self.wall.children.last()?;
        return Some((self.courses[first].transverse_start, self.course_transverse_edge(last)));
    }
}
