use {
    crate::{
        context::{
            AlignId,
            BrickId,
            Context,
            HoverableId,
            Vector,
            VisualId,
        },
        cursor::{
            Cursor,
            Hoverable,
        },
        display::PIXELS_PER_MM,
        document::{
            AtomId,
            Field,
        },
        spec::{
            SpecCondition,
            SpecSplit,
        },
        syntax::{
            Front,
            FrontArray,
            FrontPrimitive,
            Symbol,
            SymbolKind,
            Syntax,
            TypeId,
        },
        wall::{
            BrickEmpty,
            BrickInter,
            BrickKind,
            BrickText,
        },
    },
    std::{
        collections::HashSet,
        rc::Rc,
    },
};

fn front_array_of(syntax: &Syntax, type_: TypeId, front: usize) -> &FrontArray {
    let Front::Array(a) = &syntax.syntax_type(type_).front[front] else {
        panic!("front {} of type {} is not an array", front, type_);
    };
    return a;
}

pub fn empty_forward() -> Rc<HashSet<String>> {
    return Rc::new(HashSet::new());
}

pub enum ExtendBrickResult {
    Brick(BrickId),
    Empty,
    Exists,
}

pub struct Line {
    pub brick: Option<BrickId>,
    pub hard: bool,
    pub offset: usize,
    pub text: String,
}

#[derive(Clone, Copy, Debug)]
pub enum SymbolPart {
    Empty,
    Front,
    Prefix(usize),
    Separator(usize),
    Suffix(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct SymbolRef {
    pub front: usize,
    pub part: SymbolPart,
    pub type_: TypeId,
}

pub struct Visual {
    pub depth: usize,
    pub kind: VisualKind,
    pub parent: Option<VisualParent>,
}

pub struct VisualAtom {
    pub alignments: Vec<(String, AlignId)>,
    pub atom: AtomId,
    pub children: Vec<VisualId>,
    pub compact: bool,
    pub default_selection: usize,
    pub depth_score: i64,
    pub need_intermediate_cursor: bool,
    pub selectable: Vec<(String, VisualId)>,
    pub type_: TypeId,
}

pub struct VisualFieldArray {
    pub atom: AtomId,
    pub children: Vec<VisualId>,
    pub empty: Option<BrickId>,
    pub front: usize,
    pub type_: TypeId,
}

pub struct VisualFieldAtom {
    pub atom: AtomId,
    pub body: VisualId,
    pub ellipsis: Option<BrickId>,
    pub front: usize,
    pub type_: TypeId,
}

pub struct VisualGroup {
    pub children: Vec<VisualId>,
}

pub enum VisualKind {
    Atom(VisualAtom),
    FieldArray(VisualFieldArray),
    FieldAtom(VisualFieldAtom),
    Group(VisualGroup),
    Primitive(VisualPrimitive),
    Symbol(VisualSymbol),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualParent {
    pub index: usize,
    pub visual: VisualId,
}

pub struct VisualPrimitive {
    pub atom: AtomId,
    pub front: usize,
    pub hard_line_count: usize,
    pub lines: Vec<Line>,
    pub type_: TypeId,
    pub value: String,
}

pub struct VisualSymbol {
    pub brick: Option<BrickId>,
    pub condition: Option<bool>,
    pub symbol: SymbolRef,
}

impl Context {
    fn array_create_empty(&mut self, v: VisualId) -> Option<BrickId> {
        let (type_, front) = {
            let a = self.visual_field_array(v);
            (a.type_, a.front)
        };
        let spec = self.front_array_spec(type_, front);
        let empty = spec.empty.as_ref()?;
        let split = empty.split;
        let align = self.leaf_find_alignment(v, &empty.alignment);
        let split_align = self.leaf_find_alignment(v, &empty.split_alignment);
        let kind = match &empty.kind {
            SymbolKind::Text { text, style } => BrickKind::Text(BrickText {
                text: text.clone(),
                style: *style,
            }),
            SymbolKind::Space { width, ascent, descent } => {
                BrickKind::Empty(BrickEmpty {
                    ascent: ascent * PIXELS_PER_MM,
                    descent: descent * PIXELS_PER_MM,
                    span: width * PIXELS_PER_MM,
                })
            },
        };
        let brick = self.brick_new(kind, BrickInter::ArrayEmpty(v), split, align, split_align);
        let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
            unreachable!();
        };
        a.empty = Some(brick);
        return Some(brick);
    }

    pub fn array_create_element(&mut self, v: VisualId, element: AtomId, group_index: usize) -> VisualId {
        let (atom, type_, front) = {
            let a = self.visual_field_array(v);
            (a.atom, a.type_, a.front)
        };
        let depth = self.visuals[v].depth;
        let depth_score = self.visual_atom(self.visual_containing_atom(v).expect("array without atom")).depth_score;
        let syntax = self.syntax.clone();
        let f = front_array_of(&syntax, type_, front);
        let group = self.push_visual(VisualKind::Group(VisualGroup { children: vec![] }), Some(VisualParent {
            visual: v,
            index: group_index,
        }), depth + 1);
        let mut children = vec![];
        for (j, s) in f.prefix.iter().enumerate() {
            children.push(self.visual_new_symbol(SymbolRef {
                type_: type_,
                front: front,
                part: SymbolPart::Prefix(j),
            }, &s.condition, atom, Some(VisualParent {
                visual: group,
                index: children.len(),
            }), depth + 2));
        }
        children.push(self.visual_ensure_atom(element, Some(VisualParent {
            visual: group,
            index: children.len(),
        }), depth + 3, depth_score));
        for (j, s) in f.suffix.iter().enumerate() {
            children.push(self.visual_new_symbol(SymbolRef {
                type_: type_,
                front: front,
                part: SymbolPart::Suffix(j),
            }, &s.condition, atom, Some(VisualParent {
                visual: group,
                index: children.len(),
            }), depth + 2));
        }
        let VisualKind::Group(g) = &mut self.visuals[group].kind else {
            unreachable!();
        };
        g.children = children;
        return group;
    }

    pub fn array_create_separator(&mut self, v: VisualId, group_index: usize) -> VisualId {
        let (atom, type_, front) = {
            let a = self.visual_field_array(v);
            (a.atom, a.type_, a.front)
        };
        let depth = self.visuals[v].depth;
        let syntax = self.syntax.clone();
        let f = front_array_of(&syntax, type_, front);
        let group = self.push_visual(VisualKind::Group(VisualGroup { children: vec![] }), Some(VisualParent {
            visual: v,
            index: group_index,
        }), depth + 1);
        for (j, s) in f.separator.iter().enumerate() {
            let child = self.visual_new_symbol(SymbolRef {
                type_: type_,
                front: front,
                part: SymbolPart::Separator(j),
            }, &s.condition, atom, Some(VisualParent {
                visual: group,
                index: j,
            }), depth + 2);
            let VisualKind::Group(g) = &mut self.visuals[group].kind else {
                unreachable!();
            };
            g.children.push(child);
        }
        return group;
    }

    pub fn array_element_visual(&self, array: VisualId, index: usize) -> VisualId {
        let atom = self.array_elements(array)[index];
        return self.atom_visual[atom].expect("element atom has no visual");
    }

    pub fn array_elements(&self, array: VisualId) -> Vec<AtomId> {
        let a = self.visual_field_array(array);
        let Some(Field::Array(elements)) =
            self.document.document_atom(a.atom).fields.get(&self.front_array_spec(a.type_, a.front).field) else {
                panic!("array field missing");
            };
        return elements.clone();
    }

    pub fn array_value_index(&self, array: VisualId, group_index: usize) -> usize {
        let a = self.visual_field_array(array);
        if self.front_array_spec(a.type_, a.front).separator.is_empty() {
            return group_index;
        }
        return group_index / 2;
    }

    pub fn array_visual_index(&self, array: VisualId, value_index: usize) -> usize {
        let a = self.visual_field_array(array);
        if self.front_array_spec(a.type_, a.front).separator.is_empty() {
            return value_index;
        }
        return value_index * 2;
    }

    pub fn brick_create_next(&mut self, inter: BrickInter) -> ExtendBrickResult {
        match inter {
            BrickInter::Symbol(v) | BrickInter::ArrayEmpty(v) | BrickInter::FieldAtomEllipsis(v) => {
                return self.parent_create_next_brick(v)
            },
            BrickInter::Line(v, i) => {
                if i + 1 == self.visual_primitive(v).lines.len() {
                    return self.parent_create_next_brick(v);
                }
                return self.line_create_brick(v, i + 1);
            },
        }
    }

    pub fn brick_create_previous(&mut self, inter: BrickInter) -> ExtendBrickResult {
        match inter {
            BrickInter::Symbol(v) | BrickInter::ArrayEmpty(v) | BrickInter::FieldAtomEllipsis(v) => {
                return self.parent_create_previous_brick(v)
            },
            BrickInter::Line(v, i) => {
                if i == 0 {
                    return self.parent_create_previous_brick(v);
                }
                return self.line_create_brick(v, i - 1);
            },
        }
    }

    pub fn brick_hover(&mut self, brick: BrickId, point: Vector) -> Option<(HoverableId, bool)> {
        match self.bricks[brick].inter {
            BrickInter::Symbol(v) => return self.parent_hover(v, point),
            BrickInter::Line(v, i) => {
                let index = i;
                let selected = match self.cursor.and_then(|c| self.cursors[c].as_ref()) {
                    Some(Cursor::Primitive(pc)) => pc.visual == v,
                    _ => false,
                };
                if !selected {
                    if let Some(out) = self.parent_hover(v, point) {
                        return Some(out);
                    }
                }
                let brick = self.visual_primitive(v).lines[index].brick.expect("hovering line without brick");
                let under = self.brick_text_get_under(brick, point);
                let new_index = self.visual_primitive(v).lines[index].offset + under;
                return Some(self.primitive_hover_position(v, new_index));
            },
            BrickInter::ArrayEmpty(v) => return Some(self.array_hover_placeholder(v, brick)),
            BrickInter::FieldAtomEllipsis(v) => return self.parent_hover(v, point),
        }
    }

    pub fn brick_inter_destroyed(&mut self, inter: BrickInter) {
        match inter {
            BrickInter::Symbol(v) => {
                let VisualKind::Symbol(s) = &mut self.visuals[v].kind else {
                    unreachable!();
                };
                s.brick = None;
            },
            BrickInter::Line(v, i) => {
                self.visual_primitive_mut(v).lines[i].brick = None;
            },
            BrickInter::ArrayEmpty(v) => {
                let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
                    unreachable!();
                };
                a.empty = None;
            },
            BrickInter::FieldAtomEllipsis(v) => {
                let VisualKind::FieldAtom(fa) = &mut self.visuals[v].kind else {
                    unreachable!();
                };
                fa.ellipsis = None;
            },
        }
    }

    fn field_atom_create_ellipsis(&mut self, v: VisualId) -> BrickId {
        let (type_, front) = {
            let VisualKind::FieldAtom(fa) = &self.visuals[v].kind else {
                unreachable!();
            };
            (fa.type_, fa.front)
        };
        let syntax = self.syntax.clone();
        let Front::Atom(f) = &syntax.syntax_type(type_).front[front] else {
            unreachable!();
        };
        let spec = &f.ellipsis;
        let split = spec.split;
        let align = self.leaf_find_alignment(v, &spec.alignment);
        let split_align = self.leaf_find_alignment(v, &spec.split_alignment);
        let kind = match &spec.kind {
            SymbolKind::Text { text, style } => BrickKind::Text(BrickText {
                text: text.clone(),
                style: *style,
            }),
            SymbolKind::Space { width, ascent, descent } => {
                BrickKind::Empty(BrickEmpty {
                    ascent: ascent * PIXELS_PER_MM,
                    descent: descent * PIXELS_PER_MM,
                    span: width * PIXELS_PER_MM,
                })
            },
        };
        let brick = self.brick_new(kind, BrickInter::FieldAtomEllipsis(v), split, align, split_align);
        let VisualKind::FieldAtom(fa) = &mut self.visuals[v].kind else {
            unreachable!();
        };
        fa.ellipsis = Some(brick);
        self.parent_notify_first_brick_created(v, brick);
        self.parent_notify_last_brick_created(v, brick);
        return brick;
    }

    pub fn front_array_spec(&self, type_: TypeId, front: usize) -> &FrontArray {
        let Front::Array(a) = &self.syntax.syntax_type(type_).front[front] else {
            panic!("front {} of type {} is not an array", front, type_);
        };
        return a;
    }

    pub fn front_primitive_spec(&self, type_: TypeId, front: usize) -> &FrontPrimitive {
        let Front::Primitive(p) = &self.syntax.syntax_type(type_).front[front] else {
            panic!("front {} of type {} is not a primitive", front, type_);
        };
        return p;
    }

    pub fn leaf_find_alignment(&self, leaf: VisualId, name: &Option<String>) -> Option<AlignId> {
        let name = name.as_ref()?;
        let atom = self.visual_containing_atom(leaf)?;
        return self.visual_find_alignment(atom, name, None);
    }

    pub fn line_create_brick(&mut self, v: VisualId, index: usize) -> ExtendBrickResult {
        if self.visual_primitive(v).lines[index].brick.is_some() {
            return ExtendBrickResult::Exists;
        }
        let brick = self.line_create_brick_internal(v, index);
        if let Some(c) = self.cursor {
            if let Some(Cursor::Primitive(pc)) = &self.cursors[c] {
                if pc.visual == v && (pc.range.begin_line == Some(index) || pc.range.end_line == Some(index)) {
                    self.cursor_primitive_range_nudge(c);
                }
            }
        }
        return ExtendBrickResult::Brick(brick);
    }

    fn line_create_brick_internal(&mut self, v: VisualId, index: usize) -> BrickId {
        let (split, align, split_align) = 'line_spec_split_and_alignments: {
            let p = self.visual_primitive(v);
            let syntax = self.syntax.clone();
            let spec = {
                let Front::Primitive(fp) = &syntax.syntax_type(p.type_).front[p.front] else {
                    unreachable!();
                };
                fp
            };
            let hard = p.lines[index].hard;
            if index == 0 {
                break 'line_spec_split_and_alignments (
                    spec.split,
                    self.leaf_find_alignment(v, &spec.first_alignment),
                    self.leaf_find_alignment(v, &spec.first_split_alignment),
                );
            }
            (SpecSplit::Always, None, self.leaf_find_alignment(v, if hard {
                &spec.hard_split_alignment
            } else {
                &spec.soft_split_alignment
            }))
        };
        let p = self.visual_primitive(v);
        let spec = self.front_primitive_spec(p.type_, p.front);
        let style = if self.syntax.syntax_primitive_valid(p.type_, &spec.field, &p.value) {
            spec.style
        } else {
            spec.invalid_style
        };
        let text = p.lines[index].text.clone();
        let line_count = p.lines.len();
        let brick = self.brick_new(BrickKind::Line(BrickText {
            text: text,
            style: style,
        }, index), BrickInter::Line(v, index), split, align, split_align);
        self.visual_primitive_mut(v).lines[index].brick = Some(brick);
        if index == 0 {
            self.parent_notify_first_brick_created(v, brick);
        }
        if index + 1 == line_count {
            self.parent_notify_last_brick_created(v, brick);
        }
        return brick;
    }

    pub fn line_create_or_get_brick(&mut self, v: VisualId, index: usize) -> BrickId {
        if let Some(b) = self.visual_primitive(v).lines[index].brick {
            return b;
        }
        return self.line_create_brick_internal(v, index);
    }

    pub fn parent_create_next_brick(&mut self, child: VisualId) -> ExtendBrickResult {
        let Some(p) = self.visuals[child].parent else {
            return ExtendBrickResult::Empty;
        };
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) | VisualKind::FieldArray(_) => {
                let children = self.visual_children(p.visual);
                if p.index + 1 < children.len() {
                    return self.visual_create_first_brick(children[p.index + 1]);
                }
                return self.parent_create_next_brick(p.visual);
            },
            VisualKind::Atom(a) => {
                let children = a.children.clone();
                for at in p.index + 1 .. children.len() {
                    let r = self.visual_create_first_brick(children[at]);
                    if let ExtendBrickResult::Empty = r {
                        continue;
                    }
                    return r;
                }
                return self.parent_create_next_brick(p.visual);
            },
            VisualKind::FieldAtom(_) => return self.parent_create_next_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_create_previous_brick(&mut self, child: VisualId) -> ExtendBrickResult {
        let Some(p) = self.visuals[child].parent else {
            return ExtendBrickResult::Empty;
        };
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) | VisualKind::FieldArray(_) => {
                let children = self.visual_children(p.visual);
                if p.index >= 1 {
                    return self.visual_create_last_brick(children[p.index - 1]);
                }
                return self.parent_create_previous_brick(p.visual);
            },
            VisualKind::Atom(a) => {
                let children = a.children.clone();
                for at in (0 .. p.index).rev() {
                    let r = self.visual_create_last_brick(children[at]);
                    if let ExtendBrickResult::Empty = r {
                        continue;
                    }
                    return r;
                }
                return self.parent_create_previous_brick(p.visual);
            },
            VisualKind::FieldAtom(_) => return self.parent_create_previous_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_find_alignment(&self, atom: VisualId, name: &str) -> Option<AlignId> {
        let p = self.visuals[atom].parent?;
        let (container, forward) = match &self.visuals[p.visual].kind {
            VisualKind::FieldAtom(fa) => {
                let Front::Atom(f) = &self.syntax.syntax_type(fa.type_).front[fa.front] else {
                    unreachable!();
                };
                (self.visual_containing_atom(p.visual), f.forward_alignments.clone())
            },
            VisualKind::Group(_) => {
                let gp = self.visuals[p.visual].parent.expect("element group without array");
                let VisualKind::FieldArray(a) = &self.visuals[gp.visual].kind else {
                    panic!("atom's group parent is not an array field");
                };
                (
                    self.visual_containing_atom(gp.visual),
                    self.front_array_spec(a.type_, a.front).forward_alignments.clone(),
                )
            },
            _ => panic!("unexpected atom parent kind"),
        };
        let container = container?;
        return self.visual_find_alignment(container, name, Some(&forward));
    }

    pub fn parent_find_next_brick(&self, child: VisualId) -> Option<BrickId> {
        let p = self.visuals[child].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) | VisualKind::FieldArray(_) | VisualKind::Atom(_) => {
                let children = self.visual_children(p.visual);
                for at in p.index + 1 .. children.len() {
                    if let Some(b) = self.visual_get_first_brick(children[at]) {
                        return Some(b);
                    }
                }
                return self.parent_find_next_brick(p.visual);
            },
            VisualKind::FieldAtom(_) => return self.parent_find_next_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_find_previous_brick(&self, child: VisualId) -> Option<BrickId> {
        let p = self.visuals[child].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) | VisualKind::FieldArray(_) | VisualKind::Atom(_) => {
                let children = self.visual_children(p.visual);
                for at in (0 .. p.index).rev() {
                    if let Some(b) = self.visual_get_last_brick(children[at]) {
                        return Some(b);
                    }
                }
                return self.parent_find_previous_brick(p.visual);
            },
            VisualKind::FieldAtom(_) => return self.parent_find_previous_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_get_next_brick(&self, child: VisualId) -> Option<BrickId> {
        let p = self.visuals[child].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) | VisualKind::FieldArray(_) | VisualKind::Atom(_) => {
                let children = self.visual_children(p.visual);
                if p.index + 1 >= children.len() {
                    return self.parent_get_next_brick(p.visual);
                }
                return self.visual_get_first_brick(children[p.index + 1]);
            },
            VisualKind::FieldAtom(_) => return self.parent_get_next_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_get_previous_brick(&self, child: VisualId) -> Option<BrickId> {
        let p = self.visuals[child].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) | VisualKind::FieldArray(_) | VisualKind::Atom(_) => {
                let children = self.visual_children(p.visual);
                if p.index == 0 {
                    return self.parent_get_previous_brick(p.visual);
                }
                return self.visual_get_last_brick(children[p.index - 1]);
            },
            VisualKind::FieldAtom(_) => return self.parent_get_previous_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_hover(&mut self, child: VisualId, point: Vector) -> Option<(HoverableId, bool)> {
        let p = self.visuals[child].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) => return self.parent_hover(p.visual, point),
            VisualKind::FieldArray(_) => {
                let array_group_selectable = {
                    let a = self.visual_field_array(p.visual);
                    if self.front_array_spec(a.type_, a.front).separator.is_empty() {
                        true
                    } else {
                        p.index % 2 == 0
                    }
                };
                if !array_group_selectable {
                    return self.parent_hover(p.visual, point);
                }
                let index = self.array_value_index(p.visual, p.index);
                return self.array_hover_element(p.visual, index);
            },
            VisualKind::Atom(_) => {
                let Some(sel) = self.visual_selectable_index(p.visual, child) else {
                    return self.parent_hover(p.visual, point);
                };
                if let Some(Cursor::Atom(c)) = self.cursor.and_then(|c| self.cursors[c].as_ref()) {
                    if c.visual == p.visual && c.index == sel {
                        return None;
                    }
                }
                if self.visual_atom(p.visual).need_intermediate_cursor {
                    return Some(self.atom_hover_selectable(p.visual, sel));
                }
                return self.parent_hover(p.visual, point);
            },
            VisualKind::FieldAtom(_) => return self.parent_hover(p.visual, point),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub(crate) fn parent_lay_bricks_around(&mut self, v: VisualId) {
        if let Some(b) = self.parent_find_previous_brick(v) {
            self.trigger_idle_lay_bricks_after_end(b);
        }
        if let Some(b) = self.parent_find_next_brick(v) {
            self.trigger_idle_lay_bricks_before_start(b);
        }
    }

    pub fn parent_notify_first_brick_created(&mut self, child: VisualId, brick: BrickId) {
        let Some(p) = self.visuals[child].parent else {
            return;
        };
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) => {
                if let VisualKind::Atom(_) = &self.visuals[child].kind {
                    if let Some(gp) = self.visuals[p.visual].parent {
                        if let VisualKind::FieldArray(_) = &self.visuals[gp.visual].kind {
                            let index = self.array_value_index(gp.visual, gp.index);
                            self.array_element_brick_created(gp.visual, index, brick, true);
                        }
                    }
                }
                if p.index == 0 {
                    self.parent_notify_first_brick_created(p.visual, brick);
                }
            },
            VisualKind::FieldArray(_) => {
                if p.index == 0 {
                    self.parent_notify_first_brick_created(p.visual, brick);
                }
            },
            VisualKind::Atom(_) => {
                if let Some(sel) = self.visual_selectable_index(p.visual, child) {
                    self.atom_selectable_brick_created(p.visual, sel, brick, true);
                }
                if p.index == 0 {
                    self.parent_notify_first_brick_created(p.visual, brick);
                }
            },
            VisualKind::FieldAtom(_) => self.parent_notify_first_brick_created(p.visual, brick),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_notify_last_brick_created(&mut self, child: VisualId, brick: BrickId) {
        let Some(p) = self.visuals[child].parent else {
            return;
        };
        match &self.visuals[p.visual].kind {
            VisualKind::Group(g) => {
                let last = p.index + 1 == g.children.len();
                if let VisualKind::Atom(_) = &self.visuals[child].kind {
                    if let Some(gp) = self.visuals[p.visual].parent {
                        if let VisualKind::FieldArray(_) = &self.visuals[gp.visual].kind {
                            let index = self.array_value_index(gp.visual, gp.index);
                            self.array_element_brick_created(gp.visual, index, brick, false);
                        }
                    }
                }
                if last {
                    self.parent_notify_last_brick_created(p.visual, brick);
                }
            },
            VisualKind::FieldArray(a) => {
                if p.index + 1 == a.children.len() {
                    self.parent_notify_last_brick_created(p.visual, brick);
                }
            },
            VisualKind::Atom(a) => {
                let last = p.index + 1 == a.children.len();
                if let Some(sel) = self.visual_selectable_index(p.visual, child) {
                    self.atom_selectable_brick_created(p.visual, sel, brick, false);
                }
                if last {
                    self.parent_notify_last_brick_created(p.visual, brick);
                }
            },
            VisualKind::FieldAtom(_) => self.parent_notify_last_brick_created(p.visual, brick),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn primitive_find_containing(&self, v: VisualId, offset: usize) -> usize {
        let p = self.visual_primitive(v);
        for (i, line) in p.lines.iter().enumerate() {
            if line.offset + line.text.len() < offset {
                continue;
            }
            return i;
        }
        return p.lines.len();
    }

    pub(crate) fn primitive_lines_shifted(&mut self, v: VisualId, from: usize) {
        let bricks: Vec<(usize, BrickId)> =
            self
                .visual_primitive(v)
                .lines
                .iter()
                .enumerate()
                .skip(from)
                .filter_map(|(i, l)| l.brick.map(|b| (i, b)))
                .collect();
        for (i, b) in bricks {
            self.bricks[b].inter = BrickInter::Line(v, i);
        }
    }

    pub fn primitive_reflow(&mut self, v: VisualId) {
        let mut any_over = false;
        let mut all_under = true;
        let factor = self.config.retry_expand_factor;
        let mut i = self.visual_primitive(v).lines.len();
        while i > 0 {
            i -= 1;
            let (hard, brick) = {
                let l = &self.visual_primitive(v).lines[i];
                (l.hard, l.brick)
            };
            let Some(brick) = brick else {
                continue;
            };
            let edge = self.brick_converse_edge(brick);
            if !any_over && edge > self.edge {
                any_over = true;
            }
            if edge <= self.edge && edge * factor >= self.edge {
                all_under = false;
            }
            if hard && (any_over || all_under) {
                {
                    let syntax = self.syntax.clone();
                    let atom = self.visual_containing_atom(v).expect("primitive without atom");
                    let (type_, front) = {
                        let p = self.visual_primitive(v);
                        (p.type_, p.front)
                    };
                    let Front::Primitive(spec) = &syntax.syntax_type(type_).front[front] else {
                        unreachable!();
                    };
                    let font = syntax.syntax_style(spec.style).font.clone();
                    let modified_offset_start = self.visual_primitive(v).lines[i].offset;
                    let mut offset = modified_offset_start;
                    let mut text = String::new();
                    let mut end_index = i;
                    {
                        let p = self.visual_primitive(v);
                        for j in i .. p.lines.len() {
                            if j > i && p.lines[j].hard {
                                break;
                            }
                            text.push_str(&p.lines[j].text);
                            end_index += 1;
                        }
                    }
                    let modified_length = text.len();
                    let mut j = i;
                    while j < end_index {
                        if text.is_empty() && j > i {
                            break;
                        }
                        let converse = match self.visual_primitive(v).lines[j].brick {
                            Some(b) => self.brick_get_converse(b),
                            None => {
                                let name = if j == 0 {
                                    &spec.first_alignment
                                } else if j == i {
                                    &spec.hard_split_alignment
                                } else {
                                    &spec.soft_split_alignment
                                };
                                match name.as_ref().and_then(|n| self.visual_find_alignment(atom, n, None)) {
                                    Some(a) => self.aligns[a].converse,
                                    None => 0.,
                                }
                            },
                        };
                        let split = self.resplit_fit(&font, &text, converse);
                        let new_text = text[..split].to_string();
                        if new_text != self.visual_primitive(v).lines[j].text {
                            let brick = self.visual_primitive(v).lines[j].brick;
                            self.visual_primitive_mut(v).lines[j].text = new_text.clone();
                            if let Some(b) = brick {
                                self.brick_set_text(b, new_text);
                            }
                        }
                        self.visual_primitive_mut(v).lines[j].offset = offset;
                        text = text[split..].to_string();
                        offset += split;
                        j += 1;
                    }
                    let mut first_line_created = None;
                    let mut last_line_created = None;
                    if !text.is_empty() {
                        first_line_created = Some(j);
                        while !text.is_empty() {
                            let converse =
                                match spec
                                    .soft_split_alignment
                                    .as_ref()
                                    .and_then(|n| self.visual_find_alignment(atom, n, None)) {
                                    Some(a) => self.aligns[a].converse,
                                    None => 0.,
                                };
                            let split = self.resplit_fit(&font, &text, converse);
                            self.visual_primitive_mut(v).lines.insert(j, Line {
                                hard: false,
                                offset: offset,
                                text: text[..split].to_string(),
                                brick: None,
                            });
                            self.primitive_lines_shifted(v, j + 1);
                            text = text[split..].to_string();
                            offset += split;
                            j += 1;
                        }
                        last_line_created = Some(j);
                    }
                    if j < end_index {
                        let doomed: Vec<(bool, Option<BrickId>)> =
                            self.visual_primitive(v).lines[j .. end_index]
                                .iter()
                                .map(|l| (l.hard, l.brick))
                                .collect();
                        for (hard, brick) in doomed {
                            if hard {
                                self.visual_primitive_mut(v).hard_line_count -= 1;
                            }
                            if let Some(b) = brick {
                                self.brick_destroy(b);
                            }
                        }
                        self.visual_primitive_mut(v).lines.drain(j .. end_index);
                        self.primitive_lines_shifted(v, j);
                    }
                    {
                        let p = self.visual_primitive_mut(v);
                        let mut offset = offset;
                        for line in p.lines[j..].iter_mut() {
                            if line.hard {
                                offset += 1;
                            }
                            line.offset = offset;
                            offset += line.text.len();
                        }
                    }
                    if let (Some(first), Some(last)) = (first_line_created, last_line_created) {
                        self.trigger_idle_lay_bricks_lines(v, first, last - first);
                    }
                    if let Some(h) = self.hover {
                        if let Some(Hoverable::Primitive(ph)) = &self.hoverables[h] {
                            if ph.visual == v {
                                if ph.range.begin_offset >= modified_offset_start + modified_length {
                                    self.hoverable_primitive_range_nudge(h);
                                } else if ph.range.begin_offset >= modified_offset_start ||
                                    ph.range.end_offset >= modified_offset_start {
                                    self.clear_hover();
                                }
                            }
                        }
                    }
                    if let Some(c) = self.cursor {
                        if let Some(Cursor::Primitive(pc)) = &self.cursors[c] {
                            if pc.visual == v {
                                self.cursor_primitive_range_nudge(c);
                            }
                        }
                    }
                }
                any_over = false;
                all_under = true;
            }
        }
    }

    pub fn primitive_soft_wrapped(&self, v: VisualId) -> bool {
        let p = self.visual_primitive(v);
        return p.lines.len() > p.hard_line_count;
    }

    fn push_visual(&mut self, kind: VisualKind, parent: Option<VisualParent>, depth: usize) -> VisualId {
        let id = self.visuals.len();
        self.visuals.push(Visual {
            kind: kind,
            parent: parent,
            depth: depth,
        });
        return id;
    }

    fn resplit_fit(&mut self, font: &crate::measure::FontSpec, text: &str, converse: f64) -> usize {
        let width = self.display.display_font_width(font, text);
        let edge = converse + width;
        if converse < self.edge && edge > self.edge {
            let edge_offset = self.edge - converse;
            let under = self.display.display_index_at_converse(font, text, edge_offset);
            if under == text.len() {
                return under;
            }
            let mut split = crate::measure::measure_line_before_or_at(text, under);
            if split == 0 {
                split = under;
            }
            if text[..split].chars().count() < 4 {
                return text.len();
            }
            return split;
        }
        return text.len();
    }

    fn symbol_create_brick(&mut self, v: VisualId) -> BrickId {
        let VisualKind::Symbol(s) = &self.visuals[v].kind else {
            unreachable!();
        };
        let spec = self.symbol_spec(s.symbol);
        let split = spec.split;
        let align = self.leaf_find_alignment(v, &spec.alignment);
        let split_align = self.leaf_find_alignment(v, &spec.split_alignment);
        let kind = match &spec.kind {
            SymbolKind::Text { text, style } => BrickKind::Text(BrickText {
                text: text.clone(),
                style: *style,
            }),
            SymbolKind::Space { width, ascent, descent } => {
                BrickKind::Empty(BrickEmpty {
                    ascent: ascent * PIXELS_PER_MM,
                    descent: descent * PIXELS_PER_MM,
                    span: width * PIXELS_PER_MM,
                })
            },
        };
        let brick = self.brick_new(kind, BrickInter::Symbol(v), split, align, split_align);
        let VisualKind::Symbol(s) = &mut self.visuals[v].kind else {
            unreachable!();
        };
        s.brick = Some(brick);
        return brick;
    }

    pub fn symbol_condition_value(&self, c: &SpecCondition, atom: AtomId) -> bool {
        let a = self.document.document_atom(atom);
        return match c {
            SpecCondition::Empty(c) => {
                let empty = match a.fields.get(&c.field) {
                    Some(Field::Primitive(s)) => s.is_empty(),
                    Some(Field::Array(v)) => v.is_empty(),
                    _ => panic!("condition field `{}` is not a primitive or array", c.field),
                };
                empty != c.invert
            },
            SpecCondition::Precedent(c) => {
                let is_precedent = 'is_precedent: {
                    let a = self.document.document_atom(atom);
                    let Some(parent_ref) = &a.parent else {
                        break 'is_precedent true;
                    };
                    let parent = self.document.document_atom(parent_ref.atom);
                    let parent_type = self.syntax.syntax_type(parent.type_);
                    let own_type = self.syntax.syntax_type(a.type_);
                    let mut fore_child = true;
                    let mut back_child = true;
                    let mut found = false;
                    for front in &parent_type.front {
                        match front {
                            Front::Symbol(s) => {
                                if s.symbol_delimits() {
                                    if !found {
                                        back_child = false;
                                    } else {
                                        fore_child = false;
                                    }
                                }
                            },
                            Front::Primitive(_) => {
                                if !found {
                                    back_child = false;
                                } else {
                                    fore_child = false;
                                }
                            },
                            Front::Array(f) => {
                                if !found {
                                    for p in &f.prefix {
                                        if p.symbol_delimits() {
                                            back_child = false;
                                        }
                                    }
                                }
                                if f.field == parent_ref.field {
                                    let Some(Field::Array(siblings)) = parent.fields.get(&f.field) else {
                                        panic!("array field `{}` missing", f.field);
                                    };
                                    if parent_ref.index > 0 {
                                        back_child = false;
                                    }
                                    found = true;
                                    if parent_ref.index + 1 < siblings.len() {
                                        fore_child = false;
                                    }
                                }
                                if found {
                                    for s in &f.suffix {
                                        if s.symbol_delimits() {
                                            fore_child = false;
                                        }
                                    }
                                }
                            },
                            Front::Atom(f) => {
                                if f.field == parent_ref.field {
                                    found = true;
                                }
                            },
                        }
                    }
                    if !back_child && !fore_child {
                        break 'is_precedent true;
                    }
                    if parent_type.precedence < own_type.precedence {
                        break 'is_precedent true;
                    }
                    if parent_type.precedence == own_type.precedence && fore_child == parent_type.associate_forward {
                        break 'is_precedent true;
                    }
                    false
                };
                is_precedent != c.invert
            },
        };
    }

    pub fn symbol_spec(&self, r: SymbolRef) -> &Symbol {
        let front = &self.syntax.syntax_type(r.type_).front[r.front];
        match (front, r.part) {
            (Front::Symbol(s), SymbolPart::Front) => return s,
            (Front::Array(a), SymbolPart::Prefix(j)) => return &a.prefix[j],
            (Front::Array(a), SymbolPart::Suffix(j)) => return &a.suffix[j],
            (Front::Array(a), SymbolPart::Separator(j)) => return &a.separator[j],
            (Front::Array(a), SymbolPart::Empty) => return a.empty.as_ref().unwrap(),
            _ => panic!("symbol reference doesn't match the front"),
        }
    }

    pub fn visual_atom(&self, v: VisualId) -> &VisualAtom {
        let VisualKind::Atom(a) = &self.visuals[v].kind else {
            panic!("visual {} is not an atom", v);
        };
        return a;
    }

    pub fn visual_atom_mut(&mut self, v: VisualId) -> &mut VisualAtom {
        let VisualKind::Atom(a) = &mut self.visuals[v].kind else {
            panic!("visual {} is not an atom", v);
        };
        return a;
    }

    pub(crate) fn visual_children(&self, v: VisualId) -> Vec<VisualId> {
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => return a.children.clone(),
            VisualKind::Group(g) => return g.children.clone(),
            VisualKind::FieldArray(a) => return a.children.clone(),
            _ => panic!("visual {} has no children", v),
        }
    }

    pub fn visual_compact(&mut self, v: VisualId) {
        match &self.visuals[v].kind {
            VisualKind::Atom(_) => {
                self.visual_atom_mut(v).compact = true;
                for child in self.visual_children(v) {
                    self.visual_compact(child);
                }
            },
            VisualKind::Group(_) | VisualKind::FieldArray(_) => {
                for child in self.visual_children(v) {
                    if let VisualKind::Atom(_) = &self.visuals[child].kind {
                        continue;
                    }
                    self.visual_compact(child);
                }
            },
            VisualKind::Symbol(s) => {
                if let Some(b) = s.brick {
                    self.brick_layout_properties_changed(b);
                }
            },
            VisualKind::Primitive(p) => {
                if let Some(b) = p.lines[0].brick {
                    self.brick_layout_properties_changed(b);
                }
            },
            VisualKind::FieldAtom(_) => { },
        }
    }

    pub fn visual_containing_atom(&self, v: VisualId) -> Option<VisualId> {
        let p = self.visuals[v].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Atom(_) => return Some(p.visual),
            _ => return self.visual_containing_atom(p.visual),
        }
    }

    pub fn visual_create_first_brick(&mut self, v: VisualId) -> ExtendBrickResult {
        match &self.visuals[v].kind {
            VisualKind::Atom(_) | VisualKind::Group(_) => {
                for child in self.visual_children(v) {
                    let r = self.visual_create_first_brick(child);
                    if let ExtendBrickResult::Empty = r {
                        continue;
                    }
                    return r;
                }
                return ExtendBrickResult::Empty;
            },
            VisualKind::Symbol(s) => {
                if s.brick.is_some() {
                    return ExtendBrickResult::Exists;
                }
                if s.condition == Some(false) {
                    return ExtendBrickResult::Empty;
                }
                let brick = self.symbol_create_brick(v);
                self.parent_notify_first_brick_created(v, brick);
                self.parent_notify_last_brick_created(v, brick);
                return ExtendBrickResult::Brick(brick);
            },
            VisualKind::Primitive(_) => return self.line_create_brick(v, 0),
            VisualKind::FieldAtom(fa) => {
                let (body, ellipsis) = (fa.body, fa.ellipsis);
                if self.visual_field_atom_ellipsize(v) {
                    if ellipsis.is_some() {
                        return ExtendBrickResult::Exists;
                    }
                    return ExtendBrickResult::Brick(self.field_atom_create_ellipsis(v));
                }
                if body == usize::MAX {
                    return ExtendBrickResult::Empty;
                }
                return self.visual_create_first_brick(body);
            },
            VisualKind::FieldArray(a) => {
                if self.array_elements(v).is_empty() {
                    if a.empty.is_some() {
                        return ExtendBrickResult::Exists;
                    }
                    return match self.array_create_empty(v) {
                        Some(b) => ExtendBrickResult::Brick(b),
                        None => ExtendBrickResult::Empty,
                    };
                }
                for child in self.visual_children(v) {
                    let r = self.visual_create_first_brick(child);
                    if let ExtendBrickResult::Empty = r {
                        continue;
                    }
                    return r;
                }
                return ExtendBrickResult::Empty;
            },
        }
    }

    pub fn visual_create_last_brick(&mut self, v: VisualId) -> ExtendBrickResult {
        match &self.visuals[v].kind {
            VisualKind::Atom(_) | VisualKind::Group(_) => {
                for child in self.visual_children(v).into_iter().rev() {
                    let r = self.visual_create_last_brick(child);
                    if let ExtendBrickResult::Empty = r {
                        continue;
                    }
                    return r;
                }
                return ExtendBrickResult::Empty;
            },
            VisualKind::Symbol(_) => return self.visual_create_first_brick(v),
            VisualKind::Primitive(p) => {
                let last = p.lines.len() - 1;
                return self.line_create_brick(v, last);
            },
            VisualKind::FieldAtom(fa) => {
                let (body, ellipsis) = (fa.body, fa.ellipsis);
                if self.visual_field_atom_ellipsize(v) {
                    if ellipsis.is_some() {
                        return ExtendBrickResult::Exists;
                    }
                    return ExtendBrickResult::Brick(self.field_atom_create_ellipsis(v));
                }
                if body == usize::MAX {
                    return ExtendBrickResult::Empty;
                }
                return self.visual_create_last_brick(body);
            },
            VisualKind::FieldArray(a) => {
                if self.array_elements(v).is_empty() {
                    if a.empty.is_some() {
                        return ExtendBrickResult::Exists;
                    }
                    return match self.array_create_empty(v) {
                        Some(b) => ExtendBrickResult::Brick(b),
                        None => ExtendBrickResult::Empty,
                    };
                }
                for child in self.visual_children(v).into_iter().rev() {
                    let r = self.visual_create_last_brick(child);
                    if let ExtendBrickResult::Empty = r {
                        continue;
                    }
                    return r;
                }
                return ExtendBrickResult::Empty;
            },
        }
    }

    pub fn visual_create_or_get_cornerstone_candidate(&mut self, v: VisualId) -> Option<BrickId> {
        match &self.visuals[v].kind {
            VisualKind::Atom(_) | VisualKind::Group(_) => {
                for child in self.visual_children(v) {
                    if let Some(b) = self.visual_create_or_get_cornerstone_candidate(child) {
                        return Some(b);
                    }
                }
                return None;
            },
            VisualKind::Symbol(s) => {
                if s.condition.is_some() {
                    return None;
                }
                if let Some(b) = s.brick {
                    return Some(b);
                }
                let brick = self.symbol_create_brick(v);
                self.parent_notify_first_brick_created(v, brick);
                self.parent_notify_last_brick_created(v, brick);
                return Some(brick);
            },
            VisualKind::Primitive(_) => return Some(self.line_create_or_get_brick(v, 0)),
            VisualKind::FieldAtom(fa) => {
                let (body, ellipsis) = (fa.body, fa.ellipsis);
                if self.visual_field_atom_ellipsize(v) {
                    if let Some(b) = ellipsis {
                        return Some(b);
                    }
                    return Some(self.field_atom_create_ellipsis(v));
                }
                if body == usize::MAX {
                    return None;
                }
                return self.visual_create_or_get_cornerstone_candidate(body);
            },
            VisualKind::FieldArray(a) => {
                if self.array_elements(v).is_empty() {
                    if let Some(b) = a.empty {
                        return Some(b);
                    }
                    return self.array_create_empty(v);
                }
                for child in self.visual_children(v) {
                    if let Some(b) = self.visual_create_or_get_cornerstone_candidate(child) {
                        return Some(b);
                    }
                }
                return None;
            },
        }
    }

    pub fn visual_ensure_atom(
        &mut self,
        atom: AtomId,
        parent: Option<VisualParent>,
        depth: usize,
        depth_score: i64,
    ) -> VisualId {
        if let Some(v) = self.atom_visual[atom] {
            self.visual_root(v, parent, depth, depth_score);
            return v;
        }
        let syntax = self.syntax.clone();
        let atom_type = self.document.document_atom(atom).type_;
        let type_ = syntax.syntax_type(atom_type);
        let (depth, depth_score) = if parent.is_none() {
            (0, 0)
        } else {
            (depth, depth_score + type_.depth_score)
        };
        let vid = self.push_visual(VisualKind::Atom(VisualAtom {
            atom: atom,
            type_: atom_type,
            children: vec![],
            selectable: vec![],
            alignments: vec![],
            depth_score: depth_score,
            compact: false,
            need_intermediate_cursor: false,
            default_selection: 0,
        }), parent, depth);
        self.atom_visual[atom] = Some(vid);
        for (name, spec) in &type_.alignments {
            let aid = self.alignment_create(spec);
            self.visual_atom_mut(vid).alignments.push((name.clone(), aid));
        }
        for (_, aid) in self.visual_atom(vid).alignments.clone() {
            self.alignment_root(aid, vid);
        }
        let mut need_intermediate = false;
        for (index, front) in type_.front.iter().enumerate() {
            let child_parent = Some(VisualParent {
                visual: vid,
                index: index,
            });
            let child = match front {
                Front::Symbol(s) => self.visual_new_symbol(SymbolRef {
                    type_: atom_type,
                    front: index,
                    part: SymbolPart::Front,
                }, &s.condition, atom, child_parent, depth + 1),
                Front::Primitive(p) => {
                    let Some(Field::Primitive(text)) =
                        self.document.document_atom(atom).fields.get(&p.field) else {
                            panic!("primitive field `{}` missing", p.field);
                        };
                    let text = text.clone();
                    let v = self.push_visual(VisualKind::Primitive(VisualPrimitive {
                        atom: atom,
                        type_: atom_type,
                        front: index,
                        value: text.clone(),
                        lines: vec![],
                        hard_line_count: 0,
                    }), child_parent, depth + 1);
                    {
                        let mut lines = vec![];
                        let mut offset = 0;
                        for raw in text.split('\n') {
                            lines.push(Line {
                                hard: true,
                                offset: offset,
                                text: raw.to_string(),
                                brick: None,
                            });
                            offset += 1 + raw.len();
                        }
                        let p = self.visual_primitive_mut(v);
                        p.hard_line_count = lines.len();
                        p.lines = lines;
                    }
                    self.visual_atom_mut(vid).selectable.push((p.field.clone(), v));
                    v
                },
                Front::Atom(f) => {
                    let child_atom = match self.document.document_atom(atom).fields.get(&f.field) {
                        Some(Field::Atom(child)) => Some(*child),
                        Some(Field::Array(elements)) if f.from_array => elements.first().copied(),
                        _ => panic!("atom field `{}` missing", f.field),
                    };
                    let v = self.push_visual(VisualKind::FieldAtom(VisualFieldAtom {
                        atom: atom,
                        type_: atom_type,
                        front: index,
                        body: usize::MAX,
                        ellipsis: None,
                    }), child_parent, depth + 1);
                    if let Some(child_atom) = child_atom {
                        let body = self.visual_ensure_atom(child_atom, Some(VisualParent {
                            visual: v,
                            index: 0,
                        }), depth + 2, depth_score);
                        let VisualKind::FieldAtom(fa) = &mut self.visuals[v].kind else {
                            unreachable!();
                        };
                        fa.body = body;
                    }
                    need_intermediate = true;
                    self.visual_atom_mut(vid).selectable.push((f.field.clone(), v));
                    v
                },
                Front::Array(f) => {
                    let v = self.push_visual(VisualKind::FieldArray(VisualFieldArray {
                        atom: atom,
                        type_: atom_type,
                        front: index,
                        children: vec![],
                        empty: None,
                    }), child_parent, depth + 1);
                    for element in self.array_elements(v) {
                        let separate = !f.separator.is_empty() && !self.visual_field_array(v).children.is_empty();
                        if separate {
                            let group_index = self.visual_field_array(v).children.len();
                            let separator = self.array_create_separator(v, group_index);
                            let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
                                unreachable!();
                            };
                            a.children.push(separator);
                        }
                        let group_index = self.visual_field_array(v).children.len();
                        let group = self.array_create_element(v, element, group_index);
                        let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
                            unreachable!();
                        };
                        a.children.push(group);
                    }
                    self.visual_atom_mut(vid).selectable.push((f.field.clone(), v));
                    v
                },
            };
            self.visual_atom_mut(vid).children.push(child);
        }
        {
            let va = self.visual_atom_mut(vid);
            if va.selectable.len() >= 2 {
                need_intermediate = true;
            }
            va.need_intermediate_cursor = need_intermediate;
            va.default_selection =
                type_
                    .default_selection
                    .as_ref()
                    .filter(|_| need_intermediate)
                    .and_then(|d| va.selectable.iter().position(|(f, _)| f == d))
                    .unwrap_or(0);
        }
        return vid;
    }

    pub fn visual_expand(&mut self, v: VisualId) {
        match &self.visuals[v].kind {
            VisualKind::Atom(_) => {
                self.visual_atom_mut(v).compact = false;
                for child in self.visual_children(v) {
                    self.visual_expand(child);
                }
            },
            VisualKind::Group(_) | VisualKind::FieldArray(_) => {
                for child in self.visual_children(v) {
                    if let VisualKind::Atom(_) = &self.visuals[child].kind {
                        continue;
                    }
                    self.visual_expand(child);
                }
            },
            VisualKind::Symbol(s) => {
                if let Some(b) = s.brick {
                    self.brick_layout_properties_changed(b);
                }
            },
            VisualKind::Primitive(p) => {
                if let Some(b) = p.lines[0].brick {
                    self.brick_layout_properties_changed(b);
                }
            },
            VisualKind::FieldAtom(_) => { },
        }
    }

    pub fn visual_field_array(&self, v: VisualId) -> &VisualFieldArray {
        let VisualKind::FieldArray(a) = &self.visuals[v].kind else {
            panic!("visual {} is not an array field", v);
        };
        return a;
    }

    pub fn visual_field_atom_ellipsize(&self, v: VisualId) -> bool {
        if !self.window {
            return false;
        }
        let Some(atom) = self.visual_containing_atom(v) else {
            return false;
        };
        return self.visual_atom(atom).depth_score >= self.config.ellipsize_threshold;
    }

    pub fn visual_find_alignment(
        &self,
        atom: VisualId,
        name: &str,
        allow: Option<&HashSet<String>>,
    ) -> Option<AlignId> {
        if allow.map_or(true, |a| a.contains(name)) {
            if let Some((_, id)) = self.visual_atom(atom).alignments.iter().find(|(n, _)| n == name) {
                return Some(*id);
            }
        }
        return self.parent_find_alignment(atom, name);
    }

    pub fn visual_get_first_brick(&self, v: VisualId) -> Option<BrickId> {
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                return a.children.iter().find_map(|c| self.visual_get_first_brick(*c))
            },
            VisualKind::Group(g) => {
                return g.children.iter().find_map(|c| self.visual_get_first_brick(*c))
            },
            VisualKind::Symbol(s) => return s.brick,
            VisualKind::Primitive(p) => return p.lines[0].brick,
            VisualKind::FieldAtom(fa) => {
                if self.visual_field_atom_ellipsize(v) || fa.body == usize::MAX {
                    return fa.ellipsis;
                }
                return self.visual_get_first_brick(fa.body);
            },
            VisualKind::FieldArray(a) => {
                if a.empty.is_some() {
                    return a.empty;
                }
                return a.children.iter().find_map(|c| self.visual_get_first_brick(*c));
            },
        }
    }

    pub fn visual_get_last_brick(&self, v: VisualId) -> Option<BrickId> {
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                return a.children.iter().rev().find_map(|c| self.visual_get_last_brick(*c))
            },
            VisualKind::Group(g) => {
                return g.children.iter().rev().find_map(|c| self.visual_get_last_brick(*c))
            },
            VisualKind::Symbol(s) => return s.brick,
            VisualKind::Primitive(p) => return p.lines.last().unwrap().brick,
            VisualKind::FieldAtom(fa) => {
                if self.visual_field_atom_ellipsize(v) || fa.body == usize::MAX {
                    return fa.ellipsis;
                }
                return self.visual_get_last_brick(fa.body);
            },
            VisualKind::FieldArray(a) => {
                if a.empty.is_some() {
                    return a.empty;
                }
                return a.children.iter().rev().find_map(|c| self.visual_get_last_brick(*c));
            },
        }
    }

    pub fn visual_get_leaf_bricks(&self, v: VisualId, out: &mut Vec<BrickId>) {
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                for c in &a.children {
                    self.visual_get_leaf_bricks(*c, out);
                }
            },
            VisualKind::Group(g) => {
                for c in &g.children {
                    if let VisualKind::Atom(_) = &self.visuals[*c].kind {
                        continue;
                    }
                    self.visual_get_leaf_bricks(*c, out);
                }
            },
            VisualKind::FieldArray(a) => {
                for c in &a.children {
                    self.visual_get_leaf_bricks(*c, out);
                }
            },
            VisualKind::Symbol(s) => {
                if let Some(b) = s.brick {
                    out.push(b);
                }
            },
            VisualKind::Primitive(p) => {
                for l in &p.lines {
                    if let Some(b) = l.brick {
                        out.push(b);
                    }
                }
            },
            VisualKind::FieldAtom(fa) => {
                if let Some(b) = fa.ellipsis {
                    out.push(b);
                } else if fa.body != usize::MAX {
                    let body = fa.body;
                    self.visual_get_leaf_bricks(body, out);
                }
            },
        }
    }

    fn visual_new_symbol(
        &mut self,
        symbol: SymbolRef,
        condition: &Option<SpecCondition>,
        atom: AtomId,
        parent: Option<VisualParent>,
        depth: usize,
    ) -> VisualId {
        let condition = condition.as_ref().map(|c| self.symbol_condition_value(c, atom));
        return self.push_visual(VisualKind::Symbol(VisualSymbol {
            symbol: symbol,
            brick: None,
            condition: condition,
        }), parent, depth);
    }

    pub fn visual_primitive(&self, v: VisualId) -> &VisualPrimitive {
        let VisualKind::Primitive(p) = &self.visuals[v].kind else {
            panic!("visual {} is not a primitive", v);
        };
        return p;
    }

    pub fn visual_primitive_mut(&mut self, v: VisualId) -> &mut VisualPrimitive {
        let VisualKind::Primitive(p) = &mut self.visuals[v].kind else {
            panic!("visual {} is not a primitive", v);
        };
        return p;
    }

    pub fn visual_root(&mut self, v: VisualId, parent: Option<VisualParent>, depth: usize, depth_score: i64) {
        self.visuals[v].parent = parent;
        self.visuals[v].depth = depth;
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                let atom = a.atom;
                let type_ = a.type_;
                let score = if parent.is_none() {
                    0
                } else {
                    depth_score + self.syntax.syntax_type(type_).depth_score
                };
                self.atom_visual[atom] = Some(v);
                self.visual_atom_mut(v).depth_score = score;
                for child in self.visual_children(v) {
                    let child_parent = self.visuals[child].parent;
                    self.visual_root(child, child_parent, depth + 1, score);
                }
            },
            VisualKind::Group(_) | VisualKind::FieldArray(_) => {
                for child in self.visual_children(v) {
                    let child_parent = self.visuals[child].parent;
                    self.visual_root(child, child_parent, depth + 1, depth_score);
                }
            },
            VisualKind::FieldAtom(fa) => {
                let (body, ellipsis) = (fa.body, fa.ellipsis);
                if self.visual_field_atom_ellipsize(v) {
                    if body != usize::MAX {
                        self.visual_uproot(body, None);
                        let VisualKind::FieldAtom(fa) = &mut self.visuals[v].kind else {
                            unreachable!();
                        };
                        fa.body = usize::MAX;
                        self.parent_lay_bricks_around(v);
                    }
                } else {
                    if let Some(b) = ellipsis {
                        self.brick_destroy(b);
                    }
                    let atom = {
                        let (owner, type_, front) = {
                            let VisualKind::FieldAtom(fa) = &self.visuals[v].kind else {
                                unreachable!();
                            };
                            (fa.atom, fa.type_, fa.front)
                        };
                        let syntax = self.syntax.clone();
                        let Front::Atom(f) = &syntax.syntax_type(type_).front[front] else {
                            unreachable!();
                        };
                        let Some(Field::Atom(child)) =
                            self.document.document_atom(owner).fields.get(&f.field) else {
                                unreachable!();
                            };
                        *child
                    };
                    if body == usize::MAX {
                        let created = self.visual_ensure_atom(atom, Some(VisualParent {
                            visual: v,
                            index: 0,
                        }), depth + 1, depth_score);
                        let VisualKind::FieldAtom(fa) = &mut self.visuals[v].kind else {
                            unreachable!();
                        };
                        fa.body = created;
                        self.parent_lay_bricks_around(v);
                    } else {
                        self.visual_root(body, Some(VisualParent {
                            visual: v,
                            index: 0,
                        }), depth + 1, depth_score);
                    }
                }
            },
            VisualKind::Symbol(_) | VisualKind::Primitive(_) => { },
        }
    }

    pub fn visual_selectable_index(&self, atom: VisualId, child: VisualId) -> Option<usize> {
        return self.visual_atom(atom).selectable.iter().position(|(_, v)| *v == child);
    }

    pub fn visual_uproot(&mut self, v: VisualId, root: Option<VisualId>) {
        if root == Some(v) {
            return;
        }
        if let Some(c) = self.cursor {
            let cursor_visual = match self.cursor_get(c) {
                crate::cursor::Cursor::Atom(c) => c.visual,
                crate::cursor::Cursor::Array(c) => c.visual,
                crate::cursor::Cursor::Primitive(c) => c.visual,
            };
            if cursor_visual == v {
                self.clear_cursor();
            }
        }
        if let Some(h) = self.hover {
            let hover_visual = match self.hoverables[h].as_ref() {
                Some(crate::cursor::Hoverable::Atom { visual, .. }) |
                Some(crate::cursor::Hoverable::Array { visual, .. }) |
                Some(crate::cursor::Hoverable::ArrayPlaceholder { visual, .. }) => Some(
                    *visual,
                ),
                Some(crate::cursor::Hoverable::Primitive(p)) => Some(p.visual),
                None => None,
            };
            if hover_visual == Some(v) {
                self.clear_hover();
            }
        }
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                let atom = a.atom;
                self.atom_visual[atom] = None;
                for child in self.visual_children(v) {
                    self.visual_uproot(child, root);
                }
            },
            VisualKind::Group(_) | VisualKind::FieldArray(_) => {
                for child in self.visual_children(v) {
                    self.visual_uproot(child, root);
                }
            },
            VisualKind::FieldAtom(fa) => {
                let (body, ellipsis) = (fa.body, fa.ellipsis);
                if let Some(b) = ellipsis {
                    self.brick_destroy(b);
                }
                if body != usize::MAX {
                    self.visual_uproot(body, root);
                }
            },
            VisualKind::Symbol(s) => {
                if let Some(b) = s.brick {
                    self.brick_destroy(b);
                }
            },
            VisualKind::Primitive(p) => {
                for b in p.lines.iter().filter_map(|l| l.brick).collect::<Vec<_>>() {
                    self.brick_destroy(b);
                }
            },
        }
    }
}
