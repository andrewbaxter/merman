use {
    crate::{
        attachment::AttachmentRef,
        context::{
            AlignId,
            BrickId,
            Context,
            CourseId,
            TaskId,
            Vector,
            VisualId,
        },
        display::DisplayNodeId,
        iteration::{
            IterationContext,
            P,
            TaskKind,
        },
        spec::SpecSplit,
        syntax::StyleId,
        visual::VisualKind,
    },
    std::collections::HashSet,
};

pub struct Bedding {
    pub after: f64,
    pub before: f64,
}

pub struct Brick {
    pub align_id: Option<AlignId>,
    pub alignment: Option<AlignId>,
    pub alive: bool,
    pub ascent: f64,
    pub attachments: Vec<AttachmentRef>,
    pub converse: f64,
    pub converse_span: f64,
    pub course: Option<CourseId>,
    pub descent: f64,
    pub index: usize,
    pub inter: BrickInter,
    pub kind: BrickKind,
    pub node: DisplayNodeId,
    pub pad_before: f64,
    pub pre_align_converse: f64,
    pub split: SpecSplit,
    pub split_align_id: Option<AlignId>,
}

pub struct BrickEmpty {
    pub ascent: f64,
    pub descent: f64,
    pub span: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrickInter {
    ArrayEmpty(VisualId),
    FieldAtomEllipsis(VisualId),
    Line(VisualId, usize),
    Symbol(VisualId),
}

impl BrickInter {
    pub fn visual(&self) -> VisualId {
        match self {
            BrickInter::Symbol(v) |
            BrickInter::Line(v, _) |
            BrickInter::ArrayEmpty(v) |
            BrickInter::FieldAtomEllipsis(v) => return *v,
        }
    }
}

pub enum BrickKind {
    Empty(BrickEmpty),
    Line(BrickText, usize),
    Text(BrickText),
}

impl BrickKind {
    pub fn brick_kind_text(&self) -> Option<&BrickText> {
        match self {
            BrickKind::Text(t) | BrickKind::Line(t, _) => return Some(t),
            BrickKind::Empty(_) => return None,
        }
    }

    fn brick_kind_text_mut(&mut self) -> Option<&mut BrickText> {
        match self {
            BrickKind::Text(t) | BrickKind::Line(t, _) => return Some(t),
            BrickKind::Empty(_) => return None,
        }
    }
}

pub struct BrickText {
    pub style: StyleId,
    pub text: String,
}

#[derive(Default)]
struct CalcCourseConverse {
    alignment: Option<AlignId>,
    converse: f64,
    pre_align_converse: f64,
}

pub struct Course {
    pub alignment: Option<AlignId>,
    pub alignment_brick: Option<BrickId>,
    pub alive: bool,
    pub ascent: f64,
    pub children: Vec<BrickId>,
    pub descent: f64,
    pub group: DisplayNodeId,
    pub idle_compact: Option<TaskId>,
    pub idle_expand: Option<TaskId>,
    pub idle_place: Option<TaskId>,
    pub index: usize,
    pub last_expand_check_converse: f64,
    pub transverse_start: f64,
}

#[derive(Default)]
pub struct Wall {
    pub bedding: Vec<Option<Bedding>>,
    pub bedding_after: f64,
    pub bedding_before: f64,
    pub children: Vec<CourseId>,
    pub cornerstone: Option<BrickId>,
    pub cornerstone_course: Option<CourseId>,
    pub idle_adjust: Option<TaskId>,
    pub idle_compact: Option<TaskId>,
    pub idle_cull: Option<TaskId>,
    pub idle_expand: Option<TaskId>,
    pub mod_old_edge: f64,
}

impl Context {
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

    pub fn brick_converse_edge(&self, b: BrickId) -> f64 {
        return self.bricks[b].converse + self.bricks[b].converse_span;
    }

    pub fn brick_destroy(&mut self, b: BrickId) {
        self.cursor_brick_destroying(b);
        for a in self.bricks[b].attachments.clone() {
            self.attachment_destroy(a);
        }
        if let Some(c) = self.bricks[b].course {
            let at = self.bricks[b].index;
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
            let group = self.courses[c].group;
            self.display.group_remove(group, at, 1);
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
                if let Some(TaskKind::CoursePlace { changed, remove_max_ascent, remove_max_descent, .. }) =
                    self.task_kind_mut(task) {
                    *remove_max_ascent = remove_max_ascent.max(ascent);
                    *remove_max_descent = remove_max_descent.max(descent);
                    changed.remove(&brick);
                }
            }
        }
        {
            let inter = self.bricks[b].inter;
            self.brick_inter_destroyed(inter);
            if let Some(a) = self.bricks[b].alignment {
                self.alignment_remove_brick(a, b);
            }
        }
        let node = self.bricks[b].node;
        self.display.display_destroy(node);
        self.bricks[b].alive = false;
    }

    pub fn brick_get_converse(&self, b: BrickId) -> f64 {
        return self.bricks[b].converse;
    }

    pub fn brick_is_split(&self, b: BrickId) -> bool {
        let compact = 'brick_owner_compact: {
            let leaf = self.bricks[b].inter.visual();
            let Some(atom) = self.visual_containing_atom(leaf) else {
                break 'brick_owner_compact false;
            };
            self.visual_atom(atom).compact
        };
        return self.brick_is_split_with(b, compact);
    }

    pub fn brick_is_split_with(&self, b: BrickId, compact: bool) -> bool {
        if let BrickKind::Line(_, i) = self.bricks[b].kind {
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

    pub fn brick_new(
        &mut self,
        kind: BrickKind,
        inter: BrickInter,
        split: SpecSplit,
        align_id: Option<AlignId>,
        split_align_id: Option<AlignId>,
    ) -> BrickId {
        let id = self.bricks.len();
        let node = match &kind {
            BrickKind::Text(_) | BrickKind::Line(_, _) => self.display.display_text(),
            BrickKind::Empty(_) => self.display.display_blank(),
        };
        self.bricks.push(Brick {
            node: node,
            kind: kind,
            inter: inter,
            split: split,
            align_id: align_id,
            split_align_id: split_align_id,
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

    fn brick_recalculate_size(&mut self, b: BrickId) {
        let unprintable = self.syntax.spec_root.unprintable.clone();
        let node = self.bricks[b].node;
        let style_id = match self.bricks[b].kind.brick_kind_text_mut() {
            Some(t) => {
                let mut shown = String::with_capacity(t.text.len());
                for c in t.text.chars() {
                    if c.is_control() {
                        shown.push_str(&unprintable);
                    } else {
                        shown.push(c);
                    }
                }
                t.text = shown;
                Some(t.style)
            },
            None => None,
        };
        match style_id {
            Some(style_id) => {
                let stylist = self.stylist.clone();
                let style = stylist.style_text(style_id);
                let text = self.brick_text(b).to_string();
                let font = self.display.display_font_metrics(&style.font);
                let width = self.display.display_font_width(&style.font, &text);
                let brick = &mut self.bricks[b];
                brick.converse_span = width + style.padding.converse_start + style.padding.converse_end;
                brick.ascent = style.ascent.unwrap_or(font.ascent) + style.padding.transverse_start;
                brick.descent = style.descent.unwrap_or(font.descent) + style.padding.transverse_end;
                brick.pad_before = style.padding.converse_start;
                let (ascent, descent) = (self.bricks[b].ascent, self.bricks[b].descent);
                self.display.text_set(node, &text, &style.font, &style.color);
                self.display.node_set_span(node, width, ascent, descent);
            },
            None => {
                let (a, d, s) = match &self.bricks[b].kind {
                    BrickKind::Empty(e) => (e.ascent, e.descent, e.span),
                    BrickKind::Text(_) | BrickKind::Line(_, _) => unreachable!(),
                };
                let brick = &mut self.bricks[b];
                brick.converse_span = s;
                brick.ascent = a;
                brick.descent = d;
                brick.pad_before = 0.;
                self.display.node_set_span(node, s, a, d);
            },
        }
    }

    pub fn brick_remove_attachment(&mut self, b: BrickId, attachment: AttachmentRef) {
        self.bricks[b].attachments.retain(|a| *a != attachment);
    }

    pub fn brick_set_converse(&mut self, b: BrickId, min_converse: f64, converse: f64) {
        let brick = &mut self.bricks[b];
        brick.pre_align_converse = min_converse;
        brick.converse = converse;
        let (node, pad_before) = (brick.node, brick.pad_before);
        self.display.node_set_converse(node, converse + pad_before, false);
    }

    pub fn brick_set_text(&mut self, b: BrickId, text: String) {
        if let Some(t) = self.bricks[b].kind.brick_kind_text_mut() {
            t.text = text;
        }
        self.brick_recalculate_size(b);
        self.brick_layout_properties_changed(b);
    }

    pub fn brick_text(&self, b: BrickId) -> &str {
        match self.bricks[b].kind.brick_kind_text() {
            Some(t) => return &t.text,
            None => return "",
        }
    }

    pub fn brick_text_get_converse_offset(&mut self, b: BrickId, index: usize) -> f64 {
        let (text, style, pad) = match self.bricks[b].kind.brick_kind_text() {
            Some(t) => (t.text.clone(), t.style, self.bricks[b].pad_before),
            None => return 0.,
        };
        let font = self.syntax.syntax_style(style).font.clone();
        let index = index.min(text.len());
        return pad + self.display.display_font_width(&font, &text[..index]);
    }

    pub fn brick_text_get_under(&mut self, b: BrickId, point: Vector) -> usize {
        let (text, style) = match self.bricks[b].kind.brick_kind_text() {
            Some(t) => (t.text.clone(), t.style),
            None => return 0,
        };
        let font = self.syntax.syntax_style(style).font.clone();
        let converse = point.converse - self.bricks[b].converse - self.bricks[b].pad_before;
        return self.display.display_index_at_converse(&font, &text, converse);
    }

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

    fn compact_key(&self, a: VisualId) -> (i64, i64) {
        let va = self.visual_atom(a);
        let precedence = self.syntax.syntax_type(va.type_).precedence;
        return (precedence, va.depth_score);
    }

    pub fn course_add(&mut self, c: CourseId, at: usize, bricks: Vec<BrickId>) {
        assert!(!bricks.is_empty(), "adding no bricks");
        let count = bricks.len();
        for (i, b) in bricks.iter().enumerate() {
            self.courses[c].children.insert(at + i, *b);
            self.bricks[*b].course = Some(c);
            self.bricks[*b].index = at + i;
            let (group, node) = (self.courses[c].group, self.bricks[*b].node);
            self.display.group_add(group, at + i, node);
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

    pub fn course_break(&mut self, c: CourseId, index: usize) -> CourseId {
        assert!(index != 0, "breaking course at 0");
        let mut reset_cornerstone = false;
        let next = self.course_new(self.courses[c].transverse_start + self.course_transverse_span(c));
        let ci = self.courses[c].index;
        self.wall_add(ci + 1, vec![next]);
        if index < self.courses[c].children.len() {
            let transplant: Vec<BrickId> = self.courses[c].children.drain(index..).collect();
            let group = self.courses[c].group;
            self.display.group_remove(group, index, transplant.len());
            let task = self.course_get_idle_place(c);
            for brick in &transplant {
                if self.courses[c].alignment_brick == Some(*brick) {
                    let a = self.courses[c].alignment.unwrap();
                    self.alignment_remove_brick(a, *brick);
                    self.courses[c].alignment = None;
                    self.courses[c].alignment_brick = None;
                }
                let (ascent, descent) = (self.bricks[*brick].ascent, self.bricks[*brick].descent);
                if let Some(TaskKind::CoursePlace { changed, remove_max_ascent, remove_max_descent, .. }) =
                    self.task_kind_mut(task) {
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

    pub fn course_compact_step(
        &mut self,
        c: CourseId,
        skip: &mut HashSet<VisualId>,
        skip_primitives: &mut HashSet<VisualId>,
    ) -> bool {
        let mut priorities: Vec<VisualId> = vec![];
        let mut last_primitive: Option<VisualId> = None;
        let mut converse = 0.;
        for index in 0 .. self.courses[c].children.len() {
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
                },
                None => return false,
            }
        }
        let mut top = priorities[0];
        for a in priorities.iter().skip(1) {
            if self.compact_key(*a) < self.compact_key(top) {
                top = *a;
            }
        }
        self.visual_compact(top);
        skip.insert(top);
        return true;
    }

    pub fn course_destroy(&mut self, c: CourseId) {
        while let Some(last) = self.courses[c].children.last().copied() {
            self.brick_destroy(last);
        }
    }

    fn course_destroy_inner(&mut self, c: CourseId) {
        for t in [self.courses[c].idle_place, self.courses[c].idle_compact, self.courses[c].idle_expand]
            .into_iter()
            .flatten() {
            self.task_destroy(t);
        }
        let at = self.courses[c].index;
        {
            if let Some(cc) = self.wall.cornerstone_course {
                if self.courses[cc].index == at {
                    let transverse_span = self.course_transverse_span(cc);
                    self.wall.cornerstone_course = None;
                    for at2 in at .. self.wall.children.len() {
                        let following = self.wall.children[at2];
                        let t = self.courses[following].transverse_start - transverse_span;
                        self.course_set_transverse(following, t);
                    }
                }
            }
            self.wall.children.remove(at);
            let text_layer = self.text_layer;
            self.display.group_remove(text_layer, at, 1);
            let group = self.courses[c].group;
            self.display.display_destroy(group);
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
        self.courses[c].alive = false;
    }

    pub fn course_expand_step(&mut self, c: CourseId, iteration: &mut IterationContext) -> bool {
        let mut top: Option<VisualId> = None;
        for index in 0 .. self.courses[c].children.len() {
            let brick = self.courses[c].children[index];
            let leaf = self.bricks[brick].inter.visual();
            let Some(atom_visual) = self.visual_containing_atom(leaf) else {
                continue;
            };
            if self.visual_atom(atom_visual).compact {
                match top {
                    Some(t) if !(self.compact_key(atom_visual) > self.compact_key(t)) => { },
                    _ => top = Some(atom_visual),
                }
            }
        }
        let Some(top) = top else {
            return false;
        };
        {
            let mut parent_atom = top;
            while let Some(p) = self.visual_containing_atom(parent_atom) {
                parent_atom = p;
                if self.expand_ordered(parent_atom, top) && self.visual_atom(parent_atom).compact {
                    return false;
                }
            }
        }
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
            for course_i in fi ..= li {
                let course = self.wall.children[course_i];
                for brick in self.courses[course].children.clone() {
                    let leaf = self.bricks[brick].inter.visual();
                    if let VisualKind::Primitive(_) = &self.visuals[leaf].kind {
                        if self.primitive_soft_wrapped(leaf) {
                            let old_lines = self.visual_primitive(leaf).lines.len();
                            self.primitive_reflow(leaf);
                            let new_lines = self.visual_primitive(leaf).lines.len();
                            return old_lines != new_lines;
                        }
                    }
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
        {
            self.visual_get_leaf_bricks(top, &mut leaf_bricks);
            let mut course: Option<CourseId> = None;
            let mut course_last_brick = 0;
            let mut course_calc = CalcCourseConverse::default();
            for brick in &leaf_bricks {
                let Some(bc) = self.bricks[*brick].course else {
                    continue;
                };
                if Some(bc) != course {
                    let unsplit =
                        self.bricks[*brick].index == 0 && !self.brick_is_split_with(*brick, false) &&
                            self.courses[bc].index > 0;
                    if unsplit {
                        if course.is_some() && self.courses[bc].index - 1 == self.courses[course.unwrap()].index {

                        } else {
                            course_calc = CalcCourseConverse::default();
                            course = Some(self.wall.children[self.courses[bc].index - 1]);
                            course_last_brick = 0;
                        }
                    }
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
                let cc = course.unwrap();
                while course_last_brick < self.bricks[*brick].index {
                    let b1 = self.courses[cc].children[course_last_brick];
                    self.calculate_next_brick_advance(&mut course_calc, b1);
                    if course_calc.converse > self.edge {
                        return false;
                    }
                    course_last_brick += 1;
                }
                self.calculate_next_brick_advance(&mut course_calc, *brick);
                if course_calc.converse > self.edge {
                    return false;
                }
                course_last_brick += 1;
            }
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
        if !iteration.expanded.insert(top) {
            return false;
        }
        self.visual_expand(top);
        self.courses[c].last_expand_check_converse = 0.;
        return true;
    }

    pub fn course_get_idle_expand(&mut self, c: CourseId) {
        if self.courses[c].idle_expand.is_some() {
            return;
        }
        let t = self.add_iteration(TaskKind::CourseExpand { course: c }, P::COURSE_EXPAND);
        self.courses[c].idle_expand = Some(t);
    }

    fn course_get_idle_place(&mut self, c: CourseId) -> TaskId {
        if let Some(t) = self.courses[c].idle_place {
            return t;
        }
        let t = self.add_iteration(TaskKind::CoursePlace {
            course: c,
            changed: HashSet::new(),
            remove_max_ascent: 0.,
            remove_max_descent: 0.,
        }, P::COURSE_PLACE);
        self.courses[c].idle_place = Some(t);
        return t;
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
        let group = self.courses[c].group;
        self.display.group_clear(group);
        let at = self.courses[previous].children.len();
        self.course_add(previous, at, children);
        self.course_destroy_inner(c);
        if reset_cornerstone {
            let cornerstone = self.wall.cornerstone;
            self.wall_set_cornerstone_existing(cornerstone);
        }
    }

    pub fn course_new(&mut self, transverse_start: f64) -> CourseId {
        let id = self.courses.len();
        let group = self.display.display_group();
        self.display.node_set_transverse(group, transverse_start, false);
        self.courses.push(Course {
            group: group,
            index: 0,
            transverse_start: transverse_start,
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

    fn course_renumber(&mut self, c: CourseId, at: usize) {
        for i in at .. self.courses[c].children.len() {
            let b = self.courses[c].children[i];
            self.bricks[b].index = i;
        }
    }

    fn course_set_transverse(&mut self, c: CourseId, transverse: f64) {
        self.courses[c].transverse_start = transverse;
        let group = self.courses[c].group;
        let animate = self.config.animate_course_placement;
        self.display.node_set_transverse(group, transverse, animate);
        let ascent = self.courses[c].ascent;
        for child in self.courses[c].children.clone() {
            for a in self.bricks[child].attachments.clone() {
                self.attachment_set_transverse(a, transverse);
                self.attachment_set_baseline_transverse(a, transverse + ascent);
            }
        }
    }

    pub fn course_transverse_edge(&self, c: CourseId) -> f64 {
        let course = &self.courses[c];
        return course.transverse_start + course.ascent + course.descent;
    }

    pub fn course_transverse_span(&self, c: CourseId) -> f64 {
        return self.courses[c].ascent + self.courses[c].descent + self.syntax.spec_root.course_transverse_gap;
    }

    fn expand_ordered(&self, a: VisualId, b: VisualId) -> bool {
        return self.compact_key(a) >= self.compact_key(b);
    }

    pub fn run_course_compact(&mut self, task: TaskId, c: CourseId) -> bool {
        let (mut skip, mut skip_primitives) = match self.task_kind_mut(task) {
            Some(TaskKind::CourseCompact { skip, skip_primitives, .. }) => (
                std::mem::take(skip),
                std::mem::take(skip_primitives),
            ),
            _ => unreachable!(),
        };
        let out = self.course_compact_step(c, &mut skip, &mut skip_primitives);
        if let Some(TaskKind::CourseCompact { skip: s, skip_primitives: sp, .. }) = self.task_kind_mut(task) {
            *s = skip;
            *sp = skip_primitives;
        }
        return out;
    }

    pub fn run_course_place(&mut self, task: TaskId, c: CourseId) -> bool {
        let (changed, remove_max_ascent, remove_max_descent) = match self.task_kind_mut(task) {
            Some(TaskKind::CoursePlace { changed, remove_max_ascent, remove_max_descent, .. }) => (
                changed.clone(),
                *remove_max_ascent,
                *remove_max_descent,
            ),
            _ => unreachable!(),
        };
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
        if !(new_ascent && new_descent) && remove_max_ascent == self.courses[c].ascent &&
            remove_max_descent == self.courses[c].descent {
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
            let node = self.bricks[b].node;
            self.display.node_set_baseline_transverse(node, ascent, false);
            for a in self.bricks[b].attachments.clone() {
                self.attachment_set_transverse_span(a, ascent, descent);
            }
        }
        let mut calc = CalcCourseConverse::default();
        for index in 0 .. self.courses[c].children.len() {
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
            if self.courses[c].idle_compact.is_none() {
                let t = self.add_iteration(TaskKind::CourseCompact {
                    course: c,
                    skip: HashSet::new(),
                    skip_primitives: HashSet::new(),
                }, P::COURSE_COMPACT);
                self.courses[c].idle_compact = Some(t);
            }
        }
        if calc.converse * self.config.retry_expand_factor < self.courses[c].last_expand_check_converse {
            self.course_get_idle_expand(c);
        }
        if calc.converse > self.courses[c].last_expand_check_converse {
            self.courses[c].last_expand_check_converse = calc.converse;
        }
        if new_ascent || new_descent {
            let index = self.courses[c].index;
            self.wall_adjust(index);
        }
        return false;
    }

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
            let child = self.wall.children[backward as usize];
            let preceding = self.wall.children[backward as usize + 1];
            let mut transverse = self.courses[preceding].transverse_start - if stride == 0. {
                self.course_transverse_span(child)
            } else {
                stride
            };
            if preceding == cc {
                transverse -= self.wall.bedding_before;
            }
            self.course_set_transverse(child, transverse);
            backward -= 1;
            modified = true;
        }
        if forward < self.wall.children.len() {
            let preceding = self.wall.children[forward - 1];
            let mut transverse = self.courses[preceding].transverse_start + if stride == 0. {
                self.course_transverse_span(preceding)
            } else {
                stride
            };
            if preceding == cc {
                transverse += self.wall.bedding_after;
            }
            let child = self.wall.children[forward];
            self.course_set_transverse(child, transverse);
            forward += 1;
            modified = true;
        }
        if let Some(TaskKind::WallAdjust { forward: f, backward: b }) = self.task_kind_mut(task) {
            *f = forward;
            *b = backward;
        }
        self.wall_usage = match (self.wall.children.first(), self.wall.children.last()) {
            (Some(first), Some(last)) => (
                self.courses[*first].transverse_start,
                self.course_transverse_edge(*last),
            ),
            _ => (0., 0.),
        };
        return modified;
    }

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

    pub fn run_wall_cull(&mut self) -> bool {
        let Some(cornerstone_course) = self.wall.cornerstone_course else {
            return false;
        };
        let margin = self.transverse_edge * self.config.lay_beyond_view * 2.;
        let min = self.scroll - margin;
        let max = self.scroll + self.transverse_edge + margin;
        let culled = match (self.wall.children.first().copied(), self.wall.children.last().copied()) {
            (Some(first), _) if self.course_transverse_edge(first) < min => first,
            (_, Some(last)) if self.courses[last].transverse_start > max => last,
            _ => return false,
        };
        if culled == cornerstone_course {
            let Some(visible) =
                self
                    .wall
                    .children
                    .iter()
                    .copied()
                    .find(
                        |c| self.course_transverse_edge(*c) >= self.scroll &&
                            self.courses[*c].transverse_start <= self.scroll + self.transverse_edge,
                    ) else {
                    return false;
                };
            let anchor = self.courses[visible].children[0];
            self.wall_set_cornerstone_existing(Some(anchor));
        }
        self.course_destroy(culled);
        return true;
    }

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
            },
        };
        let old_course_count = self.wall.children.len();
        let again = self.courses[course].alive && self.course_expand_step(course, iteration);
        if old_course_count != self.wall.children.len() {
            at = at.saturating_sub(2);
        }
        if let Some(TaskKind::WallExpand { at: a, expanding: e }) = self.task_kind_mut(task) {
            *a = at;
            *e = if again {
                Some(course)
            } else {
                None
            };
        }
        return true;
    }

    fn wall_add(&mut self, at: usize, courses: Vec<CourseId>) {
        for (i, c) in courses.iter().enumerate() {
            self.wall.children.insert(at + i, *c);
            let (text_layer, group) = (self.text_layer, self.courses[*c].group);
            self.display.group_add(text_layer, at + i, group);
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

    pub fn wall_add_bedding(&mut self, bedding: Bedding) -> usize {
        let id = self.wall.bedding.len();
        self.wall.bedding.push(Some(bedding));
        self.wall_bedding_changed();
        return id;
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

    fn wall_bedding_changed(&mut self) {
        let mut before = 0.;
        let mut after = 0.;
        for b in self.wall.bedding.iter().flatten() {
            before += b.before;
            after += b.after;
        }
        self.wall.bedding_before = before;
        self.wall.bedding_after = after;
        let Some(cc) = self.wall.cornerstone_course else {
            return;
        };
        let index = self.courses[cc].index;
        self.wall_adjust(index);
        self.scroll_visible();
    }

    pub fn wall_clear(&mut self) {
        while let Some(last) = self.wall.children.last().copied() {
            self.course_destroy(last);
        }
        for t in [self.wall.idle_compact, self.wall.idle_expand, self.wall.idle_adjust, self.wall.idle_cull]
            .into_iter()
            .flatten() {
            self.task_destroy(t);
        }
    }

    pub fn wall_converse_edge_changed(&mut self, _old: f64, new: f64) {
        if new < self.wall.mod_old_edge {
            self.wall_idle_compact();
            self.wall.mod_old_edge = new;
        } else if new > self.wall.mod_old_edge * self.config.retry_expand_factor {
            self.wall_idle_expand();
            self.wall.mod_old_edge = new;
        }
    }

    fn wall_ensure_idle_adjust(&mut self) -> TaskId {
        if let Some(t) = self.wall.idle_adjust {
            return t;
        }
        let t = self.add_iteration(TaskKind::WallAdjust {
            forward: usize::MAX,
            backward: isize::MIN,
        }, P::WALL_ADJUST);
        self.wall.idle_adjust = Some(t);
        return t;
    }

    pub fn wall_idle_compact(&mut self) {
        match self.wall.idle_compact {
            Some(t) => {
                if let Some(TaskKind::WallCompact { at, .. }) = self.task_kind_mut(t) {
                    *at = 0;
                }
            },
            None => {
                let t = self.add_iteration(TaskKind::WallCompact {
                    at: 0,
                    compact: None,
                }, P::WALL_COMPACT);
                self.wall.idle_compact = Some(t);
            },
        }
    }

    pub fn wall_idle_expand(&mut self) {
        match self.wall.idle_expand {
            Some(t) => {
                if let Some(TaskKind::WallExpand { at, .. }) = self.task_kind_mut(t) {
                    *at = 0;
                }
            },
            None => {
                let t = self.add_iteration(TaskKind::WallExpand {
                    at: 0,
                    expanding: None,
                }, P::WALL_EXPAND);
                self.wall.idle_expand = Some(t);
            },
        }
    }

    pub fn wall_remove_bedding(&mut self, id: usize) {
        self.wall.bedding[id] = None;
        self.wall_bedding_changed();
    }

    fn wall_renumber(&mut self, at: usize) {
        for i in at .. self.wall.children.len() {
            let c = self.wall.children[i];
            self.courses[c].index = i;
        }
    }

    pub fn wall_set_cornerstone(
        &mut self,
        cornerstone: BrickId,
        find_previous: Option<BrickId>,
        find_next: Option<BrickId>,
    ) {
        self.wall.cornerstone = Some(cornerstone);
        if self.bricks[cornerstone].course.is_none() {
            match (find_previous, find_next) {
                (Some(found), Some(_)) => self.brick_add_after(found, cornerstone),
                _ => {
                    self.wall_clear();
                    let course = self.course_new(0.);
                    self.wall_add(0, vec![course]);
                    self.course_add(course, 0, vec![cornerstone]);
                },
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

    fn wall_set_cornerstone_existing(&mut self, cornerstone: Option<BrickId>) {
        match cornerstone {
            Some(b) => self.wall_set_cornerstone(b, None, None),
            None => {
                self.wall.cornerstone = None;
                self.wall.cornerstone_course = None;
            },
        }
    }

    pub fn wall_usage(&self) -> Option<(f64, f64)> {
        let first = *self.wall.children.first()?;
        let last = *self.wall.children.last()?;
        return Some((self.courses[first].transverse_start, self.course_transverse_edge(last)));
    }

    pub fn wall_view_changed(&mut self) {
        self.trigger_idle_lay_bricks_outward();
        if self.wall.idle_cull.is_none() {
            let t = self.add_iteration(TaskKind::WallCull, P::WALL_CULL);
            self.wall.idle_cull = Some(t);
        }
    }
}
