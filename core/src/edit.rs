use {
    crate::{
        context::{
            Context,
            CursorId,
            VisualId,
        },
        cursor::{
            Cursor,
            RangeLoc,
            back_of_field,
        },
        document::{
            Atom,
            AtomId,
            AtomParent,
            Field,
        },
        matcher::{
            match_group_into,
            match_pair_into,
        },
        patch::Patch,
        spec::SpecBack,
        syntax::{
            FieldKind,
            GAP_PAIR_KEY_PREFIX,
            GapKind,
            TypeId,
        },
        visual::VisualKind,
    },
    serde_json::Value,
    std::collections::HashMap,
};

fn back_ids(back: &SpecBack, out: &mut Vec<bool>) {
    match back {
        SpecBack::Id(id) => out.push(id.unique),
        SpecBack::Pair(p) => {
            back_ids(&p.key, out);
            back_ids(&p.value, out);
        },
        SpecBack::FixedArray(elems) | SpecBack::FixedSubArray(elems) => {
            for e in elems {
                back_ids(e, out);
            }
        },
        SpecBack::FixedRecord(entries) => {
            for e in entries {
                if let Some(v) = &e.value {
                    back_ids(v, out);
                }
            }
        },
        SpecBack::Array(_) |
        SpecBack::Atom(_) |
        SpecBack::FixedLiteral(_) |
        SpecBack::FixedString(_) |
        SpecBack::Literal(_) |
        SpecBack::Number(_) |
        SpecBack::Optional(_) |
        SpecBack::Record(_) |
        SpecBack::String(_) |
        SpecBack::SubArray(_) => {

        },
    }
}

pub struct EditBatch {
    pub new_level: bool,
    pub patches: Vec<Patch>,
    pub select_after: Option<String>,
    pub select_before: Option<String>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct EditUnique {
    pub atom: AtomId,
    pub field: String,
    pub kind: &'static str,
}

impl Context {
    pub fn array_insert_default(&mut self, atom: AtomId, field: &str, index: usize) -> AtomId {
        let (group, record) = self.field_group(atom, field);
        let members = self.syntax.groups[&group].clone();
        let created = if members.len() == 1 {
            self.atom_new_empty(members[0], 0)
        } else if record {
            self.atom_new_empty(self.syntax.type_gap_pair, 0)
        } else {
            self.atom_new_empty(self.syntax.type_gap, 0)
        };
        if record {
            self.pair_key_make_unique(atom, field, created, &[]);
        }
        self.change_array(atom, field, index, 0, vec![created]);
        return created;
    }

    pub fn atom_new_empty(&mut self, type_: TypeId, depth: usize) -> AtomId {
        let syntax = self.syntax.clone();
        let t = syntax.syntax_type(type_);
        let mut fields = HashMap::new();
        let mut children = vec![];
        let mut field_ids: Vec<&String> = t.fields.keys().collect();
        field_ids.sort();
        for id in field_ids {
            let field = match t.fields[id] {
                FieldKind::Primitive => Field::Primitive(String::new()),
                FieldKind::Array => Field::Array(vec![]),
                FieldKind::Atom => {
                    let Some(SpecBack::Atom(a)) = back_of_field(&t.back, id) else {
                        panic!("atom field `{}` has no atom back", id);
                    };
                    let members = &syntax.groups[&a.type_];
                    let child = if depth < 10 && members.len() == 1 {
                        self.atom_new_empty(members[0], depth + 1)
                    } else {
                        self.atom_new_empty(syntax.type_gap, 0)
                    };
                    children.push((child, id.clone()));
                    Field::Atom(child)
                },
            };
            fields.insert(id.clone(), field);
        }
        let mut unique = vec![];
        back_ids(&t.back, &mut unique);
        let mut ids = vec![];
        let mut unique_id = None;
        for u in unique {
            let id = self.ids_next;
            self.ids_next += 1;
            if u {
                unique_id = Some(id);
            }
            ids.push(id);
        }
        let atom = self.document.atoms.len();
        self.document.atoms.push(Atom {
            back_ids: ids,
            fields: fields,
            parent: None,
            path: String::new(),
            type_: type_,
            unique_id: unique_id,
        });
        for (child, field) in children {
            self.document.atoms[child].parent = Some(AtomParent {
                atom: atom,
                field: field,
                index: 0,
            });
        }
        self.atom_visual.resize(self.document.atoms.len(), None);
        return atom;
    }

    pub fn atom_select_into(&mut self, atom: AtomId) -> bool {
        let Some(visual) = self.atom_visual[atom] else {
            return false;
        };
        return self.visual_select_into_any_child(visual);
    }

    pub fn atom_subtree(&self, atom: AtomId) -> Vec<AtomId> {
        let mut out = vec![];
        let mut stack = vec![atom];
        while let Some(a) = stack.pop() {
            out.push(a);
            let mut fields: Vec<_> = self.document.document_atom(a).fields.iter().collect();
            fields.sort_by(|x, y| x.0.cmp(y.0));
            for (_, field) in fields {
                match field {
                    Field::Atom(child) => stack.push(*child),
                    Field::Array(elements) => stack.extend(elements.iter().rev()),
                    Field::Primitive(_) => { },
                }
            }
        }
        return out;
    }

    pub fn edit_break(&mut self) {
        self.edit_last = None;
    }

    fn edit_delete(&mut self, cursor: CursorId) -> bool {
        match self.cursor_get(cursor) {
            Cursor::Atom(c) => {
                let field_visual = self.visual_atom(c.visual).selectable[c.index].1;
                if !matches!(self.visuals[field_visual].kind, VisualKind::FieldAtom(_)) {
                    return false;
                }
                return self.edit_record(None, |ctx| {
                    let gap = ctx.atom_new_empty(ctx.syntax.type_gap, 0);
                    ctx.field_atom_set(field_visual, gap);
                    return true;
                });
            },
            Cursor::Array(c) => {
                let (visual, begin, end) = (c.visual, c.begin_index, c.end_index);
                let (atom, field) = self.array_field(visual);
                return self.edit_record(Some(EditUnique {
                    atom: atom,
                    field: field.clone(),
                    kind: "delete",
                }), |ctx| {
                    let mut replacement = vec![];
                    if ctx.document.document_atom(atom).parent.is_none() &&
                        ctx.array_field_len(atom, &field) == end - begin + 1 {
                        let (_, record) = ctx.field_group(atom, &field);
                        let gap = if record {
                            let gap = ctx.atom_new_empty(ctx.syntax.type_gap_pair, 0);
                            ctx.pair_key_make_unique(atom, &field, gap, &[]);
                            gap
                        } else {
                            ctx.atom_new_empty(ctx.syntax.type_gap, 0)
                        };
                        replacement.push(gap);
                    }
                    ctx.change_array(atom, &field, begin, end - begin + 1, replacement);
                    return true;
                });
            },
            Cursor::Primitive(_) => return self.edit_primitive_delete(cursor, true),
        }
    }

    fn edit_insert(&mut self, cursor: CursorId, after: bool) -> bool {
        let (atom, field, index) = match self.cursor_get(cursor) {
            Cursor::Array(c) => {
                let (atom, field) = self.array_field(c.visual);
                (atom, field, if after {
                    c.end_index + 1
                } else {
                    c.begin_index
                })
            },
            Cursor::Atom(_) | Cursor::Primitive(_) => {
                let visual = self.cursor_visual(cursor);
                let start = match &self.visuals[visual].kind {
                    VisualKind::Atom(a) => a.atom,
                    _ => self.visual_atom(self.visual_containing_atom(visual).unwrap()).atom,
                };
                let mut at = start;
                loop {
                    let Some(parent) = self.document.document_atom(at).parent.clone() else {
                        return false;
                    };
                    if matches!(
                        self.document.document_atom(parent.atom).fields.get(&parent.field),
                        Some(Field::Array(_))
                    ) {
                        break (parent.atom, parent.field, parent.index + if after {
                            1
                        } else {
                            0
                        });
                    }
                    at = parent.atom;
                }
            },
        };
        let kind = if after {
            "insert_after"
        } else {
            "insert_before"
        };
        let created = self.edit_record(Some(EditUnique {
            atom: atom,
            field: field.clone(),
            kind: kind,
        }), |ctx| ctx.array_insert_default(atom, &field, index));
        self.atom_select_into(created);
        return true;
    }

    fn edit_move(&mut self, cursor: CursorId, forward: bool) -> bool {
        let Cursor::Array(c) = self.cursor_get(cursor) else {
            return false;
        };
        let (visual, begin, end) = (c.visual, c.begin_index, c.end_index);
        let (atom, field) = self.array_field(visual);
        let len = self.array_field_len(atom, &field);
        if (forward && end + 1 >= len) || (!forward && begin == 0) {
            return false;
        }
        self.edit_record(Some(EditUnique {
            atom: atom,
            field: field.clone(),
            kind: "move",
        }), |ctx| {
            let moved = ctx.change_array(atom, &field, begin, end - begin + 1, vec![]);
            let to = if forward {
                begin + 1
            } else {
                begin - 1
            };
            ctx.change_array(atom, &field, to, 0, moved.clone());
            ctx.array_select(visual, !forward, to, to + moved.len() - 1);
        });
        return true;
    }

    pub fn edit_paste(&mut self, text: &str) -> bool {
        if !self.config.editable {
            return false;
        }
        let Some(cursor) = self.cursor else {
            return false;
        };
        match self.cursor_get(cursor) {
            Cursor::Primitive(c) => {
                let (visual, begin, end) = (c.visual, c.range.begin_offset, c.range.end_offset);
                let (atom, field) = self.primitive_field(visual);
                return self.edit_record(
                    None,
                    |ctx| ctx.edit_primitive_splice(atom, &field, begin, end - begin, text),
                );
            },
            Cursor::Atom(c) => {
                let field_visual = self.visual_atom(c.visual).selectable[c.index].1;
                let VisualKind::FieldAtom(fa) = &self.visuals[field_visual].kind else {
                    return false;
                };
                let crate::syntax::Front::Atom(f) = &self.syntax.syntax_type(fa.type_).front[fa.front] else {
                    unreachable!();
                };
                let (group, _) = self.field_group(fa.atom, &f.field.clone());
                let Ok(value) = serde_json::from_str::<Value>(text) else {
                    return false;
                };
                let values = match value {
                    Value::Array(values) => values,
                    other => vec![other],
                };
                let mut atoms = vec![];
                for v in &values {
                    match self.paste_match(&group, None, v) {
                        Some(a) => atoms.push(a),
                        None => return false,
                    }
                }
                if atoms.is_empty() {
                    return false;
                }
                return self.edit_record(None, |ctx| {
                    if atoms.len() == 1 {
                        ctx.field_atom_set(field_visual, atoms[0]);
                    } else {
                        let gap = ctx.atom_new_empty(ctx.syntax.type_suffix_gap, 0);
                        ctx.field_atom_set(field_visual, gap);
                        ctx.change_array(gap, "preceding", 0, 0, atoms);
                    }
                    return true;
                });
            },
            Cursor::Array(c) => {
                let (visual, begin, end) = (c.visual, c.begin_index, c.end_index);
                let (atom, field) = self.array_field(visual);
                let (group, record) = self.field_group(atom, &field);
                let Ok(value) = serde_json::from_str::<Value>(text) else {
                    return false;
                };
                let replaced: Vec<AtomId> = {
                    let Some(Field::Array(elements)) = self.document.document_atom(atom).fields.get(&field) else {
                        unreachable!();
                    };
                    elements[begin ..= end].to_vec()
                };
                let mut atoms = vec![];
                match (value, record) {
                    (Value::Object(entries), true) => {
                        for (key, v) in &entries {
                            let Some(a) = self.paste_match(&group, Some(key), v) else {
                                return false;
                            };
                            if !self.pair_key_make_unique(atom, &field, a, &replaced) {
                                return false;
                            }
                            let key = crate::back::pair_key(&self.syntax, &self.document, a);
                            if atoms.iter().any(|b| crate::back::pair_key(&self.syntax, &self.document, *b) == key) {
                                return false;
                            }
                            atoms.push(a);
                        }
                    },
                    (Value::Array(values), false) => {
                        for v in &values {
                            let Some(a) = self.paste_match(&group, None, v) else {
                                return false;
                            };
                            atoms.push(a);
                        }
                    },
                    (other, false) => {
                        let Some(a) = self.paste_match(&group, None, &other) else {
                            return false;
                        };
                        atoms.push(a);
                    },
                    (_, true) => return false,
                }
                if atoms.is_empty() {
                    return false;
                }
                return self.edit_record(None, |ctx| {
                    ctx.change_array(atom, &field, begin, end - begin + 1, atoms);
                    return true;
                });
            },
        }
    }

    fn edit_primitive_delete(&mut self, cursor: CursorId, next: bool) -> bool {
        let Cursor::Primitive(c) = self.cursor_get(cursor) else {
            return false;
        };
        let (visual, begin, end) = (c.visual, c.range.begin_offset, c.range.end_offset);
        let (atom, field) = self.primitive_field(visual);
        let text = self.document.document_primitive(atom, &field).to_string();
        let (from, to) = if begin != end {
            (begin, end)
        } else if next {
            if end == text.len() {
                return false;
            }
            (begin, self.environment.environment_glyph_walker(&text).glyph_after(end))
        } else {
            if begin == 0 {
                return false;
            }
            (self.environment.environment_glyph_walker(&text).glyph_before(begin), begin)
        };
        let done = self.edit_record(Some(EditUnique {
            atom: atom,
            field: field.clone(),
            kind: "text",
        }), |ctx| ctx.edit_primitive_splice(atom, &field, from, to - from, ""));
        if done {
            self.gap_choices_sync();
        }
        return done;
    }

    fn edit_primitive_splice(&mut self, atom: AtomId, field: &str, index: usize, remove: usize, add: &str) -> bool {
        let type_ = self.document.document_atom(atom).type_;
        if let SpecBack::Pair(p) = &self.syntax.syntax_type(type_).back {
            if matches!(&*p.key, SpecBack:: String(f) if f.id == field) {
                let mut key = self.document.document_primitive(atom, field).to_string();
                key.replace_range(index .. index + remove, add);
                let parent = self.document.document_atom(atom).parent.clone().unwrap();
                let Some(Field::Array(siblings)) =
                    self.document.document_atom(parent.atom).fields.get(&parent.field) else {
                        unreachable!();
                    };
                let taken =
                    siblings
                        .iter()
                        .any(|s| *s != atom && crate::back::pair_key(&self.syntax, &self.document, *s) == key);
                if taken {
                    return false;
                }
            }
        }
        self.change_primitive(atom, field, index, remove, add);
        return true;
    }

    pub fn edit_record<T>(&mut self, unique: Option<EditUnique>, f: impl FnOnce(&mut Context) -> T) -> T {
        if self.edit_patches.is_some() {
            return f(self);
        }
        let now = self.environment.environment_now_ms();
        let continues = match (&unique, &self.edit_last) {
            (Some(u), Some((last, time))) => u == last && now - time < 2000.,
            _ => false,
        };
        let select_before = self.cursor_reference().map(|r| r.reference_format());
        self.edit_patches = Some(vec![]);
        let out = f(self);
        let patches = self.edit_patches.take().unwrap();
        if patches.is_empty() {
            return out;
        }
        self.edit_last = unique.map(|u| (u, now));
        let select_after = self.cursor_reference().map(|r| r.reference_format());
        self.edit_outbox.push(EditBatch {
            new_level: !continues,
            patches: patches,
            select_after: select_after,
            select_before: select_before,
        });
        return out;
    }

    pub fn edit_action(&mut self, action: crate::keys::Action) -> bool {
        use crate::keys::Action;

        if !self.config.editable {
            return false;
        }
        let Some(cursor) = self.cursor else {
            return false;
        };
        match action {
            Action::Delete => return self.edit_delete(cursor),
            Action::Cut => {
                if let Cursor::Primitive(c) = self.cursor_get(cursor) {
                    if c.range.begin_offset == c.range.end_offset {
                        return false;
                    }
                    self.cursor_copy();
                    return self.edit_primitive_delete(cursor, true);
                }
                self.cursor_copy();
                return self.edit_delete(cursor);
            },
            Action::Suffix => {
                match self.cursor_get(cursor) {
                    Cursor::Atom(c) => {
                        let field_visual = self.visual_atom(c.visual).selectable[c.index].1;
                        let VisualKind::FieldAtom(fa) = &self.visuals[field_visual].kind else {
                            return false;
                        };
                        if fa.body == usize::MAX {
                            return false;
                        }
                        let old = self.visual_atom(fa.body).atom;
                        let gap = self.edit_record(None, |ctx| {
                            let gap = ctx.atom_new_empty(ctx.syntax.type_suffix_gap, 0);
                            ctx.field_atom_set(field_visual, gap);
                            ctx.change_array(gap, "preceding", 0, 0, vec![old]);
                            return gap;
                        });
                        self.gap_select_text(gap);
                        return true;
                    },
                    Cursor::Array(c) => {
                        let (visual, begin, end) = (c.visual, c.begin_index, c.end_index);
                        let (atom, field) = self.array_field(visual);
                        let (_, record) = self.field_group(atom, &field);
                        if record {
                            return false;
                        }
                        let gap = self.edit_record(None, |ctx| {
                            let gap = ctx.atom_new_empty(ctx.syntax.type_suffix_gap, 0);
                            let moved = ctx.change_array(atom, &field, begin, end - begin + 1, vec![gap]);
                            ctx.change_array(gap, "preceding", 0, 0, moved);
                            return gap;
                        });
                        self.gap_select_text(gap);
                        return true;
                    },
                    Cursor::Primitive(_) => return false,
                }
            },
            Action::InsertBefore => return self.edit_insert(cursor, false),
            Action::InsertAfter => return self.edit_insert(cursor, true),
            Action::MoveBefore => return self.edit_move(cursor, false),
            Action::MoveAfter => return self.edit_move(cursor, true),
            Action::DeletePrevious => return self.edit_primitive_delete(cursor, false),
            Action::DeleteNext => return self.edit_primitive_delete(cursor, true),
            Action::SplitLines => {
                if self.gap_cursor().is_some() {
                    return false;
                }
                return self.edit_type("\n");
            },
            Action::JoinLines => {
                let Cursor::Primitive(c) = self.cursor_get(cursor) else {
                    return false;
                };
                let (visual, begin, end) = (c.visual, c.range.begin_offset, c.range.end_offset);
                let (atom, field) = self.primitive_field(visual);
                let text = self.document.document_primitive(atom, &field).to_string();
                if begin == end {
                    let Some(newline) = text[end..].find('\n') else {
                        return false;
                    };
                    let at = end + newline;
                    return self.edit_record(None, |ctx| {
                        ctx.change_primitive(atom, &field, at, 1, "");
                        ctx.range_set_offsets(RangeLoc::Cursor(cursor), at, at);
                        return true;
                    });
                }
                let selected = &text[begin .. end];
                if !selected.contains('\n') {
                    return false;
                }
                let joined = selected.replace('\n', "");
                return self.edit_record(None, |ctx| {
                    ctx.change_primitive(atom, &field, begin, end - begin, &joined);
                    if let Some(c) = ctx.cursor {
                        ctx.range_set_offsets(RangeLoc::Cursor(c), begin, begin + joined.len());
                    }
                    return true;
                });
            },
            Action::ChoiceNext => return self.gap_choice_move(true),
            Action::ChoicePrevious => return self.gap_choice_move(false),
            Action::Choose => return self.gap_choose_selected(),
            _ => return false,
        }
    }

    pub fn edit_type(&mut self, text: &str) -> bool {
        if !self.config.editable {
            return false;
        }
        let Some(cursor) = self.cursor else {
            return false;
        };
        let Cursor::Primitive(c) = self.cursor_get(cursor) else {
            return false;
        };
        let (visual, begin, end) = (c.visual, c.range.begin_offset, c.range.end_offset);
        let (atom, field) = self.primitive_field(visual);
        if self.gap_cursor().is_some() {
            return self.gap_type(atom, begin, end, text);
        }
        let type_ = self.document.document_atom(atom).type_;
        let t = self.syntax.syntax_type(type_);
        let current = self.document.document_primitive(atom, &field).to_string();
        let mut preview = current.clone();
        preview.replace_range(begin .. end, text);
        let mismatch =
            t.suffix_on_pattern_mismatch && !t.is_pair && self.document.document_atom(atom).parent.is_some() &&
                t.patterns.get(&field).is_some_and(|p| {
                    let glyphs: Vec<String> =
                        unicode_segmentation::UnicodeSegmentation::graphemes(preview.as_str(), true)
                            .map(|g| g.to_string())
                            .collect();
                    return !p.pattern_matches(&glyphs, false);
                });
        if mismatch {
            let typed = format!("{}{}", text, &current[end..]);
            return self.edit_record(None, |ctx| {
                ctx.change_primitive(atom, &field, begin, current.len() - begin, "");
                let gap = ctx.atom_new_empty(ctx.syntax.type_suffix_gap, 0);
                ctx.gap_replace_in_parent(atom, gap);
                ctx.change_array(gap, "preceding", 0, 0, vec![atom]);
                ctx.gap_select_text(gap);
                return ctx.gap_type(gap, 0, 0, &typed);
            });
        }
        return self.edit_record(Some(EditUnique {
            atom: atom,
            field: field.clone(),
            kind: "text",
        }), |ctx| ctx.edit_primitive_splice(atom, &field, begin, end - begin, text));
    }

    pub fn edit_take(&mut self) -> Vec<EditBatch> {
        return std::mem::take(&mut self.edit_outbox);
    }

    pub fn field_group(&self, atom: AtomId, field: &str) -> (String, bool) {
        return self.field_group_of_type(self.document.document_atom(atom).type_, field);
    }

    pub fn field_atom_set(&mut self, field_visual: VisualId, value: AtomId) {
        let VisualKind::FieldAtom(fa) = &self.visuals[field_visual].kind else {
            panic!("visual {} doesn't show a nested atom", field_visual);
        };
        let crate::syntax::Front::Atom(f) = &self.syntax.syntax_type(fa.type_).front[fa.front] else {
            unreachable!();
        };
        let (atom, field, from_array) = (fa.atom, f.field.clone(), f.from_array);
        if from_array {
            let len = self.array_field_len(atom, &field);
            self.change_array(atom, &field, 0, len.min(1), vec![value]);
        } else {
            self.change_atom(atom, &field, value);
        }
    }

    pub fn ids_take_existing(&mut self, atom: AtomId) {
        for a in self.atom_subtree(atom) {
            for id in &self.document.atoms[a].back_ids {
                self.ids_used.insert(*id);
                self.ids_next = self.ids_next.max(id + 1);
            }
        }
    }

    pub fn pair_key_make_unique(&mut self, record: AtomId, field: &str, pair: AtomId, replaced: &[AtomId]) -> bool {
        let taken: Vec<String> = {
            let Some(Field::Array(siblings)) = self.document.document_atom(record).fields.get(field) else {
                panic!("field `{}` is not an array", field);
            };
            siblings
                .iter()
                .filter(|s| **s != pair && !replaced.contains(s))
                .map(|s| crate::back::pair_key(&self.syntax, &self.document, *s))
                .collect()
        };
        let type_ = self.document.document_atom(pair).type_;
        let SpecBack::Pair(p) = &self.syntax.syntax_type(type_).back else {
            panic!("record entry isn't a pair");
        };
        let key_field = match &*p.key {
            SpecBack::String(f) => f.id.clone(),
            _ => {
                let key = crate::back::pair_key(&self.syntax, &self.document, pair);
                return !taken.contains(&key);
            },
        };
        let current = self.document.document_primitive(pair, &key_field).to_string();
        let gap = self.syntax.syntax_type(type_).gap == GapKind::GapPair;
        let mut key = current.clone();
        let mut n = 0;
        while (gap && !key.starts_with(GAP_PAIR_KEY_PREFIX)) || taken.contains(&key) {
            key = if gap {
                format!("{}{}", GAP_PAIR_KEY_PREFIX, n)
            } else {
                format!("{}{}", current, n + 1)
            };
            n += 1;
        }
        if key != current {
            let len = current.len();
            self.document.document_primitive_splice(pair, &key_field, 0, len, &key);
        }
        return true;
    }

    fn paste_match(&mut self, group: &str, key: Option<&str>, value: &Value) -> Option<AtomId> {
        let syntax = self.syntax.clone();
        let matched = match key {
            Some(key) => match_pair_into(&syntax, &mut self.document, group, key, value),
            None => match_group_into(&syntax, &mut self.document, group, value),
        };
        let atom = matched.ok()?;
        self.atom_visual.resize(self.document.atoms.len(), None);
        for a in self.atom_subtree(atom) {
            for i in 0 .. self.document.atoms[a].back_ids.len() {
                let old = self.document.atoms[a].back_ids[i];
                if self.ids_used.contains(&old) {
                    let new = self.ids_next;
                    self.ids_next += 1;
                    self.document.atoms[a].back_ids[i] = new;
                    if self.document.atoms[a].unique_id == Some(old) {
                        self.document.atoms[a].unique_id = Some(new);
                    }
                }
                let id = self.document.atoms[a].back_ids[i];
                self.ids_used.insert(id);
                self.ids_next = self.ids_next.max(id + 1);
            }
        }
        return Some(atom);
    }
}
