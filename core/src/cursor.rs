use {
    crate::{
        back::{
            back_locate,
            back_reference,
        },
        context::{
            BorderId,
            BrickId,
            CaretId,
            Context,
            CursorId,
            HoverableId,
            TextBorderId,
            VisualId,
        },
        document::{
            AtomId,
            Field,
        },
        reference::Reference,
        serialize::{
            serialize_atom,
            serialize_pair,
        },
        spec::{
            SpecBack,
            SpecObbox,
        },
        stylist::ObboxType,
        syntax::FieldKind,
        visual::VisualKind,
    },
    serde_json::{
        Map,
        Value,
    },
};

fn back_of_field<'a>(back: &'a SpecBack, field: &str) -> Option<&'a SpecBack> {
    match back {
        SpecBack::String(f) | SpecBack::Number(f) | SpecBack::Literal(f) => {
            return if f.id == field {
                Some(back)
            } else {
                None
            };
        },
        SpecBack::Atom(a) => return if a.id == field {
            Some(back)
        } else {
            None
        },
        SpecBack::Array(a) | SpecBack::Record(a) | SpecBack::SubArray(a) => return if a.id == field {
            Some(back)
        } else {
            None
        },
        SpecBack::Optional(a) => return if a.id == field {
            Some(back)
        } else {
            None
        },
        SpecBack::Pair(p) => {
            return back_of_field(&p.key, field).or_else(|| back_of_field(&p.value, field));
        },
        SpecBack::FixedArray(elems) | SpecBack::FixedSubArray(elems) => {
            return elems.iter().find_map(|e| back_of_field(e, field));
        },
        SpecBack::FixedRecord(entries) => {
            return entries.iter().filter_map(|e| e.value.as_ref()).find_map(|v| back_of_field(v, field));
        },
        SpecBack::FixedString(_) | SpecBack::FixedLiteral(_) | SpecBack::Id(_) => return None,
    }
}

pub enum Cursor {
    Array(CursorArray),
    Atom(CursorAtom),
    Primitive(CursorPrimitive),
}

impl Cursor {
    pub fn cursor_kind(&self) -> CursorKind {
        match self {
            Cursor::Atom(_) => return CursorKind::Atom,
            Cursor::Array(_) => return CursorKind::Array,
            Cursor::Primitive(_) => return CursorKind::Primitive,
        }
    }
}

pub struct CursorArray {
    pub begin_index: usize,
    pub border: BorderId,
    pub end_index: usize,
    pub lead_first: bool,
    pub visual: VisualId,
}

pub struct CursorAtom {
    pub border: BorderId,
    pub index: usize,
    pub visual: VisualId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Array,
    Atom,
    Primitive,
}

pub struct CursorPrimitive {
    pub range: RangeState,
    pub visual: VisualId,
}

pub struct DragSelect {
    pub end: Option<Vec<String>>,
    pub start: Vec<String>,
}

pub enum Hoverable {
    Array {
        visual: VisualId,
        index: usize,
        border: BorderId,
    },
    ArrayPlaceholder {
        visual: VisualId,
        border: BorderId,
    },
    Atom {
        visual: VisualId,
        index: usize,
        border: BorderId,
    },
    Primitive(HoverablePrimitive),
}

pub struct HoverablePrimitive {
    pub range: RangeState,
    pub visual: VisualId,
}

pub enum Located {
    Atom(AtomId),
    Field(AtomId, String),
}

#[derive(Clone, Copy)]
pub enum RangeLoc {
    Cursor(CursorId),
    Hoverable(HoverableId),
}

pub struct RangeState {
    pub begin_line: Option<usize>,
    pub begin_offset: usize,
    pub border: Option<TextBorderId>,
    pub caret: Option<CaretId>,
    pub end_line: Option<usize>,
    pub end_offset: usize,
    pub for_selection: bool,
    pub lead_first: bool,
    pub style: SpecObbox,
    pub visual: VisualId,
}

impl RangeState {
    pub fn range_lead_index(&self) -> usize {
        if self.lead_first {
            return self.begin_offset;
        }
        return self.end_offset;
    }
}

impl Context {
    pub fn array_element_brick_created(&mut self, array: VisualId, index: usize, brick: BrickId, first: bool) {
        let mut borders = vec![];
        if let Some(c) = self.cursor {
            if let Cursor::Array(ca) = self.cursor_get(c) {
                if ca.visual == array && (if first {
                    ca.begin_index
                } else {
                    ca.end_index
                }) == index {
                    borders.push(ca.border);
                }
            }
        }
        if let Some(h) = self.hover {
            if let Some(Hoverable::Array { visual, index: i, border }) = &self.hoverables[h] {
                if *visual == array && *i == index {
                    borders.push(*border);
                }
            }
        }
        for b in borders {
            if first {
                self.border_set_first(b, Some(brick));
            } else {
                self.border_set_last(b, Some(brick));
            }
        }
    }

    pub(crate) fn array_field(&self, v: VisualId) -> (AtomId, String) {
        let a = self.visual_field_array(v);
        return (a.atom, self.front_array_spec(a.type_, a.front).field.clone());
    }

    pub fn array_hover_element(&mut self, array: VisualId, index: usize) -> Option<(HoverableId, bool)> {
        if let Some(c) = self.cursor {
            if let Cursor::Array(ca) = self.cursor_get(c) {
                if ca.visual == array && ca.begin_index == ca.end_index && ca.begin_index == index {
                    return None;
                }
            }
        }
        let mut changed = false;
        let id = match self.hover {
            Some(h) if matches!(&self.hoverables[h], Some(Hoverable::Array { visual, .. }) if * visual == array) => h,
            _ => {
                changed = true;
                let border = self.border_new(self.stylist.style_obbox(ObboxType::Hover));
                self.hoverable_push(Hoverable::Array {
                    visual: array,
                    index: index,
                    border: border,
                })
            },
        };
        let (old_index, border) = match self.hoverables[id].as_mut().unwrap() {
            Hoverable::Array { index: i, border, .. } => {
                let old = *i;
                *i = index;
                (old, *border)
            },
            _ => unreachable!(),
        };
        if old_index != index {
            changed = true;
        }
        let element = self.array_element_visual(array, index);
        let first = self.visual_get_first_brick(element);
        let last = self.visual_get_last_brick(element);
        self.border_set_first(border, first);
        self.border_set_last(border, last);
        return Some((id, changed));
    }

    pub fn array_hover_placeholder(&mut self, array: VisualId, brick: BrickId) -> (HoverableId, bool) {
        let border = self.border_new(self.stylist.style_obbox(ObboxType::Hover));
        let id = self.hoverable_push(Hoverable::ArrayPlaceholder {
            visual: array,
            border: border,
        });
        self.border_set_first(border, Some(brick));
        self.border_set_last(border, Some(brick));
        return (id, true);
    }

    pub fn array_select(&mut self, visual: VisualId, lead_first: bool, start: usize, end: usize) {
        if let Some(h) = self.hover {
            let clear = match &self.hoverables[h] {
                Some(Hoverable::Array { visual: hv, index, .. }) => *hv == visual && *index >= start && *index <= end,
                Some(Hoverable::ArrayPlaceholder { visual: hv, .. }) => *hv == visual,
                _ => false,
            };
            if clear {
                self.clear_hover();
            }
        }
        if let Some(c) = self.cursor {
            if let Cursor::Array(ca) = self.cursor_get(c) {
                if ca.visual == visual {
                    self.cursor_array_set_range(c, start, end);
                    return;
                }
            }
        }
        let border = self.border_new(self.stylist.style_obbox(ObboxType::Cursor));
        let id = self.set_cursor(Cursor::Array(CursorArray {
            visual: visual,
            begin_index: start,
            end_index: end,
            lead_first: lead_first,
            border: border,
        }));
        self.cursor_array_set_range(id, start, end);
    }

    pub fn atom_hover_selectable(&mut self, atom: VisualId, index: usize) -> (HoverableId, bool) {
        if let Some(h) = self.hover {
            if let Some(Hoverable::Atom { visual, index: i, .. }) = &self.hoverables[h] {
                if *visual == atom {
                    let changed = *i != index;
                    if changed {
                        self.atom_hoverable_set_index(h, index);
                    }
                    return (h, changed);
                }
            }
        }
        let border = self.border_new(self.stylist.style_obbox(ObboxType::Hover));
        let id = self.hoverable_push(Hoverable::Atom {
            visual: atom,
            index: index,
            border: border,
        });
        self.atom_hoverable_set_index(id, index);
        return (id, true);
    }

    fn atom_hoverable_set_index(&mut self, id: HoverableId, index: usize) {
        let (visual, border) = match self.hoverables[id].as_mut().unwrap() {
            Hoverable::Atom { visual, index: i, border } => {
                *i = index;
                (*visual, *border)
            },
            _ => unreachable!(),
        };
        let field_visual = self.visual_atom(visual).selectable[index].1;
        let first = self.visual_get_first_brick(field_visual);
        let last = self.visual_get_last_brick(field_visual);
        self.border_set_first(border, first);
        self.border_set_last(border, last);
    }

    pub fn atom_parent_select_field(&mut self, atom: AtomId) -> bool {
        let Some(parent_ref) = &self.document.document_atom(atom).parent else {
            return false;
        };
        let (parent_atom, field, index) = (parent_ref.atom, parent_ref.field.clone(), parent_ref.index);
        let parent_visual = self.atom_visual[parent_atom].unwrap();
        let kind =
            *self.syntax.syntax_type(self.document.document_atom(parent_atom).type_).fields.get(&field).unwrap();
        match kind {
            FieldKind::Array => {
                let field_visual =
                    self.visual_atom(parent_visual).selectable.iter().find(|(f, _)| *f == field).unwrap().1;
                return self.field_array_select_into(field_visual, true, index, index);
            },
            FieldKind::Atom => {
                self.atom_select_by_id(parent_visual, &field);
                return true;
            },
            FieldKind::Primitive => unreachable!(),
        }
    }

    pub fn atom_select(&mut self, visual: VisualId, index: usize) {
        if let Some(h) = self.hover {
            if let Some(Hoverable::Atom { visual: hv, index: hi, .. }) = &self.hoverables[h] {
                if *hv == visual && *hi == index {
                    self.clear_hover();
                }
            }
        }
        if let Some(c) = self.cursor {
            if let Cursor::Atom(ca) = self.cursor_get(c) {
                if ca.visual == visual {
                    if let Cursor::Atom(ca) = self.cursor_get_mut(c) {
                        ca.index = index;
                    }
                    self.cursor_atom_reset_cornerstone(c);
                    return;
                }
            }
        }
        let border = self.border_new(self.stylist.style_obbox(ObboxType::Cursor));
        let id = self.set_cursor(Cursor::Atom(CursorAtom {
            visual: visual,
            index: index,
            border: border,
        }));
        self.cursor_atom_reset_cornerstone(id);
    }

    pub fn atom_select_by_id(&mut self, visual: VisualId, field: &str) {
        let index =
            self
                .visual_atom(visual)
                .selectable
                .iter()
                .position(|(f, _)| f == field)
                .unwrap_or_else(|| panic!("field `{}` is not selectable", field));
        self.atom_select(visual, index);
    }

    pub fn atom_selectable_brick_created(&mut self, atom: VisualId, sel: usize, brick: BrickId, first: bool) {
        let mut borders = vec![];
        if let Some(c) = self.cursor {
            if let Cursor::Atom(ca) = self.cursor_get(c) {
                if ca.visual == atom && ca.index == sel {
                    borders.push(ca.border);
                }
            }
        }
        if let Some(h) = self.hover {
            if let Some(Hoverable::Atom { visual, index, border }) = &self.hoverables[h] {
                if *visual == atom && *index == sel {
                    borders.push(*border);
                }
            }
        }
        for b in borders {
            if first {
                self.border_set_first(b, Some(brick));
            } else {
                self.border_set_last(b, Some(brick));
            }
        }
    }

    pub fn atom_syntax_path(&self, atom: AtomId) -> Vec<String> {
        let Some(parent_ref) = &self.document.document_atom(atom).parent else {
            return vec![];
        };
        let mut path = self.field_syntax_path(parent_ref.atom, &parent_ref.field);
        let kind =
            self.syntax.syntax_type(self.document.document_atom(parent_ref.atom).type_).fields[&parent_ref.field];
        if kind == FieldKind::Array {
            path.push(parent_ref.index.to_string());
        }
        return path;
    }

    pub fn clear_cursor(&mut self) {
        let Some(c) = self.cursor.take() else {
            return;
        };
        self.cursor_destroy(c);
    }

    pub fn clear_hover(&mut self) {
        if let Some(h) = self.hover.take() {
            self.hoverable_clear(h);
        }
        if let Some(t) = self.hover_idle {
            self.task_destroy(t);
        }
        self.hover_brick = None;
    }

    fn copy_array(&mut self, array: VisualId, atoms: &[AtomId]) {
        let a = self.visual_field_array(array);
        let field = self.front_array_spec(a.type_, a.front).field.clone();
        let record =
            matches!(back_of_field(&self.syntax.syntax_type(a.type_).back, &field), Some(SpecBack::Record(_)));
        self.copy_atoms(atoms, record);
    }

    pub fn copy_atoms(&mut self, atoms: &[AtomId], record: bool) {
        let value = if record {
            let mut out = Map::new();
            for a in atoms {
                let (k, v) = serialize_pair(&self.syntax, &self.document, *a);
                out.insert(k, v);
            }
            Value::Object(out)
        } else {
            Value::Array(atoms.iter().map(|a| serialize_atom(&self.syntax, &self.document, *a)).collect())
        };
        self.environment.environment_clipboard_set(&serde_json::to_string_pretty(&value).unwrap());
    }

    pub fn cursor_array_set_begin(&mut self, id: CursorId, index: usize) {
        let (visual, border) = match self.cursor_get_mut(id) {
            Cursor::Array(c) => {
                c.lead_first = true;
                c.begin_index = index;
                (c.visual, c.border)
            },
            _ => unreachable!(),
        };
        self.cursor_array_set_cornerstone(visual, index);
        let first = self.visual_get_first_brick(self.array_element_visual(visual, index));
        self.border_set_first(border, first);
    }

    fn cursor_array_set_cornerstone(&mut self, visual: VisualId, index: usize) {
        let element = self.array_element_visual(visual, index);
        let Some(cornerstone) = self.visual_create_or_get_cornerstone_candidate(element) else {
            self.wall.cornerstone = None;
            self.wall.cornerstone_course = None;
            return;
        };
        let (mut find_previous, mut find_next) = (None, None);
        if self.bricks[cornerstone].course.is_none() {
            let children = self.visual_field_array(visual).children.clone();
            let vi = self.array_visual_index(visual, index);
            for at in (0 .. vi).rev() {
                if let Some(b) = self.visual_get_last_brick(children[at]) {
                    find_previous = Some(b);
                    break;
                }
            }
            if find_previous.is_none() {
                find_previous = self.parent_get_previous_brick(visual);
            }
            for at in vi + 1 .. children.len() {
                if let Some(b) = self.visual_get_first_brick(children[at]) {
                    find_next = Some(b);
                    break;
                }
            }
            if find_next.is_none() {
                find_next = self.parent_get_next_brick(visual);
            }
        }
        self.scroll_follow = true;
        self.wall_set_cornerstone(cornerstone, find_previous, find_next);
    }

    pub fn cursor_array_set_end(&mut self, id: CursorId, index: usize) {
        let (visual, border) = match self.cursor_get_mut(id) {
            Cursor::Array(c) => {
                c.lead_first = false;
                c.end_index = index;
                (c.visual, c.border)
            },
            _ => unreachable!(),
        };
        self.cursor_array_set_cornerstone(visual, index);
        let last = self.visual_get_last_brick(self.array_element_visual(visual, index));
        self.border_set_last(border, last);
    }

    pub fn cursor_array_set_position(&mut self, id: CursorId, index: usize) {
        if let Cursor::Array(c) = self.cursor_get_mut(id) {
            c.lead_first = true;
        }
        self.cursor_array_set_range(id, index, index);
    }

    pub fn cursor_array_set_range(&mut self, id: CursorId, begin: usize, end: usize) {
        let (visual, lead_first, border) = match self.cursor_get(id) {
            Cursor::Array(c) => (c.visual, c.lead_first, c.border),
            _ => unreachable!(),
        };
        if let Cursor::Array(c) = self.cursor_get_mut(id) {
            c.begin_index = begin;
            c.end_index = end;
        }
        if lead_first {
            self.cursor_array_set_cornerstone(visual, begin);
        } else {
            self.cursor_array_set_cornerstone(visual, end);
        }
        let first = self.visual_get_first_brick(self.array_element_visual(visual, begin));
        let last = self.visual_get_last_brick(self.array_element_visual(visual, end));
        self.border_set_first(border, first);
        self.border_set_last(border, last);
    }

    pub fn cursor_atom_reset_cornerstone(&mut self, id: CursorId) {
        let (visual, index, border) = match self.cursor_get(id) {
            Cursor::Atom(c) => (c.visual, c.index, c.border),
            _ => unreachable!(),
        };
        let field_visual = self.visual_atom(visual).selectable[index].1;
        let visual_index = self.visuals[field_visual].parent.unwrap().index;
        let children = self.visual_atom(visual).children.clone();
        let cornerstone = self.visual_create_or_get_cornerstone_candidate(field_visual);
        match cornerstone {
            Some(cornerstone) => {
                let (mut find_previous, mut find_next) = (None, None);
                if self.bricks[cornerstone].course.is_none() {
                    for at in (0 .. visual_index).rev() {
                        if let Some(b) = self.visual_get_last_brick(children[at]) {
                            find_previous = Some(b);
                            break;
                        }
                    }
                    if find_previous.is_none() {
                        find_previous = self.parent_get_previous_brick(visual);
                    }
                    for at in visual_index + 1 .. children.len() {
                        if let Some(b) = self.visual_get_first_brick(children[at]) {
                            find_next = Some(b);
                            break;
                        }
                    }
                    if find_next.is_none() {
                        find_next = self.parent_get_next_brick(visual);
                    }
                }
                self.scroll_follow = true;
                self.wall_set_cornerstone(cornerstone, find_previous, find_next);
            },
            None => {
                self.wall.cornerstone = None;
                self.wall.cornerstone_course = None;
            },
        }
        let first = self.visual_get_first_brick(field_visual);
        let last = self.visual_get_last_brick(field_visual);
        self.border_set_first(border, first);
        self.border_set_last(border, last);
    }

    fn cursor_border_containing(&self, brick: BrickId) -> Option<BorderId> {
        let c = self.cursor?;
        let mut at = self.bricks[brick].inter.visual();
        match self.cursor_get(c) {
            Cursor::Atom(ca) => {
                let field = self.visual_atom(ca.visual).selectable[ca.index].1;
                loop {
                    if at == field {
                        return Some(ca.border);
                    }
                    at = self.visuals[at].parent?.visual;
                }
            },
            Cursor::Array(ca) => {
                loop {
                    let p = self.visuals[at].parent?;
                    if p.visual == ca.visual {
                        let index = self.array_value_index(ca.visual, p.index);
                        if index >= ca.begin_index && index <= ca.end_index {
                            return Some(ca.border);
                        }
                        return None;
                    }
                    at = p.visual;
                }
            },
            Cursor::Primitive(_) => return None,
        }
    }

    pub fn cursor_brick_destroying(&mut self, brick: BrickId) {
        let Some(c) = self.cursor else {
            return;
        };
        let border = match self.cursor_get(c) {
            Cursor::Atom(ca) => ca.border,
            Cursor::Array(ca) => ca.border,
            Cursor::Primitive(_) => return,
        };
        let Some(bd) = self.borders[border].as_ref() else {
            return;
        };
        let (first, last) = (bd.first, bd.last);
        if last == Some(brick) {
            let previous =
                self.brick_previous(brick).filter(|p| self.cursor_border_containing(*p) == Some(border));
            self.border_set_last(border, previous);
        }
        if first == Some(brick) {
            let next = self.brick_next(brick).filter(|n| self.cursor_border_containing(*n) == Some(border));
            self.border_set_first(border, next);
        }
    }

    pub fn cursor_brick_laid(&mut self, brick: BrickId, forward: bool) {
        let Some(b) = self.cursor_border_containing(brick) else {
            return;
        };
        if forward {
            self.border_set_last(b, Some(brick));
        } else {
            self.border_set_first(b, Some(brick));
        }
    }

    pub fn cursor_copy(&mut self) {
        let Some(c) = self.cursor else {
            return;
        };
        match self.cursor_get(c) {
            Cursor::Atom(ca) => {
                let field_visual = self.visual_atom(ca.visual).selectable[ca.index].1;
                match &self.visuals[field_visual].kind {
                    VisualKind::FieldAtom(fa) => {
                        let body = self.visual_atom(fa.body).atom;
                        self.copy_atoms(&[body], false);
                    },
                    VisualKind::FieldArray(_) => {
                        let elements = self.array_elements(field_visual);
                        if elements.is_empty() {
                            self.copy_array(field_visual, &[]);
                        } else {
                            let n = elements.len();
                            self.copy_array(field_visual, &elements[0 .. n]);
                        }
                    },
                    VisualKind::Primitive(p) => {
                        let text = p.value.clone();
                        self.environment.environment_clipboard_set(&text);
                    },
                    _ => panic!("unexpected selectable visual"),
                }
            },
            Cursor::Array(ca) => {
                let elements = self.array_elements(ca.visual);
                let (v, b, e) = (ca.visual, ca.begin_index, ca.end_index);
                self.copy_array(v, &elements[b ..= e]);
            },
            Cursor::Primitive(cp) => {
                let text = self.visual_primitive(cp.visual).value.clone();
                let (b, e) = (cp.range.begin_offset, cp.range.end_offset);
                self.environment.environment_clipboard_set(&text[b.min(text.len()) .. e.min(text.len())]);
            },
        }
    }

    fn cursor_destroy(&mut self, id: CursorId) {
        let Some(cursor) = self.cursors[id].take() else {
            return;
        };
        match cursor {
            Cursor::Atom(c) => self.border_destroy(c.border),
            Cursor::Array(c) => self.border_destroy(c.border),
            Cursor::Primitive(mut c) => self.range_destroy_state(&mut c.range),
        }
    }

    pub fn cursor_get(&self, id: CursorId) -> &Cursor {
        return self.cursors[id].as_ref().expect("cursor destroyed");
    }

    fn cursor_get_mut(&mut self, id: CursorId) -> &mut Cursor {
        return self.cursors[id].as_mut().expect("cursor destroyed");
    }

    pub fn cursor_primitive_range_nudge(&mut self, id: CursorId) {
        self.range_nudge(RangeLoc::Cursor(id));
    }

    pub fn cursor_reference(&self) -> Option<Reference> {
        let (located, range) = match self.cursor_get(self.cursor?) {
            Cursor::Atom(c) => {
                let va = self.visual_atom(c.visual);
                let field = va.selectable[c.index].0.clone();
                match self.document.document_atom(va.atom).fields.get(&field) {
                    Some(Field::Atom(child)) => (Located::Atom(*child), None),
                    _ => (Located::Field(va.atom, field), None),
                }
            },
            Cursor::Array(c) => {
                if c.begin_index == c.end_index {
                    (Located::Atom(self.array_elements(c.visual)[c.begin_index]), None)
                } else {
                    let (atom, field) = self.array_field(c.visual);
                    (Located::Field(atom, field), Some((c.begin_index, c.end_index + 1)))
                }
            },
            Cursor::Primitive(c) => {
                let (atom, field) = self.primitive_field(c.visual);
                let (b, e) = (c.range.begin_offset, c.range.end_offset);
                (Located::Field(atom, field), Some((b.min(e), b.max(e))))
            },
        };
        return Some(back_reference(&self.syntax, &self.document, &located, range));
    }

    pub fn cursor_select_reference(&mut self, reference: &Reference) -> bool {
        let mut reference = reference.clone();
        loop {
            let selected = 'select: {
                let Some(target) = back_locate(&self.syntax, &self.document, &reference) else {
                    break 'select false;
                };
                match target.located {
                    Located::Atom(a) => {
                        let Some(parent) = &self.document.document_atom(a).parent else {
                            break 'select false;
                        };
                        if self.atom_visual[parent.atom].is_none() {
                            break 'select false;
                        }
                        break 'select self.atom_parent_select_field(a);
                    },
                    Located::Field(a, field) => {
                        let Some(visual) = self.atom_visual[a] else {
                            break 'select false;
                        };
                        let Some((index, child)) =
                            self
                                .visual_atom(visual)
                                .selectable
                                .iter()
                                .enumerate()
                                .find_map(|(i, (f, v))| (*f == field).then_some((i, *v))) else {
                                break 'select false;
                            };
                        match (&self.visuals[child].kind, target.range) {
                            (VisualKind::Primitive(p), range) => {
                                let len = p.value.len();
                                let (begin, end) = range.unwrap_or((len, len));
                                self.primitive_select(child, true, begin.min(len), end.min(len));
                                break 'select true;
                            },
                            (VisualKind::FieldArray(_), Some((begin, end))) if end > begin => {
                                let len = self.array_elements(child).len();
                                if begin >= len {
                                    break 'select false;
                                }
                                break 'select self.field_array_select_into(
                                    child,
                                    true,
                                    begin,
                                    (end - 1).min(len - 1),
                                );
                            },
                            _ => {
                                self.atom_select(visual, index);
                                break 'select true;
                            },
                        }
                    },
                }
            };
            if selected {
                return true;
            }
            if reference.range.take().is_some() {
                continue;
            }
            if reference.path.pop().is_none() && reference.id.take().is_none() {
                return false;
            }
        }
    }

    pub fn cursor_syntax_path(&self, id: CursorId) -> Vec<String> {
        match self.cursor_get(id) {
            Cursor::Atom(c) => {
                let va = self.visual_atom(c.visual);
                let field = va.selectable[c.index].0.clone();
                return self.field_syntax_path(va.atom, &field);
            },
            Cursor::Array(c) => {
                let (atom, field) = self.array_field(c.visual);
                let mut p = self.field_syntax_path(atom, &field);
                p.push(c.begin_index.to_string());
                return p;
            },
            Cursor::Primitive(c) => {
                let (atom, field) = self.primitive_field(c.visual);
                let mut p = self.field_syntax_path(atom, &field);
                p.push(c.range.range_lead_index().to_string());
                return p;
            },
        }
    }

    pub fn field_array_select_into(&mut self, visual: VisualId, lead_first: bool, start: usize, end: usize) -> bool {
        if self.array_elements(visual).is_empty() {
            return false;
        }
        self.array_select(visual, lead_first, start, end);
        return true;
    }

    pub fn field_parent_select_parent(&mut self, atom: AtomId, field: &str) -> bool {
        let visual = self.atom_visual[atom].unwrap();
        if self.visual_atom(visual).need_intermediate_cursor {
            self.atom_select_by_id(visual, field);
            return true;
        }
        if self.document.document_atom(atom).parent.is_none() {
            return false;
        }
        return self.atom_parent_select_field(atom);
    }

    pub fn field_syntax_path(&self, atom: AtomId, field: &str) -> Vec<String> {
        let mut path = self.atom_syntax_path(atom);
        path.push("named".to_string());
        path.push(field.to_string());
        return path;
    }

    pub fn hoverable_clear(&mut self, id: HoverableId) {
        let Some(h) = self.hoverables[id].take() else {
            return;
        };
        match h {
            Hoverable::Atom { border, .. } |
            Hoverable::Array { border, .. } |
            Hoverable::ArrayPlaceholder { border, .. } => self.border_destroy(
                border,
            ),
            Hoverable::Primitive(mut p) => self.range_destroy_state(&mut p.range),
        }
    }

    pub fn hoverable_primitive_range_nudge(&mut self, id: HoverableId) {
        self.range_nudge(RangeLoc::Hoverable(id));
    }

    fn hoverable_push(&mut self, h: Hoverable) -> HoverableId {
        let id = self.hoverables.len();
        self.hoverables.push(Some(h));
        return id;
    }

    pub fn hoverable_select(&mut self, id: HoverableId) {
        match self.hoverables[id].as_ref().expect("hoverable destroyed") {
            Hoverable::Atom { visual, index, .. } => {
                let (v, i) = (*visual, *index);
                self.atom_select(v, i);
            },
            Hoverable::Array { visual, index, .. } => {
                let (v, i) = (*visual, *index);
                self.array_select(v, true, i, i);
            },
            Hoverable::ArrayPlaceholder { visual, .. } => {
                let v = *visual;
                self.array_select(v, true, 0, 0);
            },
            Hoverable::Primitive(p) => {
                let (v, b, e) = (p.visual, p.range.begin_offset, p.range.end_offset);
                self.primitive_select(v, true, b, e);
            },
        }
    }

    pub fn hoverable_syntax_path(&self, id: HoverableId) -> Vec<String> {
        match self.hoverables[id].as_ref().expect("hoverable destroyed") {
            Hoverable::Atom { visual, index, .. } => {
                let mut p = self.atom_syntax_path(self.visual_atom(*visual).atom);
                p.push(index.to_string());
                return p;
            },
            Hoverable::Array { visual, index, .. } => {
                let (atom, field) = self.array_field(*visual);
                let mut p = self.field_syntax_path(atom, &field);
                p.push(index.to_string());
                return p;
            },
            Hoverable::ArrayPlaceholder { visual, .. } => {
                let (atom, field) = self.array_field(*visual);
                let mut p = self.field_syntax_path(atom, &field);
                p.push("0".to_string());
                return p;
            },
            Hoverable::Primitive(hp) => {
                let (atom, field) = self.primitive_field(hp.visual);
                let mut p = self.field_syntax_path(atom, &field);
                p.push(hp.range.range_lead_index().to_string());
                return p;
            },
        }
    }

    pub(crate) fn primitive_field(&self, v: VisualId) -> (AtomId, String) {
        let p = self.visual_primitive(v);
        return (p.atom, self.front_primitive_spec(p.type_, p.front).field.clone());
    }

    pub fn primitive_hover_position(&mut self, visual: VisualId, offset: usize) -> (HoverableId, bool) {
        let mut changed = false;
        let id = match self.hover {
            Some(h) if matches!(&self.hoverables[h], Some(Hoverable::Primitive(p)) if p.visual == visual) => h,
            _ => {
                changed = true;
                let range = RangeState {
                    for_selection: false,
                    visual: visual,
                    begin_offset: 0,
                    end_offset: 0,
                    begin_line: None,
                    end_line: None,
                    lead_first: true,
                    caret: None,
                    border: None,
                    style: self.syntax.spec_root.hover.clone(),
                };
                self.hoverable_push(Hoverable::Primitive(HoverablePrimitive {
                    visual: visual,
                    range: range,
                }))
            },
        };
        if self.range(RangeLoc::Hoverable(id)).range_lead_index() != offset {
            changed = true;
        }
        self.range_set_offset(RangeLoc::Hoverable(id), offset);
        return (id, changed);
    }

    pub fn primitive_select(&mut self, visual: VisualId, lead_first: bool, begin: usize, end: usize) {
        if let Some(c) = self.cursor {
            if let Cursor::Primitive(cp) = self.cursor_get(c) {
                if cp.visual == visual {
                    if let Cursor::Primitive(cp) = self.cursor_get_mut(c) {
                        cp.range.lead_first = lead_first;
                    }
                    self.range_set_offsets(RangeLoc::Cursor(c), begin, end);
                    return;
                }
            }
        }
        let range = RangeState {
            for_selection: true,
            visual: visual,
            begin_offset: 0,
            end_offset: 0,
            begin_line: None,
            end_line: None,
            lead_first: lead_first,
            caret: None,
            border: None,
            style: self.syntax.spec_root.cursor.clone(),
        };
        let id = self.set_cursor(Cursor::Primitive(CursorPrimitive {
            visual: visual,
            range: range,
        }));
        self.range_set_offsets(RangeLoc::Cursor(id), begin, end);
    }

    pub(crate) fn range(&self, loc: RangeLoc) -> &RangeState {
        match loc {
            RangeLoc::Cursor(c) => match self.cursor_get(c) {
                Cursor::Primitive(p) => return &p.range,
                _ => panic!("range on non-primitive cursor"),
            },
            RangeLoc::Hoverable(h) => match self.hoverables[h].as_ref().expect("hoverable destroyed") {
                Hoverable::Primitive(p) => return &p.range,
                _ => panic!("range on non-primitive hoverable"),
            },
        }
    }

    fn range_destroy_state(&mut self, range: &mut RangeState) {
        if let Some(b) = range.border.take() {
            self.text_border_destroy(b);
        }
        if let Some(c) = range.caret.take() {
            self.caret_destroy(c);
        }
    }

    fn range_mut(&mut self, loc: RangeLoc) -> &mut RangeState {
        match loc {
            RangeLoc::Cursor(c) => match self.cursor_get_mut(c) {
                Cursor::Primitive(p) => return &mut p.range,
                _ => panic!("range on non-primitive cursor"),
            },
            RangeLoc::Hoverable(h) => match self.hoverables[h].as_mut().expect("hoverable destroyed") {
                Hoverable::Primitive(p) => return &mut p.range,
                _ => panic!("range on non-primitive hoverable"),
            },
        }
    }

    pub fn range_nudge(&mut self, loc: RangeLoc) {
        let (b, e) = {
            let r = self.range(loc);
            (r.begin_offset, r.end_offset)
        };
        self.range_set_offsets(loc, b, e);
    }

    pub fn range_set_begin_offset(&mut self, loc: RangeLoc, offset: usize) {
        let end = self.range(loc).end_offset;
        if offset >= end {
            self.range_mut(loc).lead_first = false;
            self.range_set_offsets(loc, end, offset);
        } else {
            self.range_mut(loc).lead_first = true;
            self.range_set_offsets(loc, offset, end);
        }
    }

    pub fn range_set_cornerstone(&mut self, loc: RangeLoc, index: usize) {
        if !self.range(loc).for_selection {
            return;
        }
        let visual = self.range(loc).visual;
        let cornerstone = self.line_create_or_get_brick(visual, index);
        let (mut find_previous, mut find_next) = (None, None);
        if self.bricks[cornerstone].course.is_none() {
            let lines = self.visual_primitive(visual).lines.len();
            for at in (0 .. index).rev() {
                if let Some(b) = self.visual_primitive(visual).lines[at].brick {
                    find_previous = Some(b);
                    break;
                }
            }
            if find_previous.is_none() {
                find_previous = self.parent_find_previous_brick(visual);
            }
            for at in index + 1 .. lines {
                if let Some(b) = self.visual_primitive(visual).lines[at].brick {
                    find_next = Some(b);
                    break;
                }
            }
            if find_next.is_none() {
                find_next = self.parent_find_next_brick(visual);
            }
        }
        self.scroll_follow = true;
        self.wall_set_cornerstone(cornerstone, find_previous, find_next);
    }

    pub fn range_set_end_offset(&mut self, loc: RangeLoc, offset: usize) {
        let begin = self.range(loc).begin_offset;
        if offset <= begin {
            self.range_mut(loc).lead_first = true;
            self.range_set_offsets(loc, offset, begin);
        } else {
            self.range_mut(loc).lead_first = false;
            self.range_set_offsets(loc, begin, offset);
        }
    }

    pub fn range_set_offset(&mut self, loc: RangeLoc, offset: usize) {
        self.range_set_offsets(loc, offset, offset);
    }

    pub fn range_set_offsets(&mut self, loc: RangeLoc, begin_offset: usize, end_offset: usize) {
        let visual = self.range(loc).visual;
        let length = self.visual_primitive(visual).value.len();
        let was_point = {
            let r = self.range(loc);
            r.begin_offset == r.end_offset
        };
        let begin = begin_offset.min(length);
        let end = end_offset.min(length).max(begin);
        {
            let r = self.range_mut(loc);
            r.begin_offset = begin;
            r.end_offset = end;
        }
        if begin == end {
            if let Some(b) = self.range_mut(loc).border.take() {
                self.text_border_destroy(b);
            }
            if self.range(loc).caret.is_none() {
                let style = self.range(loc).style.clone();
                let caret = self.caret_new(style);
                self.range_mut(loc).caret = Some(caret);
            }
            let index = self.primitive_find_containing(visual, begin);
            {
                let r = self.range_mut(loc);
                r.begin_line = Some(index);
                r.end_line = Some(index);
            }
            self.range_set_cornerstone(loc, index);
            let brick = self.visual_primitive(visual).lines[index].brick;
            let line_offset = self.visual_primitive(visual).lines[index].offset;
            let caret = self.range(loc).caret.unwrap();
            self.caret_set_position(caret, brick, begin - line_offset);
        } else {
            if was_point {
                let r = self.range_mut(loc);
                r.begin_line = None;
                r.end_line = None;
            }
            if let Some(c) = self.range(loc).caret {
                self.caret_destroy(c);
                self.range_mut(loc).caret = None;
            }
            let begin_index = self.primitive_find_containing(visual, begin);
            self.range_mut(loc).begin_line = Some(begin_index);
            let new_first_brick = self.visual_primitive(visual).lines[begin_index].brick;
            let new_first_index = begin - self.visual_primitive(visual).lines[begin_index].offset;
            let end_index = self.primitive_find_containing(visual, end);
            self.range_mut(loc).end_line = Some(end_index);
            let new_last_brick = self.visual_primitive(visual).lines[end_index].brick;
            let new_last_index = end - self.visual_primitive(visual).lines[end_index].offset;
            if self.range(loc).border.is_none() {
                let style = self.range(loc).style.clone();
                let border = self.text_border_new(style);
                self.range_mut(loc).border = Some(border);
            }
            if self.range(loc).lead_first {
                if new_first_brick.is_some() {
                    self.range_set_cornerstone(loc, begin_index);
                }
            } else if new_last_brick.is_some() {
                self.range_set_cornerstone(loc, end_index);
            }
            let border = self.range(loc).border.unwrap();
            self.text_border_set_both(border, new_first_brick, new_first_index, new_last_brick, new_last_index);
        }
    }

    fn set_cursor(&mut self, cursor: Cursor) -> CursorId {
        self.select_token += 1;
        let token = self.select_token;
        let old = self.cursor;
        let id = self.cursors.len();
        self.cursors.push(Some(cursor));
        self.cursor = Some(id);
        if let Some(o) = old {
            self.cursor_destroy(o);
        }
        if token != self.select_token {
            return id;
        }
        self.trigger_idle_lay_bricks_outward();
        return id;
    }

    pub fn syntax_locate(&self, path: &[String]) -> Option<Located> {
        return self.document.document_locate(self.document.root, path);
    }

    pub fn visual_select_into_any_child(&mut self, v: VisualId) -> bool {
        let v = 'window: {
            if !self.window {
                break 'window v;
            }
            let atom_visual = match &self.visuals[v].kind {
                VisualKind::Atom(_) => Some(v),
                _ => self.visual_containing_atom(v),
            };
            let Some(atom_visual) = atom_visual else {
                break 'window v;
            };
            let atom = self.visual_atom(atom_visual).atom;
            let was_root = self.root_visual;
            self.window_adjust_minimal_to(atom);
            if self.root_visual == was_root {
                break 'window v;
            }
            break 'window self.atom_visual[atom].unwrap();
        };
        match &self.visuals[v].kind {
            VisualKind::Atom(a) => {
                if a.selectable.is_empty() {
                    return false;
                }
                if a.need_intermediate_cursor {
                    let index = a.default_selection;
                    self.atom_select(v, index);
                } else {
                    let first = a.selectable[0].1;
                    self.visual_select_into_any_child(first);
                }
                return true;
            },
            VisualKind::Group(g) => {
                for child in g.children.clone() {
                    if self.visual_select_into_any_child(child) {
                        return true;
                    }
                }
                return false;
            },
            VisualKind::Symbol(_) => return false,
            VisualKind::Primitive(p) => {
                let len = p.value.len();
                self.primitive_select(v, true, len, len);
                return true;
            },
            VisualKind::FieldAtom(fa) => {
                let (atom, type_, front) = (fa.atom, fa.type_, fa.front);
                let crate::syntax::Front::Atom(f) = &self.syntax.syntax_type(type_).front[front] else {
                    unreachable!();
                };
                let field = f.field.clone();
                let atom_visual = self.atom_visual[atom].unwrap();
                self.atom_select_by_id(atom_visual, &field);
                return true;
            },
            VisualKind::FieldArray(_) => {
                self.field_array_select_into(v, true, 0, 0);
                return true;
            },
        }
    }
}
