use {
    crate::{
        back::{
            BackLocation,
            Element,
            back_atom_segments,
            back_locations,
            field_segments,
        },
        context::{
            Context,
            VisualId,
        },
        cursor::{
            Cursor,
            Hoverable,
            back_of_field,
        },
        document::{
            AtomId,
            Field,
        },
        patch::{
            Patch,
            SetTarget,
        },
        serialize::{
            serialize_atom,
            serialize_literal,
            serialize_pair,
        },
        spec::{
            SpecBack,
            SpecCondition,
        },
        visual::{
            Line,
            VisualKind,
            VisualParent,
        },
    },
    serde_json::{
        Map,
        Value,
    },
};

impl Context {
    pub fn array_field_len(&self, atom: AtomId, field: &str) -> usize {
        let Some(Field::Array(elements)) = self.document.document_atom(atom).fields.get(field) else {
            panic!("field `{}` is not an array", field);
        };
        return elements.len();
    }

    fn atom_conditions_refresh(&mut self, atom: AtomId, field: Option<&str>) {
        let Some(visual) = self.atom_visual[atom] else {
            return;
        };
        let mut stack = self.visual_atom(visual).children.clone();
        while let Some(v) = stack.pop() {
            match &self.visuals[v].kind {
                VisualKind::Group(g) => stack.extend(g.children.iter().copied()),
                VisualKind::FieldArray(a) => stack.extend(a.children.iter().copied()),
                VisualKind::Symbol(s) => {
                    let Some(condition) = &self.symbol_spec(s.symbol).condition else {
                        continue;
                    };
                    let matches = match (condition, field) {
                        (SpecCondition::Empty(c), Some(field)) => c.field == field,
                        (SpecCondition::Precedent(_), None) => true,
                        _ => false,
                    };
                    if !matches {
                        continue;
                    }
                    let (old, brick) = (s.condition, s.brick);
                    let condition = condition.clone();
                    let new = self.symbol_condition_value(&condition, atom);
                    if old == Some(new) {
                        continue;
                    }
                    let VisualKind::Symbol(s) = &mut self.visuals[v].kind else {
                        unreachable!();
                    };
                    s.condition = Some(new);
                    if !new {
                        if let Some(b) = brick {
                            self.brick_destroy(b);
                        }
                    } else {
                        self.parent_lay_bricks_around(v);
                    }
                },
                VisualKind::Atom(_) | VisualKind::FieldAtom(_) | VisualKind::Primitive(_) => { },
            }
        }
    }

    pub fn change_array(
        &mut self,
        atom: AtomId,
        field: &str,
        index: usize,
        remove: usize,
        add: Vec<AtomId>,
    ) -> Vec<AtomId> {
        let back = &self.syntax.syntax_type(self.document.document_atom(atom).type_).back;
        let optional = matches!(back_of_field(back, field), Some(SpecBack::Optional(_)));
        let old_len = self.array_field_len(atom, field);
        if self.edit_patches.is_some() && !optional {
            let (mut path, element) = self.change_field_path(atom, field);
            let patch = match element {
                Element::Index(offset) | Element::Sub(offset, _) => Patch::Splice {
                    path: std::mem::take(&mut path),
                    index: offset + index,
                    remove: remove,
                    add: add.iter().map(|a| serialize_atom(&self.syntax, &self.document, *a)).collect(),
                },
                Element::Record => Patch::SpliceRecord {
                    path: std::mem::take(&mut path),
                    index: index,
                    remove: remove,
                    add: add.iter().map(|a| serialize_pair(&self.syntax, &self.document, *a)).collect(),
                },
                Element::Optional(_) | Element::Value => panic!("field `{}` is not an array", field),
            };
            self.edit_patches.as_mut().unwrap().push(patch);
        }
        let removed = self.document.document_array_splice(atom, field, index, remove, add.clone());
        if self.edit_patches.is_some() && optional {
            let (path, _) = self.change_field_path(atom, field);
            let back = &self.syntax.syntax_type(self.document.document_atom(atom).type_).back;
            let Some(SpecBack::Optional(f)) = back_of_field(back, field) else {
                unreachable!();
            };
            let mut value = Map::new();
            match add.first() {
                Some(a) => value.insert(f.some_key.clone(), serialize_atom(&self.syntax, &self.document, *a)),
                None => value.insert(f.none_key.clone(), Value::Null),
            };
            self.edit_patches.as_mut().unwrap().push(Patch::Set {
                path: path,
                target: SetTarget::Field,
                value: Value::Object(value),
            });
        }
        for v in self.field_visuals(atom, field) {
            match &self.visuals[v].kind {
                VisualKind::FieldArray(_) => {
                    let new_len = self.array_field_len(atom, field);
                    let separator = {
                        let a = self.visual_field_array(v);
                        !self.front_array_spec(a.type_, a.front).separator.is_empty()
                    };
                    let cursor_range = match self.cursor.map(|c| self.cursor_get(c)) {
                        Some(Cursor::Array(c)) if c.visual == v => Some((c.begin_index, c.end_index)),
                        _ => None,
                    };
                    let deep_index = match (cursor_range, self.cursor) {
                        (None, Some(c)) => {
                            let mut at = self.cursor_visual(c);
                            loop {
                                let Some(p) = self.visuals[at].parent else {
                                    break None;
                                };
                                if p.visual == v {
                                    break Some(self.array_value_index(v, p.index));
                                }
                                at = p.visual;
                            }
                        },
                        _ => None,
                    };
                    let hover_here = match self.hover.and_then(|h| self.hoverables[h].as_ref()) {
                        Some(Hoverable::Array { visual, .. }) | Some(Hoverable::ArrayPlaceholder { visual, .. }) => {
                            *visual == v
                        },
                        _ => false,
                    };
                    if hover_here {
                        self.clear_hover();
                    }
                    let children_len = self.visual_field_array(v).children.len();
                    let (visual_index, visual_remove) = if separator {
                        (if index == 0 {
                            0
                        } else {
                            index * 2 - 1
                        }, (remove * 2).min(children_len))
                    } else {
                        (index, remove)
                    };
                    for i in (visual_index .. visual_index + visual_remove).rev() {
                        let child = self.visual_field_array(v).children[i];
                        self.visual_uproot(child, None);
                        let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
                            unreachable!();
                        };
                        a.children.remove(i);
                    }
                    let mut add_index = visual_index;
                    for element in &add {
                        if separator && add_index > 0 {
                            let group = self.array_create_separator(v, add_index);
                            self.visual_field_array_insert(v, add_index, group);
                            add_index += 1;
                        }
                        let group = self.array_create_element(v, *element, add_index);
                        self.visual_field_array_insert(v, add_index, group);
                        add_index += 1;
                    }
                    if separator && visual_index == 0 && !add.is_empty() && new_len > add.len() {
                        let group = self.array_create_separator(v, add_index);
                        self.visual_field_array_insert(v, add_index, group);
                        add_index += 1;
                    }
                    let children = self.visual_field_array(v).children.clone();
                    for (i, child) in children.iter().enumerate().skip(visual_index) {
                        let parent = self.visuals[*child].parent.as_mut().unwrap();
                        parent.index = i;
                    }
                    if !add.is_empty() {
                        if let Some(empty) = self.visual_field_array(v).empty {
                            self.brick_destroy(empty);
                        }
                        'lay: {
                            if visual_index > 0 {
                                if let Some(previous) =
                                    (0 .. visual_index)
                                        .rev()
                                        .find_map(|i| self.visual_get_last_brick(children[i])) {
                                    self.trigger_idle_lay_bricks_after_end(previous);
                                    break 'lay;
                                }
                            }
                            if let Some(next) =
                                (add_index ..
                                    children.len()).find_map(|i| self.visual_get_first_brick(children[i])) {
                                self.trigger_idle_lay_bricks_before_start(next);
                                break 'lay;
                            }
                            self.parent_lay_bricks_around(v);
                        }
                    } else if new_len == 0 {
                        self.parent_lay_bricks_around(v);
                    }
                    let settle = |old: usize| -> usize {
                        if old >= index + remove {
                            return old - remove + add.len();
                        }
                        if old >= index {
                            return (index + add.len().saturating_sub(1)).min(new_len.saturating_sub(1));
                        }
                        return old;
                    };
                    if let Some((begin, end)) = cursor_range {
                        if new_len == 0 {
                            if !self.field_parent_select_parent(atom, field) {
                                self.clear_cursor();
                            }
                        } else {
                            let c = self.cursor.unwrap();
                            self.cursor_array_set_range(c, settle(begin), settle(end));
                        }
                    } else if let Some(deep) = deep_index {
                        if self.cursor.is_none() && deep >= index && deep < index + remove {
                            if new_len == 0 {
                                self.field_parent_select_parent(atom, field);
                            } else {
                                let to = settle(deep);
                                self.array_select(v, true, to, to);
                            }
                        }
                    }
                },
                VisualKind::FieldAtom(_) => {
                    if index == 0 {
                        self.visual_field_atom_changed(v);
                    }
                },
                _ => panic!("array field shown by a {} visual", v),
            }
        }
        let new_len = self.array_field_len(atom, field);
        if (old_len == 0) != (new_len == 0) {
            self.atom_conditions_refresh(atom, Some(field));
        }
        let Some(Field::Array(elements)) = self.document.document_atom(atom).fields.get(field) else {
            unreachable!();
        };
        let neighbors: Vec<AtomId> =
            [index.checked_sub(1), Some(index + add.len())]
                .into_iter()
                .flatten()
                .filter_map(|i| elements.get(i).copied())
                .collect();
        for n in neighbors {
            self.atom_conditions_refresh(n, None);
        }
        self.wall_ensure();
        return removed;
    }

    pub fn change_atom(&mut self, atom: AtomId, field: &str, value: AtomId) -> AtomId {
        if self.edit_patches.is_some() {
            let (path, _) = self.change_field_path(atom, field);
            let Some(Field::Atom(old)) = self.document.document_atom(atom).fields.get(field) else {
                panic!("field `{}` is not an atom", field);
            };
            let old = *old;
            let skip = back_locations(&self.syntax, &self.document, &path).into_iter().filter_map(|l| match l {
                BackLocation::Atom(a) => Some(a),
                BackLocation::Field(..) => None,
            }).position(|a| a == old).expect("atom isn't at its own path");
            let value = serialize_atom(&self.syntax, &self.document, value);
            self.edit_patches.as_mut().unwrap().push(Patch::Set {
                path: path,
                target: SetTarget::Atom { skip: skip },
                value: value,
            });
        }
        let old = self.document.document_atom_set(atom, field, value);
        for v in self.field_visuals(atom, field) {
            self.visual_field_atom_changed(v);
        }
        self.wall_ensure();
        return old;
    }

    fn change_field_path(&self, atom: AtomId, field: &str) -> (Vec<crate::reference::Segment>, Element) {
        let mut path = back_atom_segments(&self.syntax, &self.document, atom);
        let (segments, element) = field_segments(&self.syntax, &self.document, atom, field);
        path.extend(segments);
        return (path, element);
    }

    pub fn change_primitive(&mut self, atom: AtomId, field: &str, index: usize, remove: usize, add: &str) {
        let old_empty = self.document.document_primitive(atom, field).is_empty();
        let old_valid =
            self
                .syntax
                .syntax_primitive_valid(
                    self.document.document_atom(atom).type_,
                    field,
                    self.document.document_primitive(atom, field),
                );
        let type_ = self.document.document_atom(atom).type_;
        let pair_key = match &self.syntax.syntax_type(type_).back {
            SpecBack::Pair(p) => matches!(&*p.key, SpecBack:: String(f) if f.id == field),
            _ => false,
        };
        let mut after = self.document.document_primitive(atom, field).to_string();
        after.replace_range(index .. index + remove, add);
        if self.edit_patches.is_some() {
            let patch = if pair_key {
                let parent =
                    self.document.document_atom(atom).parent.clone().expect("record entry without a record");
                let (path, _) = self.change_field_path(parent.atom, &parent.field);
                Patch::Key {
                    path: path,
                    index: parent.index,
                    key: after.clone(),
                }
            } else {
                let (path, _) = self.change_field_path(atom, field);
                match back_of_field(&self.syntax.syntax_type(type_).back, field) {
                    Some(SpecBack::String(_)) => Patch::Text {
                        path: path,
                        index: index,
                        remove: remove,
                        add: add.to_string(),
                    },
                    Some(back @ (SpecBack::Number(_) | SpecBack::Literal(_))) => Patch::Set {
                        path: path,
                        target: SetTarget::Field,
                        value: serialize_literal(&after, matches!(back, SpecBack::Number(_))),
                    },
                    _ => panic!("field `{}` is not a primitive", field),
                }
            };
            self.edit_patches.as_mut().unwrap().push(patch);
        }
        self.document.document_primitive_splice(atom, field, index, remove, add);
        for v in self.field_visuals(atom, field) {
            self.visual_primitive_mut(v).value.replace_range(index .. index + remove, add);
            if remove > 0 {
                let base = self.primitive_find_containing(v, index);
                let mut remaining = remove;
                let (base_offset, base_text) = {
                    let line = &self.visual_primitive(v).lines[base];
                    (line.offset, line.text.clone())
                };
                let excise_start = index - base_offset;
                let excise_end = (excise_start + remaining).min(base_text.len());
                let mut text = format!("{}{}", &base_text[..excise_start], &base_text[excise_end..]);
                remaining -= excise_end - excise_start;
                let mut remove_lines = 0;
                while remaining > 0 {
                    let (hard, line_text) = {
                        let line = &self.visual_primitive(v).lines[base + 1 + remove_lines];
                        (line.hard, line.text.clone())
                    };
                    if hard {
                        remaining -= 1;
                        self.visual_primitive_mut(v).hard_line_count -= 1;
                    }
                    let excise_end = remaining.min(line_text.len());
                    text.push_str(&line_text[excise_end..]);
                    remaining -= excise_end;
                    remove_lines += 1;
                }
                self.primitive_line_set_text(v, base, text);
                let doomed: Vec<_> =
                    self.visual_primitive(v).lines[base + 1 .. base + 1 + remove_lines]
                        .iter()
                        .filter_map(|l| l.brick)
                        .collect();
                for b in doomed {
                    self.brick_destroy(b);
                }
                self.visual_primitive_mut(v).lines.drain(base + 1 .. base + 1 + remove_lines);
                if remove_lines > 0 {
                    self.primitive_lines_shifted(v, base + 1);
                }
                for line in self.visual_primitive_mut(v).lines[base + 1..].iter_mut() {
                    line.offset -= remove;
                }
            }
            if !add.is_empty() {
                let segments: Vec<&str> = add.split('\n').collect();
                let mut line_index = self.primitive_find_containing(v, index);
                let original = line_index;
                let (line_offset, mut line_text) = {
                    let line = &self.visual_primitive(v).lines[line_index];
                    (line.offset, line.text.clone())
                };
                let at = index - line_offset;
                let mut remainder = None;
                if segments.len() > 1 {
                    remainder = Some(line_text.split_off(at));
                }
                line_text.insert_str(at, segments[0]);
                let mut moving_offset = line_offset + line_text.len();
                self.primitive_line_set_text(v, line_index, line_text);
                for segment in &segments[1..] {
                    line_index += 1;
                    moving_offset += 1;
                    self.visual_primitive_mut(v).lines.insert(line_index, Line {
                        brick: None,
                        hard: true,
                        offset: moving_offset,
                        text: segment.to_string(),
                    });
                    self.visual_primitive_mut(v).hard_line_count += 1;
                    moving_offset += segment.len();
                }
                if let Some(remainder) = remainder {
                    let mut last = self.visual_primitive(v).lines[line_index].text.clone();
                    last.push_str(&remainder);
                    self.primitive_line_set_text(v, line_index, last);
                }
                let created = line_index - original;
                if created > 0 {
                    self.primitive_lines_shifted(v, original + 1);
                }
                for line in self.visual_primitive_mut(v).lines[line_index + 1..].iter_mut() {
                    line.offset += add.len();
                }
                if created > 0 {
                    self.trigger_idle_lay_bricks_lines(v, original + 1, created);
                }
            }
            if let Some(h) = self.hover {
                if matches!(&self.hoverables[h], Some(Hoverable::Primitive(p)) if p.visual == v) {
                    self.clear_hover();
                }
            }
            if let Some(c) = self.cursor {
                if let Cursor::Primitive(p) = self.cursor_get(c) {
                    if p.visual == v {
                        let settle = |old: usize| -> usize {
                            if old < index {
                                return old;
                            }
                            if old >= index + remove {
                                return old - remove + add.len();
                            }
                            return index + add.len();
                        };
                        let (begin, end) = (settle(p.range.begin_offset), settle(p.range.end_offset));
                        self.range_set_offsets(crate::cursor::RangeLoc::Cursor(c), begin, end);
                    }
                }
            }
        }
        let valid = self.syntax.syntax_primitive_valid(type_, field, &after);
        if valid != old_valid {
            for v in self.field_visuals(atom, field) {
                let (type_, front) = {
                    let p = self.visual_primitive(v);
                    (p.type_, p.front)
                };
                let spec = self.front_primitive_spec(type_, front);
                let style = if valid {
                    spec.style
                } else {
                    spec.invalid_style
                };
                let lines: Vec<_> =
                    self
                        .visual_primitive(v)
                        .lines
                        .iter()
                        .filter_map(|l| l.brick.map(|b| (b, l.text.clone())))
                        .collect();
                for (b, text) in lines {
                    if let crate::wall::BrickKind::Line(t, _) = &mut self.bricks[b].kind {
                        t.style = style;
                    }
                    self.brick_set_text(b, text);
                }
            }
        }
        if old_empty != after.is_empty() {
            self.atom_conditions_refresh(atom, Some(field));
        }
        self.wall_ensure();
    }

    fn field_visuals(&self, atom: AtomId, field: &str) -> Vec<VisualId> {
        let Some(visual) = self.atom_visual[atom] else {
            return vec![];
        };
        return self
            .visual_atom(visual)
            .selectable
            .iter()
            .filter(|(f, _)| f == field)
            .map(|(_, v)| *v)
            .collect();
    }

    fn primitive_line_set_text(&mut self, v: VisualId, index: usize, text: String) {
        let brick = self.visual_primitive(v).lines[index].brick;
        self.visual_primitive_mut(v).lines[index].text = text.clone();
        if let Some(b) = brick {
            self.brick_set_text(b, text);
        }
    }

    fn visual_field_array_insert(&mut self, v: VisualId, at: usize, child: VisualId) {
        let VisualKind::FieldArray(a) = &mut self.visuals[v].kind else {
            unreachable!();
        };
        a.children.insert(at, child);
    }

    fn visual_field_atom_changed(&mut self, v: VisualId) {
        if self.visual_field_atom_ellipsize(v) {
            return;
        }
        let (owner, type_, front, body) = {
            let VisualKind::FieldAtom(fa) = &self.visuals[v].kind else {
                unreachable!();
            };
            (fa.atom, fa.type_, fa.front, fa.body)
        };
        let owner_visual = self.visual_containing_atom(v).expect("nested atom visual without atom");
        let fix_deep = self.cursor.is_some_and(|c| {
            let mut at = self.cursor_visual(c);
            loop {
                let Some(p) = self.visuals[at].parent else {
                    return false;
                };
                if p.visual == v {
                    return true;
                }
                at = p.visual;
            }
        });
        let fix_cornerstone = match self.cursor.map(|c| self.cursor_get(c)) {
            Some(Cursor::Atom(c)) => c.visual == owner_visual &&
                self.visual_atom(owner_visual).selectable[c.index].1 == v,
            _ => false,
        };
        if body != usize::MAX {
            self.visual_uproot(body, None);
        }
        let crate::syntax::Front::Atom(f) = &self.syntax.syntax_type(type_).front[front] else {
            unreachable!();
        };
        let child = match self.document.document_atom(owner).fields.get(&f.field) {
            Some(Field::Atom(child)) => Some(*child),
            Some(Field::Array(elements)) => elements.first().copied(),
            _ => panic!("atom field `{}` missing", f.field),
        };
        let depth = self.visuals[v].depth;
        let depth_score = self.visual_atom(owner_visual).depth_score;
        let new_body = match child {
            Some(child) => self.visual_ensure_atom(child, Some(VisualParent {
                visual: v,
                index: 0,
            }), depth + 1, depth_score),
            None => usize::MAX,
        };
        let VisualKind::FieldAtom(fa) = &mut self.visuals[v].kind else {
            unreachable!();
        };
        fa.body = new_body;
        if fix_cornerstone {
            let c = self.cursor.unwrap();
            self.cursor_atom_reset_cornerstone(c);
        } else {
            self.parent_lay_bricks_around(v);
            if fix_deep && self.cursor.is_none() {
                if let Some(child) = child {
                    self.atom_parent_select_field(child);
                }
            }
        }
    }

    fn wall_ensure(&mut self) {
        if !self.wall.children.is_empty() {
            return;
        }
        let root = self.root_visual;
        if let Some(cornerstone) = self.visual_create_or_get_cornerstone_candidate(root) {
            self.wall_set_cornerstone(cornerstone, None, None);
        }
    }
}
