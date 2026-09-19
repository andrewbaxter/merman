//! The visual tree (merman's `Visual` classes): one node per atom, field and
//! symbol. Visuals exist for the whole document; bricks are created lazily by
//! walking the tree outward from the cornerstone.
use crate::context::{AlignId, BrickId, Context, HoverableId, Vector, VisualId};
use crate::cursor::{Cursor, Hoverable};
use crate::document::{AtomId, Field};
use crate::spec::{SpecCondition, SpecSplit};
use crate::syntax::{Front, FrontArray, FrontPrimitive, Symbol, SymbolKind, TypeId};
use crate::wall::{BrickInter, BrickKind};
use std::collections::HashSet;
use std::rc::Rc;

pub struct Visual {
    pub kind: VisualKind,
    pub parent: Option<VisualParent>,
    pub depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualParent {
    pub visual: VisualId,
    /// Index among the parent's children.
    pub index: usize,
}

pub enum VisualKind {
    Atom(VisualAtom),
    Group(VisualGroup),
    Symbol(VisualSymbol),
    Primitive(VisualPrimitive),
    FieldAtom(VisualFieldAtom),
    FieldArray(VisualFieldArray),
}

pub struct VisualAtom {
    pub atom: AtomId,
    pub type_: TypeId,
    pub children: Vec<VisualId>,
    /// (field id, visual) of the front elements that can be selected.
    pub selectable: Vec<(String, VisualId)>,
    pub alignments: Vec<(String, AlignId)>,
    pub depth_score: i64,
    pub compact: bool,
    pub need_intermediate_cursor: bool,
    pub default_selection: usize,
}

pub struct VisualGroup {
    pub children: Vec<VisualId>,
}

#[derive(Clone, Copy, Debug)]
pub enum SymbolPart {
    Front,
    Prefix(usize),
    Suffix(usize),
    Separator(usize),
    Empty,
}

#[derive(Clone, Copy, Debug)]
pub struct SymbolRef {
    pub type_: TypeId,
    pub front: usize,
    pub part: SymbolPart,
}

pub struct VisualSymbol {
    pub symbol: SymbolRef,
    pub brick: Option<BrickId>,
    /// Evaluated condition; None if the symbol has no condition.
    pub condition: Option<bool>,
}

pub struct VisualPrimitive {
    pub atom: AtomId,
    pub type_: TypeId,
    pub front: usize,
    pub value: String,
    pub lines: Vec<Line>,
    pub hard_line_count: usize,
}

pub struct Line {
    pub hard: bool,
    pub offset: usize,
    pub text: String,
    pub brick: Option<BrickId>,
}

pub struct VisualFieldAtom {
    pub atom: AtomId,
    pub type_: TypeId,
    pub front: usize,
    pub body: VisualId,
}

pub struct VisualFieldArray {
    pub atom: AtomId,
    pub type_: TypeId,
    pub front: usize,
    /// Element groups, interleaved with separator groups if the front has separators.
    pub children: Vec<VisualId>,
    pub empty: Option<BrickId>,
}

pub enum ExtendBrickResult {
    /// No contents, no brick to create; skip and continue.
    Empty,
    /// Brick was already created; stop laying bricks in this direction.
    Exists,
    /// Created this brick, place and use as source for next.
    Brick(BrickId),
}

impl Context {
    // ---- Spec access -------------------------------------------------------

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

    pub fn visual_field_array(&self, v: VisualId) -> &VisualFieldArray {
        let VisualKind::FieldArray(a) = &self.visuals[v].kind else {
            panic!("visual {} is not an array field", v);
        };
        return a;
    }

    fn visual_children(&self, v: VisualId) -> Vec<VisualId> {
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => return a.children.clone(),
            VisualKind::Group(g) => return g.children.clone(),
            VisualKind::FieldArray(a) => return a.children.clone(),
            _ => panic!("visual {} has no children", v),
        }
    }

    /// The atom visual containing this visual (merman `atomVisual()` on a
    /// visual's parent).
    pub fn visual_containing_atom(&self, v: VisualId) -> Option<VisualId> {
        let p = self.visuals[v].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Atom(_) => return Some(p.visual),
            _ => return self.visual_containing_atom(p.visual),
        }
    }

    /// Selectable index of an atom's child, if it is selectable.
    pub fn visual_selectable_index(&self, atom: VisualId, child: VisualId) -> Option<usize> {
        return self
            .visual_atom(atom)
            .selectable
            .iter()
            .position(|(_, v)| *v == child);
    }

    /// Value index of an element group within its array (separators interleave).
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

    fn array_group_selectable(&self, array: VisualId, group_index: usize) -> bool {
        let a = self.visual_field_array(array);
        if self.front_array_spec(a.type_, a.front).separator.is_empty() {
            return true;
        }
        return group_index % 2 == 0;
    }

    pub fn array_elements(&self, array: VisualId) -> Vec<AtomId> {
        let a = self.visual_field_array(array);
        let Some(Field::Array(elements)) = self
            .document
            .document_atom(a.atom)
            .fields
            .get(&self.front_array_spec(a.type_, a.front).field)
        else {
            panic!("array field missing");
        };
        return elements.clone();
    }

    /// Visual of the element atom at a value index.
    pub fn array_element_visual(&self, array: VisualId, index: usize) -> VisualId {
        let atom = self.array_elements(array)[index];
        return self.atom_visual[atom].expect("element atom has no visual");
    }

    // ---- Construction ------------------------------------------------------

    fn push_visual(&mut self, kind: VisualKind, parent: Option<VisualParent>, depth: usize) -> VisualId {
        let id = self.visuals.len();
        self.visuals.push(Visual {
            kind,
            parent,
            depth,
        });
        return id;
    }

    /// Merman's `VisualAtom` constructor: creates the atom's visual and, eagerly,
    /// the visuals of everything under it.
    pub fn visual_ensure_atom(
        &mut self,
        atom: AtomId,
        parent: Option<VisualParent>,
        depth: usize,
        depth_score: i64,
    ) -> VisualId {
        if let Some(v) = self.atom_visual[atom] {
            return v;
        }
        let syntax = self.syntax.clone();
        let document = self.document.clone();
        let a = document.document_atom(atom);
        let type_ = syntax.syntax_type(a.type_);
        let (depth, depth_score) = if parent.is_none() {
            (0, 0)
        } else {
            (depth, depth_score + type_.depth_score)
        };
        let vid = self.push_visual(
            VisualKind::Atom(VisualAtom {
                atom,
                type_: a.type_,
                children: vec![],
                selectable: vec![],
                alignments: vec![],
                depth_score,
                compact: false,
                need_intermediate_cursor: false,
                default_selection: 0,
            }),
            parent,
            depth,
        );
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
                index,
            });
            let child = match front {
                Front::Symbol(s) => self.visual_new_symbol(
                    SymbolRef {
                        type_: a.type_,
                        front: index,
                        part: SymbolPart::Front,
                    },
                    &s.condition,
                    atom,
                    child_parent,
                    depth + 1,
                ),
                Front::Primitive(p) => {
                    let Some(Field::Primitive(text)) = a.fields.get(&p.field) else {
                        panic!("primitive field `{}` missing", p.field);
                    };
                    let v = self.push_visual(
                        VisualKind::Primitive(VisualPrimitive {
                            atom,
                            type_: a.type_,
                            front: index,
                            value: text.clone(),
                            lines: vec![],
                            hard_line_count: 0,
                        }),
                        child_parent,
                        depth + 1,
                    );
                    self.primitive_set(v, text);
                    self.visual_atom_mut(vid).selectable.push((p.field.clone(), v));
                    v
                }
                Front::Atom(f) => {
                    let Some(Field::Atom(child_atom)) = a.fields.get(&f.field) else {
                        panic!("atom field `{}` missing", f.field);
                    };
                    let v = self.push_visual(
                        VisualKind::FieldAtom(VisualFieldAtom {
                            atom,
                            type_: a.type_,
                            front: index,
                            body: 0,
                        }),
                        child_parent,
                        depth + 1,
                    );
                    let body = self.visual_ensure_atom(
                        *child_atom,
                        Some(VisualParent {
                            visual: v,
                            index: 0,
                        }),
                        depth + 2,
                        depth_score,
                    );
                    let VisualKind::FieldAtom(fa) = &mut self.visuals[v].kind else {
                        unreachable!();
                    };
                    fa.body = body;
                    need_intermediate = true;
                    self.visual_atom_mut(vid).selectable.push((f.field.clone(), v));
                    v
                }
                Front::Array(f) => {
                    let v = self.push_visual(
                        VisualKind::FieldArray(VisualFieldArray {
                            atom,
                            type_: a.type_,
                            front: index,
                            children: vec![],
                            empty: None,
                        }),
                        child_parent,
                        depth + 1,
                    );
                    self.array_build_children(v, depth_score);
                    self.visual_atom_mut(vid).selectable.push((f.field.clone(), v));
                    v
                }
            };
            self.visual_atom_mut(vid).children.push(child);
        }
        {
            let va = self.visual_atom_mut(vid);
            if va.selectable.len() >= 2 {
                need_intermediate = true;
            }
            va.need_intermediate_cursor = need_intermediate;
            va.default_selection = 0;
        }
        return vid;
    }

    fn visual_new_symbol(
        &mut self,
        symbol: SymbolRef,
        condition: &Option<SpecCondition>,
        atom: AtomId,
        parent: Option<VisualParent>,
        depth: usize,
    ) -> VisualId {
        let condition = condition.as_ref().map(|c| self.condition_show(c, atom));
        return self.push_visual(
            VisualKind::Symbol(VisualSymbol {
                symbol,
                brick: None,
                condition,
            }),
            parent,
            depth,
        );
    }

    /// Merman's `VisualFieldArray.coreChange` for the initial contents.
    fn array_build_children(&mut self, v: VisualId, depth_score: i64) {
        let (atom, type_, front) = {
            let a = self.visual_field_array(v);
            (a.atom, a.type_, a.front)
        };
        let depth = self.visuals[v].depth;
        let elements = self.array_elements(v);
        let syntax = self.syntax.clone();
        let spec = syntax.syntax_type(type_);
        let Front::Array(f) = &spec.front[front] else {
            unreachable!();
        };
        for (i, element) in elements.iter().enumerate() {
            if !f.separator.is_empty() && i > 0 {
                let group_index = self.visual_field_array(v).children.len();
                let group = self.push_visual(
                    VisualKind::Group(VisualGroup { children: vec![] }),
                    Some(VisualParent {
                        visual: v,
                        index: group_index,
                    }),
                    depth + 1,
                );
                for (j, s) in f.separator.iter().enumerate() {
                    let child = self.visual_new_symbol(
                        SymbolRef {
                            type_,
                            front,
                            part: SymbolPart::Separator(j),
                        },
                        &s.condition,
                        atom,
                        Some(VisualParent {
                            visual: group,
                            index: j,
                        }),
                        depth + 2,
                    );
                    let VisualKind::Group(g) = &mut self.visuals[group].kind else {
                        unreachable!();
                    };
                    g.children.push(child);
                }
                let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
                    unreachable!();
                };
                a.children.push(group);
            }
            let group_index = self.visual_field_array(v).children.len();
            let group = self.push_visual(
                VisualKind::Group(VisualGroup { children: vec![] }),
                Some(VisualParent {
                    visual: v,
                    index: group_index,
                }),
                depth + 1,
            );
            let mut children = vec![];
            for (j, s) in f.prefix.iter().enumerate() {
                children.push(self.visual_new_symbol(
                    SymbolRef {
                        type_,
                        front,
                        part: SymbolPart::Prefix(j),
                    },
                    &s.condition,
                    atom,
                    Some(VisualParent {
                        visual: group,
                        index: children.len(),
                    }),
                    depth + 2,
                ));
            }
            children.push(self.visual_ensure_atom(
                *element,
                Some(VisualParent {
                    visual: group,
                    index: children.len(),
                }),
                depth + 3,
                depth_score,
            ));
            for (j, s) in f.suffix.iter().enumerate() {
                children.push(self.visual_new_symbol(
                    SymbolRef {
                        type_,
                        front,
                        part: SymbolPart::Suffix(j),
                    },
                    &s.condition,
                    atom,
                    Some(VisualParent {
                        visual: group,
                        index: children.len(),
                    }),
                    depth + 2,
                ));
            }
            let VisualKind::Group(g) = &mut self.visuals[group].kind else {
                unreachable!();
            };
            g.children = children;
            let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
                unreachable!();
            };
            a.children.push(group);
        }
    }

    fn condition_show(&self, condition: &SpecCondition, atom: AtomId) -> bool {
        let a = self.document.document_atom(atom);
        match condition {
            SpecCondition::Empty(c) => {
                let empty = match a.fields.get(&c.field) {
                    Some(Field::Primitive(s)) => s.is_empty(),
                    Some(Field::Array(v)) => v.is_empty(),
                    _ => panic!("condition field `{}` is not a primitive or array", c.field),
                };
                return empty != c.invert;
            }
            SpecCondition::Precedent(c) => return self.is_precedent(atom) != c.invert,
        }
    }

    /// Whether the atom binds at least as tightly as its parent, so it needs
    /// no parentheses (merman `AtomType.isPrecedent`).
    fn is_precedent(&self, atom: AtomId) -> bool {
        let a = self.document.document_atom(atom);
        let Some(parent_ref) = &a.parent else {
            return true;
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
                }
                Front::Primitive(_) => {
                    if !found {
                        back_child = false;
                    } else {
                        fore_child = false;
                    }
                }
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
                }
                Front::Atom(f) => {
                    if f.field == parent_ref.field {
                        found = true;
                    }
                }
            }
        }
        if !back_child && !fore_child {
            return true;
        }
        if parent_type.precedence < own_type.precedence {
            return true;
        }
        if parent_type.precedence == own_type.precedence && fore_child == parent_type.associate_forward {
            return true;
        }
        return false;
    }

    // ---- Alignment lookup --------------------------------------------------

    /// Merman `VisualAtom.findAlignment(name, allow)`.
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

    /// An atom's parent's alignment lookup: the containing atom's alignments
    /// restricted to what the field forwards.
    pub fn parent_find_alignment(&self, atom: VisualId, name: &str) -> Option<AlignId> {
        let p = self.visuals[atom].parent?;
        let (container, forward) = match &self.visuals[p.visual].kind {
            VisualKind::FieldAtom(fa) => {
                let Front::Atom(f) = &self.syntax.syntax_type(fa.type_).front[fa.front] else {
                    unreachable!();
                };
                (self.visual_containing_atom(p.visual), f.forward_alignments.clone())
            }
            VisualKind::Group(_) => {
                let gp = self.visuals[p.visual].parent.expect("element group without array");
                let VisualKind::FieldArray(a) = &self.visuals[gp.visual].kind else {
                    panic!("atom's group parent is not an array field");
                };
                (
                    self.visual_containing_atom(gp.visual),
                    self.front_array_spec(a.type_, a.front).forward_alignments.clone(),
                )
            }
            _ => panic!("unexpected atom parent kind"),
        };
        let container = container?;
        return self.visual_find_alignment(container, name, Some(&forward));
    }

    /// Alignment for a brick of a leaf visual: looked up on the containing atom
    /// without restriction.
    pub fn leaf_find_alignment(&self, leaf: VisualId, name: &Option<String>) -> Option<AlignId> {
        let name = name.as_ref()?;
        let atom = self.visual_containing_atom(leaf)?;
        return self.visual_find_alignment(atom, name, None);
    }

    // ---- Brick creation ----------------------------------------------------

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
            }
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
            }
            VisualKind::Primitive(_) => return self.line_create_brick(v, 0),
            VisualKind::FieldAtom(fa) => {
                let body = fa.body;
                return self.visual_create_first_brick(body);
            }
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
            }
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
            }
            VisualKind::Symbol(_) => return self.visual_create_first_brick(v),
            VisualKind::Primitive(p) => {
                let last = p.lines.len() - 1;
                return self.line_create_brick(v, last);
            }
            VisualKind::FieldAtom(fa) => {
                let body = fa.body;
                return self.visual_create_last_brick(body);
            }
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
            }
        }
    }

    pub fn visual_get_first_brick(&self, v: VisualId) -> Option<BrickId> {
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                return a.children.iter().find_map(|c| self.visual_get_first_brick(*c))
            }
            VisualKind::Group(g) => {
                return g.children.iter().find_map(|c| self.visual_get_first_brick(*c))
            }
            VisualKind::Symbol(s) => return s.brick,
            VisualKind::Primitive(p) => return p.lines[0].brick,
            VisualKind::FieldAtom(fa) => return self.visual_get_first_brick(fa.body),
            VisualKind::FieldArray(a) => {
                if a.empty.is_some() {
                    return a.empty;
                }
                return a.children.iter().find_map(|c| self.visual_get_first_brick(*c));
            }
        }
    }

    pub fn visual_get_last_brick(&self, v: VisualId) -> Option<BrickId> {
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                return a
                    .children
                    .iter()
                    .rev()
                    .find_map(|c| self.visual_get_last_brick(*c))
            }
            VisualKind::Group(g) => {
                return g
                    .children
                    .iter()
                    .rev()
                    .find_map(|c| self.visual_get_last_brick(*c))
            }
            VisualKind::Symbol(s) => return s.brick,
            VisualKind::Primitive(p) => return p.lines.last().unwrap().brick,
            VisualKind::FieldAtom(fa) => return self.visual_get_last_brick(fa.body),
            VisualKind::FieldArray(a) => {
                if a.empty.is_some() {
                    return a.empty;
                }
                return a
                    .children
                    .iter()
                    .rev()
                    .find_map(|c| self.visual_get_last_brick(*c));
            }
        }
    }

    /// Merman `createOrGetCornerstoneCandidate`: an existing or new brick to
    /// seed brick laying from, or None if the visual has nothing to show.
    pub fn visual_create_or_get_cornerstone_candidate(&mut self, v: VisualId) -> Option<BrickId> {
        match &self.visuals[v].kind {
            VisualKind::Atom(_) | VisualKind::Group(_) => {
                for child in self.visual_children(v) {
                    if let Some(b) = self.visual_create_or_get_cornerstone_candidate(child) {
                        return Some(b);
                    }
                }
                return None;
            }
            VisualKind::Symbol(s) => {
                // Cornerstones can't suddenly disappear without cursor changing
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
            }
            VisualKind::Primitive(_) => return Some(self.line_create_or_get_brick(v, 0)),
            VisualKind::FieldAtom(fa) => {
                let body = fa.body;
                return self.visual_create_or_get_cornerstone_candidate(body);
            }
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
            }
        }
    }

    fn symbol_create_brick(&mut self, v: VisualId) -> BrickId {
        let VisualKind::Symbol(s) = &self.visuals[v].kind else {
            unreachable!();
        };
        let syntax = self.syntax.clone();
        let spec = self.symbol_spec(s.symbol);
        let split = spec.split;
        let align = self.leaf_find_alignment(v, &spec.alignment);
        let split_align = self.leaf_find_alignment(v, &spec.split_alignment);
        let kind = match &spec.kind {
            SymbolKind::Text { text, style } => BrickKind::Text {
                text: text.clone(),
                style: *style,
            },
            SymbolKind::Space {
                width,
                ascent,
                descent,
            } => {
                let to_pixels = syntax.spec_root.to_pixels;
                BrickKind::Empty {
                    ascent: ascent * to_pixels,
                    descent: descent * to_pixels,
                    span: width * to_pixels,
                }
            }
        };
        let brick = self.brick_new(kind, BrickInter::Symbol(v), split, align, split_align);
        let VisualKind::Symbol(s) = &mut self.visuals[v].kind else {
            unreachable!();
        };
        s.brick = Some(brick);
        return brick;
    }

    fn array_create_empty(&mut self, v: VisualId) -> Option<BrickId> {
        let (type_, front) = {
            let a = self.visual_field_array(v);
            (a.type_, a.front)
        };
        let syntax = self.syntax.clone();
        let spec = self.front_array_spec(type_, front);
        let empty = spec.empty.as_ref()?;
        let split = empty.split;
        let align = self.leaf_find_alignment(v, &empty.alignment);
        let split_align = self.leaf_find_alignment(v, &empty.split_alignment);
        let kind = match &empty.kind {
            SymbolKind::Text { text, style } => BrickKind::Text {
                text: text.clone(),
                style: *style,
            },
            SymbolKind::Space {
                width,
                ascent,
                descent,
            } => {
                let to_pixels = syntax.spec_root.to_pixels;
                BrickKind::Empty {
                    ascent: ascent * to_pixels,
                    descent: descent * to_pixels,
                    span: width * to_pixels,
                }
            }
        };
        let brick = self.brick_new(kind, BrickInter::ArrayEmpty(v), split, align, split_align);
        let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
            unreachable!();
        };
        a.empty = Some(brick);
        return Some(brick);
    }

    /// The brick's visual stopped referencing it (merman `brickDestroyed`).
    pub fn brick_inter_destroyed(&mut self, inter: BrickInter) {
        match inter {
            BrickInter::Symbol(v) => {
                let VisualKind::Symbol(s) = &mut self.visuals[v].kind else {
                    unreachable!();
                };
                s.brick = None;
            }
            BrickInter::Line(v, i) => {
                self.visual_primitive_mut(v).lines[i].brick = None;
            }
            BrickInter::ArrayEmpty(v) => {
                let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
                    unreachable!();
                };
                a.empty = None;
            }
        }
    }

    /// Merman `BrickInterface.createNext`.
    pub fn brick_create_next(&mut self, inter: BrickInter) -> ExtendBrickResult {
        match inter {
            BrickInter::Symbol(v) | BrickInter::ArrayEmpty(v) => return self.parent_create_next_brick(v),
            BrickInter::Line(v, i) => {
                if i + 1 == self.visual_primitive(v).lines.len() {
                    return self.parent_create_next_brick(v);
                }
                return self.line_create_brick(v, i + 1);
            }
        }
    }

    pub fn brick_create_previous(&mut self, inter: BrickInter) -> ExtendBrickResult {
        match inter {
            BrickInter::Symbol(v) | BrickInter::ArrayEmpty(v) => {
                return self.parent_create_previous_brick(v)
            }
            BrickInter::Line(v, i) => {
                if i == 0 {
                    return self.parent_create_previous_brick(v);
                }
                return self.line_create_brick(v, i - 1);
            }
        }
    }

    // ---- Parent navigation (merman `VisualParent`) -------------------------

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
            }
            VisualKind::Atom(a) => {
                let children = a.children.clone();
                for at in p.index + 1..children.len() {
                    let r = self.visual_create_first_brick(children[at]);
                    if let ExtendBrickResult::Empty = r {
                        continue;
                    }
                    return r;
                }
                return self.parent_create_next_brick(p.visual);
            }
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
            }
            VisualKind::Atom(a) => {
                let children = a.children.clone();
                for at in (0..p.index).rev() {
                    let r = self.visual_create_last_brick(children[at]);
                    if let ExtendBrickResult::Empty = r {
                        continue;
                    }
                    return r;
                }
                return self.parent_create_previous_brick(p.visual);
            }
            VisualKind::FieldAtom(_) => return self.parent_create_previous_brick(p.visual),
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
            }
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
            }
            VisualKind::FieldAtom(_) => return self.parent_get_previous_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_find_next_brick(&self, child: VisualId) -> Option<BrickId> {
        let p = self.visuals[child].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) | VisualKind::FieldArray(_) | VisualKind::Atom(_) => {
                let children = self.visual_children(p.visual);
                for at in p.index + 1..children.len() {
                    if let Some(b) = self.visual_get_first_brick(children[at]) {
                        return Some(b);
                    }
                }
                return self.parent_find_next_brick(p.visual);
            }
            VisualKind::FieldAtom(_) => return self.parent_find_next_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    pub fn parent_find_previous_brick(&self, child: VisualId) -> Option<BrickId> {
        let p = self.visuals[child].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) | VisualKind::FieldArray(_) | VisualKind::Atom(_) => {
                let children = self.visual_children(p.visual);
                for at in (0..p.index).rev() {
                    if let Some(b) = self.visual_get_last_brick(children[at]) {
                        return Some(b);
                    }
                }
                return self.parent_find_previous_brick(p.visual);
            }
            VisualKind::FieldAtom(_) => return self.parent_find_previous_brick(p.visual),
            _ => panic!("leaf visual used as parent"),
        }
    }

    /// A child's first brick was created: update cursor/hover borders that
    /// start on it and propagate to the parent if the child is first.
    pub fn parent_notify_first_brick_created(&mut self, child: VisualId, brick: BrickId) {
        let Some(p) = self.visuals[child].parent else {
            return;
        };
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) => {
                // The nested atom of an array element: array cursor/hover borders
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
            }
            VisualKind::FieldArray(_) => {
                if p.index == 0 {
                    self.parent_notify_first_brick_created(p.visual, brick);
                }
            }
            VisualKind::Atom(_) => {
                if let Some(sel) = self.visual_selectable_index(p.visual, child) {
                    self.atom_selectable_brick_created(p.visual, sel, brick, true);
                }
                if p.index == 0 {
                    self.parent_notify_first_brick_created(p.visual, brick);
                }
            }
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
            }
            VisualKind::FieldArray(a) => {
                if p.index + 1 == a.children.len() {
                    self.parent_notify_last_brick_created(p.visual, brick);
                }
            }
            VisualKind::Atom(a) => {
                let last = p.index + 1 == a.children.len();
                if let Some(sel) = self.visual_selectable_index(p.visual, child) {
                    self.atom_selectable_brick_created(p.visual, sel, brick, false);
                }
                if last {
                    self.parent_notify_last_brick_created(p.visual, brick);
                }
            }
            VisualKind::FieldAtom(_) => self.parent_notify_last_brick_created(p.visual, brick),
            _ => panic!("leaf visual used as parent"),
        }
    }

    /// Merman `VisualParent.hover`: find what hovering a brick of `child` means.
    pub fn parent_hover(&mut self, child: VisualId, point: Vector) -> Option<(HoverableId, bool)> {
        let p = self.visuals[child].parent?;
        match &self.visuals[p.visual].kind {
            VisualKind::Group(_) => return self.parent_hover(p.visual, point),
            VisualKind::FieldArray(_) => {
                // FrontArrayParent
                if !self.array_group_selectable(p.visual, p.index) {
                    return self.parent_hover(p.visual, point);
                }
                let index = self.array_value_index(p.visual, p.index);
                return self.array_hover_element(p.visual, index);
            }
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
            }
            VisualKind::FieldAtom(_) => return self.parent_hover(p.visual, point),
            _ => panic!("leaf visual used as parent"),
        }
    }

    /// Merman `Brick.hover` through the brick's visual.
    pub fn brick_hover(&mut self, brick: BrickId, point: Vector) -> Option<(HoverableId, bool)> {
        match self.bricks[brick].inter {
            BrickInter::Symbol(v) => return self.parent_hover(v, point),
            BrickInter::Line(v, i) => return self.line_hover(v, i, point),
            BrickInter::ArrayEmpty(v) => return Some(self.array_hover_placeholder(v, brick)),
        }
    }

    // ---- Compact / expand --------------------------------------------------

    pub fn visual_compact(&mut self, v: VisualId) {
        match &self.visuals[v].kind {
            VisualKind::Atom(_) => {
                self.visual_atom_mut(v).compact = true;
                for child in self.visual_children(v) {
                    self.visual_compact(child);
                }
            }
            VisualKind::Group(_) | VisualKind::FieldArray(_) => {
                for child in self.visual_children(v) {
                    // Nested atoms are wrapped in merman and not compacted with the parent
                    if let VisualKind::Atom(_) = &self.visuals[child].kind {
                        continue;
                    }
                    self.visual_compact(child);
                }
            }
            VisualKind::Symbol(s) => {
                if let Some(b) = s.brick {
                    self.brick_layout_properties_changed(b);
                }
            }
            VisualKind::Primitive(p) => {
                if let Some(b) = p.lines[0].brick {
                    self.brick_layout_properties_changed(b);
                }
            }
            VisualKind::FieldAtom(_) => {}
        }
    }

    pub fn visual_expand(&mut self, v: VisualId) {
        match &self.visuals[v].kind {
            VisualKind::Atom(_) => {
                self.visual_atom_mut(v).compact = false;
                for child in self.visual_children(v) {
                    self.visual_expand(child);
                }
            }
            VisualKind::Group(_) | VisualKind::FieldArray(_) => {
                for child in self.visual_children(v) {
                    if let VisualKind::Atom(_) = &self.visuals[child].kind {
                        continue;
                    }
                    self.visual_expand(child);
                }
            }
            VisualKind::Symbol(s) => {
                if let Some(b) = s.brick {
                    self.brick_layout_properties_changed(b);
                }
            }
            VisualKind::Primitive(p) => {
                if let Some(b) = p.lines[0].brick {
                    self.brick_layout_properties_changed(b);
                }
            }
            VisualKind::FieldAtom(_) => {}
        }
    }

    /// Bricks in order, for the expansion check (merman `getLeafBricks`).
    pub fn visual_get_leaf_bricks(&self, v: VisualId, out: &mut Vec<BrickId>) {
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                for c in &a.children {
                    self.visual_get_leaf_bricks(*c, out);
                }
            }
            VisualKind::Group(g) => {
                for c in &g.children {
                    if let VisualKind::Atom(_) = &self.visuals[*c].kind {
                        continue;
                    }
                    self.visual_get_leaf_bricks(*c, out);
                }
            }
            VisualKind::FieldArray(a) => {
                for c in &a.children {
                    self.visual_get_leaf_bricks(*c, out);
                }
            }
            VisualKind::Symbol(s) => {
                if let Some(b) = s.brick {
                    out.push(b);
                }
            }
            VisualKind::Primitive(p) => {
                for l in &p.lines {
                    if let Some(b) = l.brick {
                        out.push(b);
                    }
                }
            }
            VisualKind::FieldAtom(fa) => self.visual_get_leaf_bricks(fa.body, out),
        }
    }

    // ---- Primitives (merman `VisualFieldPrimitive`) ------------------------

    fn primitive_set(&mut self, v: VisualId, text: &str) {
        let unprintable = self.syntax.spec_root.unprintable.clone();
        let mut lines = vec![];
        let mut offset = 0;
        for raw in text.split('\n') {
            lines.push(Line {
                hard: true,
                offset,
                text: printable(raw, &unprintable),
                brick: None,
            });
            offset += 1 + raw.len();
        }
        let p = self.visual_primitive_mut(v);
        p.hard_line_count = lines.len();
        p.lines = lines;
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

    fn primitive_renumber(&mut self, v: VisualId, from: usize, mut offset: usize) {
        let p = self.visual_primitive_mut(v);
        for line in p.lines[from..].iter_mut() {
            if line.hard {
                offset += 1;
            }
            line.offset = offset;
            offset += line.text.len();
        }
    }

    pub fn primitive_soft_wrapped(&self, v: VisualId) -> bool {
        let p = self.visual_primitive(v);
        return p.lines.len() > p.hard_line_count;
    }

    fn line_spec_split_and_alignments(
        &self,
        v: VisualId,
        index: usize,
    ) -> (SpecSplit, Option<AlignId>, Option<AlignId>) {
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
            return (
                spec.split,
                self.leaf_find_alignment(v, &spec.first_alignment),
                self.leaf_find_alignment(v, &spec.first_split_alignment),
            );
        }
        return (
            SpecSplit::Always,
            None,
            self.leaf_find_alignment(
                v,
                if hard {
                    &spec.hard_split_alignment
                } else {
                    &spec.soft_split_alignment
                },
            ),
        );
    }

    fn line_create_brick_internal(&mut self, v: VisualId, index: usize) -> BrickId {
        let (split, align, split_align) = self.line_spec_split_and_alignments(v, index);
        let p = self.visual_primitive(v);
        let style = self.front_primitive_spec(p.type_, p.front).style;
        let text = p.lines[index].text.clone();
        let line_count = p.lines.len();
        let brick = self.brick_new(
            BrickKind::Text { text, style },
            BrickInter::Line(v, index),
            split,
            align,
            split_align,
        );
        self.visual_primitive_mut(v).lines[index].brick = Some(brick);
        if index == 0 {
            self.parent_notify_first_brick_created(v, brick);
        }
        if index + 1 == line_count {
            self.parent_notify_last_brick_created(v, brick);
        }
        return brick;
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

    pub fn line_create_or_get_brick(&mut self, v: VisualId, index: usize) -> BrickId {
        if let Some(b) = self.visual_primitive(v).lines[index].brick {
            return b;
        }
        return self.line_create_brick_internal(v, index);
    }

    fn line_hover(&mut self, v: VisualId, index: usize, point: Vector) -> Option<(HoverableId, bool)> {
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
    }

    /// Merman `primitiveReflow`: re-wrap hard line groups that overflow, or are
    /// well under the edge.
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
                self.primitive_resplit_one(v, i);
                any_over = false;
                all_under = true;
            }
        }
    }

    /// Merman `resplitOne`: wrap the hard line starting at `i` (and its soft
    /// continuations) into as many lines as needed from their current positions.
    fn primitive_resplit_one(&mut self, v: VisualId, i: usize) -> bool {
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
        // Get the full unwrapped text
        let mut text = String::new();
        let mut end_index = i;
        {
            let p = self.visual_primitive(v);
            for j in i..p.lines.len() {
                if j > i && p.lines[j].hard {
                    break;
                }
                text.push_str(&p.lines[j].text);
                end_index += 1;
            }
        }
        let modified_length = text.len();
        let mut changed = false;
        let mut j = i;
        // Wrap text into existing lines
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
                }
            };
            let split = self.resplit_fit(&font, &text, converse);
            let new_text = text[..split].to_string();
            if new_text != self.visual_primitive(v).lines[j].text {
                self.line_set_text(v, j, new_text);
            }
            if self.visual_primitive(v).lines[j].offset != offset {
                changed = true;
            }
            self.visual_primitive_mut(v).lines[j].offset = offset;
            text = text[split..].to_string();
            offset += split;
            j += 1;
        }
        // If text remains, make new lines
        let mut first_line_created = None;
        let mut last_line_created = None;
        if !text.is_empty() {
            first_line_created = Some(j);
            while !text.is_empty() {
                let converse = match spec
                    .soft_split_alignment
                    .as_ref()
                    .and_then(|n| self.visual_find_alignment(atom, n, None))
                {
                    Some(a) => self.aligns[a].converse,
                    None => 0.,
                };
                let split = self.resplit_fit(&font, &text, converse);
                self.visual_primitive_mut(v).lines.insert(
                    j,
                    Line {
                        hard: false,
                        offset,
                        text: text[..split].to_string(),
                        brick: None,
                    },
                );
                self.primitive_lines_shifted(v, j + 1);
                text = text[split..].to_string();
                offset += split;
                j += 1;
                changed = true;
            }
            last_line_created = Some(j);
        }
        // If ran out of text early, delete following soft lines
        if j < end_index {
            changed = true;
            // Destroy the bricks while their lines (and so their line indices) still exist
            let doomed: Vec<(bool, Option<BrickId>)> = self.visual_primitive(v).lines[j..end_index]
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
            self.visual_primitive_mut(v).lines.drain(j..end_index);
            self.primitive_lines_shifted(v, j);
        }
        // Cleanup
        self.primitive_renumber(v, j, offset);
        if let (Some(first), Some(last)) = (first_line_created, last_line_created) {
            self.trigger_idle_lay_bricks_lines(v, first, last - first);
        }
        // Adjust hover/selection
        if let Some(h) = self.hover {
            if let Some(Hoverable::Primitive(ph)) = &self.hoverables[h] {
                if ph.visual == v {
                    if ph.range.begin_offset >= modified_offset_start + modified_length {
                        self.hoverable_primitive_range_nudge(h);
                    } else if ph.range.begin_offset >= modified_offset_start
                        || ph.range.end_offset >= modified_offset_start
                    {
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
        return changed;
    }

    /// Bricks of lines from `from` on refer to their line index; fix them after
    /// inserting or removing lines.
    fn primitive_lines_shifted(&mut self, v: VisualId, from: usize) {
        let bricks: Vec<(usize, BrickId)> = self
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

    /// Merman `ResplitOneBuilder.build`'s split computation: how much of the
    /// text fits from `converse`.
    fn resplit_fit(&mut self, font: &crate::measure::FontSpec, text: &str, converse: f64) -> usize {
        let width = self.measure.measure_width(font, text);
        let edge = converse + width;
        if converse < self.edge && edge > self.edge {
            let edge_offset = self.edge - converse;
            let under = crate::measure::measure_index_at_converse(&mut *self.measure, font, text, edge_offset);
            if under == text.len() {
                return under;
            }
            let mut split = crate::measure::measure_line_before_or_at(text, under);
            if split == 0 {
                split = under;
            }
            if text[..split].chars().count() < 4 {
                // Compact limit
                return text.len();
            }
            return split;
        }
        return text.len();
    }

    fn line_set_text(&mut self, v: VisualId, index: usize, text: String) {
        let brick = self.visual_primitive(v).lines[index].brick;
        self.visual_primitive_mut(v).lines[index].text = text.clone();
        if let Some(b) = brick {
            self.brick_set_text(b, text);
        }
    }
}

fn printable(text: &str, unprintable: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() {
            out.push_str(unprintable);
        } else {
            out.push(c);
        }
    }
    return out;
}

/// Shared forward set for visuals without one.
pub fn empty_forward() -> Rc<HashSet<String>> {
    return Rc::new(HashSet::new());
}
