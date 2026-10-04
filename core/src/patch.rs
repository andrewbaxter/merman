use {
    crate::{
        back::{
            BackLocation,
            back_locations,
        },
        context::Context,
        cursor::back_of_field,
        document::{
            AtomId,
            Field,
        },
        matcher::{
            match_group_into,
            match_pair_into,
        },
        reference::Segment,
        spec::SpecBack,
    },
    serde::{
        Deserialize,
        Serialize,
    },
    serde_json::{
        Map,
        Value,
    },
    std::collections::HashSet,
};

fn json_at<'a>(value: &'a mut Value, path: &[Segment]) -> Result<&'a mut Value, String> {
    let mut at = value;
    for (i, segment) in path.iter().enumerate() {
        at = match (segment, at) {
            (Segment::Key(key), Value::Object(o)) => o
                .get_mut(key)
                .ok_or_else(|| format!("No key {:?} at {}", key, path_format(&path[..i])))?,
            (Segment::Index(index), Value::Array(a)) => a
                .get_mut(*index)
                .ok_or_else(|| format!("No element {} at {}", index, path_format(&path[..i])))?,
            _ => return Err(format!("Nothing at {}", path_format(&path[..=i]))),
        };
    }
    return Ok(at);
}

pub fn patch_apply_json(root: &mut Value, patch: &Patch) -> Result<Patch, String> {
    match patch {
        Patch::Splice { path, index, remove, add } => {
            let Value::Array(elements) = json_at(root, path)? else {
                return Err(format!("{} isn't an array", path_format(path)));
            };
            if index + remove > elements.len() {
                return Err(format!("{} has no elements {} to {}", path_format(path), index, index + remove));
            }
            let removed = elements.splice(*index .. index + remove, add.iter().cloned()).collect();
            return Ok(Patch::Splice {
                path: path.clone(),
                index: *index,
                remove: add.len(),
                add: removed,
            });
        },
        Patch::SpliceRecord { path, index, remove, add } => {
            let Value::Object(o) = json_at(root, path)? else {
                return Err(format!("{} isn't an object", path_format(path)));
            };
            if index + remove > o.len() {
                return Err(format!("{} has no entries {} to {}", path_format(path), index, index + remove));
            }
            let mut keys = HashSet::new();
            for (i, key) in o.keys().enumerate() {
                if i < *index || i >= index + remove {
                    keys.insert(key.clone());
                }
            }
            for (key, _) in add {
                if !keys.insert(key.clone()) {
                    return Err(format!("{} would have the key {:?} twice", path_format(path), key));
                }
            }
            let mut entries: Vec<(String, Value)> = std::mem::take(o).into_iter().collect();
            let removed = entries.splice(*index .. index + remove, add.iter().cloned()).collect();
            *o = entries.into_iter().collect::<Map<String, Value>>();
            return Ok(Patch::SpliceRecord {
                path: path.clone(),
                index: *index,
                remove: add.len(),
                add: removed,
            });
        },
        Patch::Text { path, index, remove, add } => {
            let Value::String(text) = json_at(root, path)? else {
                return Err(format!("{} isn't a string", path_format(path)));
            };
            if index + remove > text.len() || !text.is_char_boundary(*index) ||
                !text.is_char_boundary(index + remove) {
                return Err(format!("{} has no text {} to {}", path_format(path), index, index + remove));
            }
            let removed = text[*index .. index + remove].to_string();
            text.replace_range(*index .. index + remove, add);
            return Ok(Patch::Text {
                path: path.clone(),
                index: *index,
                remove: add.len(),
                add: removed,
            });
        },
        Patch::Key { path, index, key } => {
            let Value::Object(o) = json_at(root, path)? else {
                return Err(format!("{} isn't an object", path_format(path)));
            };
            if *index >= o.len() {
                return Err(format!("{} has no entry {}", path_format(path), index));
            }
            if o.keys().enumerate().any(|(i, k)| i != *index && k == key) {
                return Err(format!("{} would have the key {:?} twice", path_format(path), key));
            }
            let mut entries: Vec<(String, Value)> = std::mem::take(o).into_iter().collect();
            let old = std::mem::replace(&mut entries[*index].0, key.clone());
            *o = entries.into_iter().collect::<Map<String, Value>>();
            return Ok(Patch::Key {
                path: path.clone(),
                index: *index,
                key: old,
            });
        },
        Patch::Set { path, value, target } => {
            let at = json_at(root, path)?;
            let old = std::mem::replace(at, value.clone());
            return Ok(Patch::Set {
                path: path.clone(),
                value: old,
                target: *target,
            });
        },
    }
}

fn path_format(path: &[Segment]) -> String {
    let mut out = String::from("#");
    for segment in path {
        match segment {
            Segment::Key(key) => out.push_str(&format!(".{:?}", key)),
            Segment::Index(index) => out.push_str(&format!("[{}]", index)),
        }
    }
    return out;
}

pub fn patch_merge(previous: &mut (Patch, Patch), next: &(Patch, Patch)) -> bool {
    match (previous, next) {
        (
            (
                Patch::Text { path, index, remove, add },
                Patch::Text { remove: undo_remove, add: undo_add, index: undo_index, .. },
            ),
            (
                Patch::Text { path: next_path, index: next_index, remove: next_remove, add: next_add },
                Patch::Text { add: next_undo_add, .. },
            ),
        ) if
            path == next_path => {
            if *next_index == *index + add.len() {
                *remove += next_remove;
                add.push_str(next_add);
                *undo_remove += next_add.len();
                undo_add.push_str(next_undo_add);
                return true;
            }
            if *next_index + next_remove == *index {
                *index = *next_index;
                *undo_index = *next_index;
                *remove += next_remove;
                add.insert_str(0, next_add);
                *undo_remove += next_add.len();
                undo_add.insert_str(0, next_undo_add);
                return true;
            }
            return false;
        },
        (
            (Patch::Key { path, index, key }, _),
            (Patch::Key { path: next_path, index: next_index, key: next_key }, _),
        ) if
            path == next_path && index == next_index => {
            *key = next_key.clone();
            return true;
        },
        (
            (Patch::Set { path, value, target }, _),
            (Patch::Set { path: next_path, value: next_value, target: next_target }, _),
        ) if
            path == next_path && target == next_target => {
            *value = next_value.clone();
            return true;
        },
        _ => return false,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Patch {
    Key {
        path: Vec<Segment>,
        index: usize,
        key: String,
    },
    Set {
        path: Vec<Segment>,
        target: SetTarget,
        value: Value,
    },
    Splice {
        path: Vec<Segment>,
        index: usize,
        remove: usize,
        add: Vec<Value>,
    },
    SpliceRecord {
        path: Vec<Segment>,
        index: usize,
        remove: usize,
        add: Vec<(String, Value)>,
    },
    Text {
        path: Vec<Segment>,
        index: usize,
        remove: usize,
        add: String,
    },
}

pub enum PatchApplied {
    Done,
    Reload(Value),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SetTarget {
    Atom {
        skip: usize,
    },
    Document,
    Field,
}

impl Context {
    fn patch_field(
        &self,
        path: &[Segment],
        want: impl Fn(&SpecBack) -> bool,
    ) -> Result<(AtomId, String, usize), String> {
        for location in back_locations(&self.syntax, &self.document, path) {
            let BackLocation::Field(atom, field, offset) = location else {
                continue;
            };
            let back = &self.syntax.syntax_type(self.document.document_atom(atom).type_).back;
            if back_of_field(back, &field).is_some_and(&want) {
                return Ok((atom, field, offset));
            }
        }
        return Err(format!("No field of the right kind at {}", path_format(path)));
    }

    pub fn patch_apply(&mut self, patch: &Patch) -> Result<PatchApplied, String> {
        match patch {
            Patch::Splice { path, index, remove, add } => {
                let (atom, field, offset) =
                    self.patch_field(path, |b| matches!(b, SpecBack::Array(_) | SpecBack::SubArray(_)))?;
                let back = &self.syntax.syntax_type(self.document.document_atom(atom).type_).back;
                let Some(SpecBack::Array(a) | SpecBack::SubArray(a)) = back_of_field(back, &field) else {
                    unreachable!();
                };
                let group = a.element.clone();
                let Some(index) = index.checked_sub(offset) else {
                    return Err(format!("{} has no elements from {}", path_format(path), index));
                };
                let length = self.array_field_len(atom, &field);
                if index + remove > length {
                    return Err(format!("{} has no elements {} to {}", path_format(path), index, index + remove));
                }
                let mut atoms = vec![];
                for value in add {
                    atoms.push(self.patch_match(&group, None, value)?);
                }
                self.change_array(atom, &field, index, *remove, atoms);
                return Ok(PatchApplied::Done);
            },
            Patch::SpliceRecord { path, index, remove, add } => {
                let (atom, field, _) = self.patch_field(path, |b| matches!(b, SpecBack::Record(_)))?;
                let back = &self.syntax.syntax_type(self.document.document_atom(atom).type_).back;
                let Some(SpecBack::Record(a)) = back_of_field(back, &field) else {
                    unreachable!();
                };
                let group = a.element.clone();
                let length = self.array_field_len(atom, &field);
                if index + remove > length {
                    return Err(format!("{} has no entries {} to {}", path_format(path), index, index + remove));
                }
                let mut atoms = vec![];
                for (key, value) in add {
                    atoms.push(self.patch_match(&group, Some(key), value)?);
                }
                self.change_array(atom, &field, *index, *remove, atoms);
                return Ok(PatchApplied::Done);
            },
            Patch::Text { path, index, remove, add } => {
                let (atom, field, _) = self.patch_field(path, |b| matches!(b, SpecBack::String(_)))?;
                let text = self.document.document_primitive(atom, &field);
                if index + remove > text.len() || !text.is_char_boundary(*index) ||
                    !text.is_char_boundary(index + remove) {
                    return Err(format!("{} has no text {} to {}", path_format(path), index, index + remove));
                }
                self.change_primitive(atom, &field, *index, *remove, add);
                return Ok(PatchApplied::Done);
            },
            Patch::Key { path, index, key } => {
                let (atom, field, _) = self.patch_field(path, |b| matches!(b, SpecBack::Record(_)))?;
                let Some(Field::Array(elements)) = self.document.document_atom(atom).fields.get(&field) else {
                    unreachable!();
                };
                let Some(pair) = elements.get(*index).copied() else {
                    return Err(format!("{} has no entry {}", path_format(path), index));
                };
                let SpecBack::Pair(spec) =
                    &self.syntax.syntax_type(self.document.document_atom(pair).type_).back else {
                        return Err(format!("Entry {} of {} isn't a key/value pair", index, path_format(path)));
                    };
                let SpecBack::String(key_field) = &*spec.key else {
                    return Err(format!("Entry {} of {} has a fixed key", index, path_format(path)));
                };
                let key_field = key_field.id.clone();
                let length = self.document.document_primitive(pair, &key_field).len();
                self.change_primitive(pair, &key_field, 0, length, key);
                return Ok(PatchApplied::Done);
            },
            Patch::Set { path, value, target } => match target {
                SetTarget::Document => return Ok(PatchApplied::Reload(value.clone())),
                SetTarget::Atom { skip } => {
                    let atoms: Vec<AtomId> =
                        back_locations(&self.syntax, &self.document, path).into_iter().filter_map(|l| match l {
                            BackLocation::Atom(a) => Some(a),
                            BackLocation::Field(..) => None,
                        }).collect();
                    let Some(old) = atoms.get(*skip).copied() else {
                        return Err(format!("No atom at {}", path_format(path)));
                    };
                    let Some(parent) = self.document.document_atom(old).parent.clone() else {
                        return Err(format!("The atom at {} has no parent", path_format(path)));
                    };
                    let back = &self.syntax.syntax_type(self.document.document_atom(parent.atom).type_).back;
                    let Some(SpecBack::Atom(f)) = back_of_field(back, &parent.field) else {
                        return Err(format!("The atom at {} isn't in an atom field", path_format(path)));
                    };
                    let group = f.type_.clone();
                    let new = self.patch_match(&group, None, value)?;
                    self.change_atom(parent.atom, &parent.field, new);
                    return Ok(PatchApplied::Done);
                },
                SetTarget::Field => {
                    let (atom, field, _) =
                        self.patch_field(
                            path,
                            |b| matches!(b, SpecBack::Number(_) | SpecBack::Literal(_) | SpecBack::Optional(_)),
                        )?;
                    let back = &self.syntax.syntax_type(self.document.document_atom(atom).type_).back;
                    match back_of_field(back, &field) {
                        Some(SpecBack::Optional(f)) => {
                            let (some_key, none_key, group) =
                                (f.some_key.clone(), f.none_key.clone(), f.element.clone());
                            let Value::Object(o) = value else {
                                return Err(format!("{} must be an object", path_format(path)));
                            };
                            let new = match (o.get(&some_key), o.get(&none_key)) {
                                (Some(v), None) if o.len() == 1 => vec![self.patch_match(&group, None, v)?],
                                (None, Some(Value::Null)) if o.len() == 1 => vec![],
                                _ => return Err(
                                    format!(
                                        "{} must be {{{}: ...}} or {{{}: null}}",
                                        path_format(path),
                                        some_key,
                                        none_key
                                    ),
                                ),
                            };
                            let length = self.array_field_len(atom, &field);
                            self.change_array(atom, &field, 0, length, new);
                        },
                        Some(back @ (SpecBack::Number(_) | SpecBack::Literal(_))) => {
                            let number = matches!(back, SpecBack::Number(_));
                            let text =
                                literal_field_text(
                                    value,
                                    number,
                                ).ok_or_else(
                                    || format!("{} isn't a valid value for the field at {}", value, path_format(path)),
                                )?;
                            let length = self.document.document_primitive(atom, &field).len();
                            self.change_primitive(atom, &field, 0, length, &text);
                        },
                        _ => unreachable!(),
                    }
                    return Ok(PatchApplied::Done);
                },
            },
        }
    }

    fn patch_match(&mut self, group: &str, key: Option<&str>, value: &Value) -> Result<AtomId, String> {
        let syntax = self.syntax.clone();
        let matched = match key {
            Some(key) => match_pair_into(&syntax, &mut self.document, group, key, value),
            None => match_group_into(&syntax, &mut self.document, group, value),
        };
        let atom = matched.map_err(|e| format!("The value doesn't fit there:\n{}", e.mismatch_format()))?;
        self.atom_visual.resize(self.document.atoms.len(), None);
        self.ids_take_existing(atom);
        return Ok(atom);
    }
}

pub fn literal_field_text(value: &Value, number_only: bool) -> Option<String> {
    match value {
        Value::Number(n) => return Some(n.to_string()),
        Value::Null if !number_only => return Some("null".to_string()),
        Value::Bool(b) if !number_only => return Some(b.to_string()),
        Value::String(text) => {
            let prefix = if number_only {
                crate::matcher::INVALID_NUMBER_PREFIX
            } else {
                crate::matcher::INVALID_LITERAL_PREFIX
            };
            return text.strip_prefix(prefix).map(|t| t.to_string());
        },
        _ => return None,
    }
}
