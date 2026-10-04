use {
    crate::{
        context::Context,
        cursor::Cursor,
        document::{
            AtomId,
            Field,
        },
        edit::EditUnique,
        syntax::{
            Front,
            StyleId,
            GapKind,
            Symbol,
            SymbolKind,
            TypeId,
        },
        pattern::PatternMatcher,
        visual::VisualKind,
    },
    std::{
        collections::HashSet,
        rc::Rc,
    },
};

#[derive(Clone)]
pub struct GapChoice {
    consume_text: usize,
    consume_preceding: usize,
    cursor_field: Option<String>,
    following: Option<String>,
    pub name: String,
    parsed: Vec<(String, String)>,
    pub preview: Vec<(String, Option<StyleId>)>,
    supply: Vec<(String, Vec<AtomId>)>,
    pub type_: TypeId,
}

pub struct GapChoices {
    pub choices: Vec<GapChoice>,
    pub gap: AtomId,
    pub index: usize,
    pub text: String,
}

struct Candidate {
    following: Option<String>,
    key: Vec<KeyPart>,
    preceding: Vec<(String, String, bool)>,
    preview: Vec<(String, Option<StyleId>)>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct KeyState {
    offset: usize,
    part: usize,
    texts: Vec<String>,
}

enum KeyPart {
    Primitive(String, Option<Rc<PatternMatcher>>),
    Symbol(Vec<String>),
}

fn preview_push(
    preview: &mut Vec<(String, Option<StyleId>)>,
    spaces: &mut Vec<(String, Option<StyleId>)>,
    text: String,
    style: Option<StyleId>,
) {
    if text.trim().is_empty() {
        spaces.push((text, style));
        return;
    }
    preview.extend(spaces.drain(..));
    preview.push((text, style));
}

fn glyphs_of(text: &str) -> Vec<String> {
    return unicode_segmentation::UnicodeSegmentation::graphemes(text, true).map(|g| g.to_string()).collect();
}

fn key_close(key: &[KeyPart], states: Vec<KeyState>) -> Vec<KeyState> {
    let mut out = vec![];
    let mut seen = HashSet::new();
    let mut stack = states;
    while let Some(s) = stack.pop() {
        if !seen.insert(s.clone()) {
            continue;
        }
        if let Some(KeyPart::Primitive(_, pattern)) = key.get(s.part) {
            let i = key[..s.part].iter().filter(|p| matches!(p, KeyPart::Primitive(..))).count();
            if pattern.as_ref().map_or(true, |p| p.pattern_matches(&glyphs_of(&s.texts[i]), false)) {
                stack.push(KeyState {
                    part: s.part + 1,
                    offset: 0,
                    texts: s.texts.clone(),
                });
            }
        }
        out.push(s);
    }
    return out;
}

impl Context {
    pub fn gap_cursor(&self) -> Option<AtomId> {
        let c = self.cursor?;
        let Cursor::Primitive(p) = self.cursor_get(c) else {
            return None;
        };
        let (atom, field) = self.primitive_field(p.visual);
        if field != "gap" || self.syntax.syntax_type(self.document.document_atom(atom).type_).gap == GapKind::None {
            return None;
        }
        return Some(atom);
    }

    pub fn field_group_of_type(&self, type_: TypeId, field: &str) -> (String, bool) {
        let back = &self.syntax.syntax_type(type_).back;
        match crate::cursor::back_of_field(back, field) {
            Some(crate::spec::SpecBack::Atom(a)) => return (a.type_.clone(), false),
            Some(crate::spec::SpecBack::Array(a) | crate::spec::SpecBack::SubArray(a)) => return (
                a.element.clone(),
                false,
            ),
            Some(crate::spec::SpecBack::Optional(a)) => return (a.element.clone(), false),
            Some(crate::spec::SpecBack::Record(a)) => return (a.element.clone(), true),
            _ => panic!("field `{}` holds no atoms", field),
        }
    }

    fn gap_choices_compute(&self, gap: AtomId, text: &str) -> (Vec<GapChoice>, usize, usize) {
        let glyphs = self.environment.environment_split_glyphs(text);
        let gap_type = self.syntax.syntax_type(self.document.document_atom(gap).type_).gap;
        let preceding: Vec<AtomId> = match self.document.document_atom(gap).fields.get("preceding") {
            Some(Field::Array(p)) => p.clone(),
            _ => vec![],
        };
        let Some(parent) = &self.document.document_atom(gap).parent else {
            return (vec![], 0, glyphs.len());
        };
        let (base, _) = self.field_group(parent.atom, &parent.field);
        let mut candidates = vec![];
        let mut seen = HashSet::new();
        let mut stack: Vec<TypeId> = self.syntax.groups[&base].iter().rev().copied().collect();
        while let Some(t) = stack.pop() {
            if !seen.insert(t) {
                continue;
            }
            let info = {
                let type_def = self.syntax.syntax_type(t);
                let mut preceding = vec![];
                let mut key = vec![];
                let mut preview = vec![];
                let mut following = None;
                let mut spaces: Vec<(String, Option<StyleId>)> = vec![];
                let symbol =
                    |
                        key: &mut Vec<KeyPart>,
                        preview: &mut Vec<(String, Option<StyleId>)>,
                        spaces: &mut Vec<(String, Option<StyleId>)>,
                        s: &Symbol,
                    | {
                        let text = match (&s.gap_key, &s.kind) {
                            (Some(k), _) if !k.is_empty() => k.clone(),
                            (None, SymbolKind::Text { text, .. }) => text.clone(),
                            _ => return,
                        };
                        match &s.kind {
                            SymbolKind::Text { text, style } => preview_push(
                                preview,
                                spaces,
                                text.clone(),
                                Some(*style),
                            ),
                            SymbolKind::Space { .. } => preview_push(preview, spaces, " ".to_string(), None),
                        }
                        key.push(KeyPart::Symbol(self.environment.environment_split_glyphs(&text)));
                    };
                for front in &type_def.front {
                    match front {
                        Front::Symbol(s) => symbol(&mut key, &mut preview, &mut spaces, s),
                        Front::Array(a) => {
                            if a.prefix.is_empty() {
                                if !key.is_empty() {
                                    following = Some(a.field.clone());
                                    break;
                                }
                                let (group, _) = self.field_group_of_type(t, &a.field);
                                preceding.push((a.field.clone(), group, true));
                            } else {
                                for p in &a.prefix {
                                    symbol(&mut key, &mut preview, &mut spaces, p);
                                }
                                following = Some(a.field.clone());
                                break;
                            }
                        },
                        Front::Atom(a) => {
                            if !key.is_empty() {
                                following = Some(a.field.clone());
                                break;
                            }
                            let (group, _) = self.field_group_of_type(t, &a.field);
                            preceding.push((a.field.clone(), group, false));
                        },
                        Front::Primitive(p) => {
                            let gap_style =
                                self.syntax.syntax_type(self.syntax.type_gap).front.iter().find_map(|f| match f {
                                    Front::Primitive(p) => Some(p.style),
                                    _ => None,
                                });
                            preview_push(&mut preview, &mut spaces, "▢".to_string(), gap_style);
                            key.push(KeyPart::Primitive(p.field.clone(), type_def.patterns.get(&p.field).cloned()));
                        },
                    }
                }
                Candidate {
                    preceding: preceding,
                    key: key,
                    preview: preview,
                    following: following,
                }
            };
            match gap_type {
                GapKind::Gap | GapKind::GapPair => {
                    match info.preceding.first() {
                        None => candidates.push((t, 0, vec![], info)),
                        Some((_, group, _)) => {
                            stack.extend(self.syntax.groups[group].iter().rev().copied());
                        },
                    }
                },
                GapKind::SuffixGap => {
                    for (_, group, _) in info.preceding.iter().rev() {
                        stack.extend(self.syntax.groups[group].iter().rev().copied());
                    }

                    fn walk(
                        context: &Context,
                        fronts: &[(String, String, bool)],
                        atoms: &[AtomId],
                        front: usize,
                        taken: usize,
                        assigned: &mut Vec<Vec<AtomId>>,
                        best: &mut Option<(usize, Vec<Vec<AtomId>>)>,
                    ) {
                        if front == fronts.len() {
                            if best.as_ref().map_or(true, |(n, _)| taken > *n) {
                                *best = Some((taken, assigned.clone()));
                            }
                            return;
                        }
                        let (_, group, array) = &fronts[fronts.len() - 1 - front];
                        let array = *array;
                        let fits =
                            |a: AtomId| context
                                .syntax
                                .groups[group].contains(&context.document.document_atom(a).type_);
                        let next = (taken < atoms.len()).then(|| atoms[atoms.len() - 1 - taken]);
                        if array {
                            walk(context, fronts, atoms, front + 1, taken, assigned, best);
                        }
                        if let Some(a) = next.filter(|a| fits(*a)) {
                            assigned[front].insert(0, a);
                            if array {
                                walk(context, fronts, atoms, front, taken + 1, assigned, best);
                            } else {
                                walk(context, fronts, atoms, front + 1, taken + 1, assigned, best);
                            }
                            assigned[front].remove(0);
                        }
                    }

                    let mut best = None;
                    let mut assigned = vec![
                        vec![];
                        info.preceding.len()
                    ];
                    walk(self, &info.preceding, &preceding, 0, 0, &mut assigned, &mut best);
                    if let Some((taken, mut assigned)) = best.filter(|(taken, _)| *taken > 0) {
                        assigned.reverse();
                        let supply = info.preceding.iter().map(|(f, _, _)| f.clone()).zip(assigned).collect();
                        candidates.push((t, taken, supply, info));
                    }
                },
                GapKind::None => unreachable!(),
            }
        }
        let mut all_states = vec![];
        let mut longest = 0;
        for (_, _, _, info) in &candidates {
            let primitives = info.key.iter().filter(|p| matches!(p, KeyPart::Primitive(..))).count();
            let mut states = key_close(&info.key, vec![KeyState {
                part: 0,
                offset: 0,
                texts: vec![
                    String::new();
                    primitives
                ],
            }]);
            let mut per_length = vec![states.clone()];
            for g in &glyphs {
                if states.is_empty() {
                    break;
                }
                states = {
                    let mut out = vec![];
                    for s in &states {
                        match info.key.get(s.part) {
                            Some(KeyPart::Primitive(_, pattern)) => {
                                let mut texts = s.texts.clone();
                                let i =
                                    info.key[..s.part]
                                        .iter()
                                        .filter(|p| matches!(p, KeyPart::Primitive(..)))
                                        .count();
                                texts[i].push_str(g);
                                if !pattern
                                    .as_ref()
                                    .map_or(true, |p| p.pattern_matches(&glyphs_of(&texts[i]), true)) {
                                    continue;
                                }
                                out.push(KeyState {
                                    part: s.part,
                                    offset: 0,
                                    texts: texts,
                                });
                            },
                            Some(KeyPart::Symbol(glyphs)) => {
                                if glyphs.get(s.offset).map(|g| g.as_str()) != Some(g.as_str()) {
                                    continue;
                                }
                                if s.offset + 1 == glyphs.len() {
                                    out.push(KeyState {
                                        part: s.part + 1,
                                        offset: 0,
                                        texts: s.texts.clone(),
                                    });
                                } else {
                                    out.push(KeyState {
                                        part: s.part,
                                        offset: s.offset + 1,
                                        texts: s.texts.clone(),
                                    });
                                }
                            },
                            None => { },
                        }
                    }
                    key_close(&info.key, out)
                };
                if !states.is_empty() {
                    per_length.push(states.clone());
                }
            }
            longest = longest.max(per_length.len() - 1);
            all_states.push(per_length);
        }
        let mut complete = vec![];
        let mut incomplete = vec![];
        for ((t, taken, supply, info), per_length) in candidates.into_iter().zip(all_states) {
            let Some(states) = per_length.get(longest) else {
                continue;
            };
            let best =
                states
                    .iter()
                    .find(|s| s.part == info.key.len())
                    .or_else(|| states.iter().max_by_key(|s| (s.part, s.offset)))
                    .unwrap();
            let is_complete = best.part == info.key.len();
            let primitive_fields: Vec<String> = info.key.iter().filter_map(|p| match p {
                KeyPart::Primitive(f, _) => Some(f.clone()),
                KeyPart::Symbol(_) => None,
            }).collect();
            let parsed: Vec<(String, String)> =
                primitive_fields.iter().cloned().zip(best.texts.iter().cloned()).collect();
            let next_primitive_after = |part: usize| info.key[part..].iter().find_map(|p| match p {
                KeyPart::Primitive(f, _) => Some(f.clone()),
                KeyPart::Symbol(_) => None,
            });
            let cursor_field = if is_complete {
                match info.key.last() {
                    Some(KeyPart::Primitive(f, _)) => Some(f.clone()),
                    _ => None,
                }
            } else {
                match &info.key[best.part] {
                    KeyPart::Primitive(f, _) => Some(f.clone()),
                    KeyPart::Symbol(_) if best.offset > 0 => next_primitive_after(best.part + 1),
                    KeyPart::Symbol(_) => match best.part.checked_sub(1).map(|p| &info.key[p]) {
                        Some(KeyPart::Primitive(f, _)) => Some(f.clone()),
                        _ => None,
                    },
                }
            };
            let choice = GapChoice {
                type_: t,
                name: self.syntax.syntax_type(t).name.clone(),
                preview: info.preview.clone(),
                consume_text: longest,
                consume_preceding: taken,
                supply: supply,
                parsed: parsed,
                cursor_field: cursor_field,
                following: info.following.clone(),
            };
            if is_complete {
                complete.push(choice);
            } else {
                incomplete.push(choice);
            }
        }
        complete.extend(incomplete);
        return (complete, longest, glyphs.len());
    }

    pub fn gap_choice_move(&mut self, next: bool) -> bool {
        self.gap_choices_sync();
        let Some(choices) = self.gap_choices.as_mut() else {
            return false;
        };
        let count = choices.choices.len();
        if count == 0 {
            return false;
        }
        choices.index = if next {
            (choices.index + 1) % count
        } else {
            (choices.index + count - 1) % count
        };
        return true;
    }

    fn gap_choose(&mut self, gap: AtomId, choice: GapChoice, text: &str) {
        let glyphs = self.environment.environment_split_glyphs(text);
        let remainder: String = glyphs[choice.consume_text..].concat();
        let following = choice.following.clone();
        self.edit_record(None, |ctx| {
            let created = ctx.atom_new_empty(choice.type_, 0);
            for (field, value) in &choice.parsed {
                let length = ctx.document.document_primitive(created, field).len();
                ctx.document.document_primitive_splice(created, field, 0, length, value);
            }
            if choice.consume_preceding > 0 {
                let length = ctx.array_field_len(gap, "preceding");
                ctx.change_array(
                    gap,
                    "preceding",
                    length - choice.consume_preceding,
                    choice.consume_preceding,
                    vec![],
                );
                for (field, atoms) in &choice.supply {
                    match ctx.document.document_atom(created).fields.get(field) {
                        Some(Field::Atom(_)) => {
                            ctx.document.document_atom_set(created, field, atoms[0]);
                        },
                        Some(Field::Array(_)) => {
                            ctx.document.document_array_splice(created, field, 0, 0, atoms.clone());
                        },
                        _ => panic!("preceding front field `{}` holds no atoms", field),
                    }
                }
            }
            let place_suffix = remainder.is_empty() && choice.cursor_field.is_none() && following.is_none();
            if place_suffix || (!remainder.is_empty() && following.is_none()) {
                match ctx.syntax.syntax_type(ctx.document.document_atom(gap).type_).gap {
                    GapKind::SuffixGap => {
                        let length = ctx.array_field_len(gap, "preceding");
                        ctx.change_array(gap, "preceding", length, 0, vec![created]);
                        let text = ctx.document.document_primitive(gap, "gap").len();
                        ctx.change_primitive(gap, "gap", 0, text, "");
                        ctx.gap_select_text(gap);
                    },
                    GapKind::GapPair => {
                        ctx.gap_replace_in_parent(gap, created);
                        ctx.atom_select_into(created);
                    },
                    GapKind::Gap => {
                        let wrap = ctx.atom_new_empty(ctx.syntax.type_suffix_gap, 0);
                        ctx.gap_replace_in_parent(gap, wrap);
                        ctx.change_array(wrap, "preceding", 0, 0, vec![created]);
                        ctx.gap_select_text(wrap);
                    },
                    GapKind::None => unreachable!(),
                }
            } else {
                let suffix =
                    ctx.syntax.syntax_type(ctx.document.document_atom(gap).type_).gap == GapKind::SuffixGap;
                if suffix && ctx.array_field_len(gap, "preceding") > 0 {
                    let length = ctx.array_field_len(gap, "preceding");
                    ctx.change_array(gap, "preceding", length, 0, vec![created]);
                    let text = ctx.document.document_primitive(gap, "gap").len();
                    ctx.change_primitive(gap, "gap", 0, text, "");
                } else {
                    ctx.gap_replace_in_parent(gap, created);
                }
                if !remainder.is_empty() {
                    ctx.gap_select_into_following(created, following.as_deref().unwrap());
                } else if let Some(field) = &choice.cursor_field {
                    ctx.primitive_select_field(created, field);
                } else {
                    ctx.gap_select_into_following(created, following.as_deref().unwrap());
                }
            }
        });
        self.gap_choices = None;
        if !remainder.is_empty() {
            self.edit_type(&remainder);
        }
        self.gap_choices_sync();
    }

    pub fn gap_choose_selected(&mut self) -> bool {
        self.gap_choices_sync();
        let Some(choices) = self.gap_choices.as_ref() else {
            return false;
        };
        let Some(choice) = choices.choices.get(choices.index).cloned() else {
            return false;
        };
        let (gap, text) = (choices.gap, choices.text.clone());
        self.gap_choose(gap, choice, &text);
        return true;
    }

    pub fn gap_choices_sync(&mut self) -> bool {
        let Some(gap) = self.gap_cursor() else {
            return self.gap_choices.take().is_some();
        };
        let text = self.document.document_primitive(gap, "gap").to_string();
        if self.gap_choices.as_ref().is_some_and(|c| c.gap == gap && c.text == text) {
            return false;
        }
        let (choices, _, _) = self.gap_choices_compute(gap, &text);
        self.gap_choices = Some(GapChoices {
            choices: choices,
            gap: gap,
            index: 0,
            text: text,
        });
        return true;
    }

    pub fn gap_exit(&mut self) -> bool {
        let Some(gap) = self.gap_cursor() else {
            return false;
        };
        let Some(parent) = self.document.document_atom(gap).parent.clone() else {
            return false;
        };
        self.atom_parent_select_field(gap);
        if !self.document.document_primitive(gap, "gap").is_empty() {
            return true;
        }
        let in_array =
            matches!(self.document.document_atom(parent.atom).fields.get(&parent.field), Some(Field::Array(_)));
        match self.syntax.syntax_type(self.document.document_atom(gap).type_).gap {
            GapKind::Gap | GapKind::GapPair => {
                if !in_array || self.document.document_atom(parent.atom).parent.is_none() ||
                    self.array_field_len(parent.atom, &parent.field) != 1 {
                    return true;
                }
                self.edit_record(None, |ctx| {
                    ctx.change_array(parent.atom, &parent.field, 0, 1, vec![]);
                });
            },
            GapKind::SuffixGap => {
                let preceding: Vec<AtomId> = match self.document.document_atom(gap).fields.get("preceding") {
                    Some(Field::Array(p)) => p.clone(),
                    _ => unreachable!(),
                };
                let (group, _) = self.field_group(parent.atom, &parent.field);
                let fits =
                    preceding
                        .iter()
                        .all(|a| self.syntax.groups[&group].contains(&self.document.document_atom(*a).type_));
                if preceding.is_empty() || !fits || (!in_array && preceding.len() != 1) {
                    return true;
                }
                self.edit_record(None, |ctx| {
                    let moved = ctx.change_array(gap, "preceding", 0, preceding.len(), vec![]);
                    if in_array {
                        ctx.change_array(parent.atom, &parent.field, parent.index, 1, moved);
                    } else {
                        ctx.change_atom(parent.atom, &parent.field, moved[0]);
                    }
                });
            },
            GapKind::None => unreachable!(),
        }
        return true;
    }

    pub fn gap_replace_in_parent(&mut self, gap: AtomId, created: AtomId) {
        let parent = self.document.document_atom(gap).parent.clone().expect("gap without a parent");
        match self.document.document_atom(parent.atom).fields.get(&parent.field) {
            Some(Field::Array(_)) => {
                if self.syntax.syntax_type(self.document.document_atom(created).type_).is_pair {
                    self.pair_key_make_unique(parent.atom, &parent.field, created, &[gap]);
                }
                self.change_array(parent.atom, &parent.field, parent.index, 1, vec![created]);
            },
            Some(Field::Atom(_)) => {
                self.change_atom(parent.atom, &parent.field, created);
            },
            _ => unreachable!(),
        }
    }

    pub fn gap_select_text(&mut self, gap: AtomId) {
        self.primitive_select_field(gap, "gap");
    }

    fn gap_select_into_following(&mut self, created: AtomId, field: &str) {
        match self.document.document_atom(created).fields.get(field) {
            Some(Field::Primitive(_)) => self.primitive_select_field(created, field),
            Some(Field::Atom(child)) => {
                let child = *child;
                self.atom_select_into(child);
            },
            Some(Field::Array(_)) => {
                let element = self.array_insert_default(created, field, 0);
                self.atom_select_into(element);
            },
            None => panic!("following field `{}` missing", field),
        }
    }

    pub fn gap_type(&mut self, gap: AtomId, begin: usize, end: usize, text: &str) -> bool {
        let mut preview = self.document.document_primitive(gap, "gap").to_string();
        preview.replace_range(begin .. end, text);
        let (choices, longest, glyphs) = self.gap_choices_compute(gap, &preview);
        if glyphs > 0 {
            if longest == glyphs {
                if choices.len() == 1 && self.syntax.syntax_type(choices[0].type_).auto_choose_unambiguous {
                    let choice = choices[0].clone();
                    self.gap_choose(gap, choice, &preview);
                    return true;
                }
            } else if longest > 0 {
                if let Some(choice) = choices.first().cloned() {
                    self.gap_choose(gap, choice, &preview);
                    return true;
                }
            }
        }
        let done = self.edit_record(Some(EditUnique {
            atom: gap,
            field: "gap".to_string(),
            kind: "text",
        }), |ctx| {
            ctx.change_primitive(gap, "gap", begin, end - begin, text);
            return true;
        });
        self.gap_choices_sync();
        return done;
    }

    pub fn primitive_select_field(&mut self, atom: AtomId, field: &str) {
        let Some(visual) = self.atom_visual[atom] else {
            return;
        };
        let Some((_, v)) = self.visual_atom(visual).selectable.iter().find(|(f, _)| f == field).cloned() else {
            return;
        };
        let VisualKind::Primitive(p) = &self.visuals[v].kind else {
            return;
        };
        let len = p.value.len();
        self.primitive_select(v, true, len, len);
    }
}
