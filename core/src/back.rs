use crate::{
    cursor::Located,
    document::{
        AtomId,
        Document,
        Field,
    },
    reference::{
        Reference,
        Segment,
    },
    spec::SpecBack,
    syntax::Syntax,
};

pub fn back_atom_segments(syntax: &Syntax, document: &Document, atom: AtomId) -> Vec<Segment> {
    let Some(parent) = &document.document_atom(atom).parent else {
        return vec![];
    };
    let mut out = back_atom_segments(syntax, document, parent.atom);
    let (segments, element) = field_segments(syntax, document, parent.atom, &parent.field);
    out.extend(segments);
    match element {
        Element::Index(offset) | Element::Sub(offset, _) => out.push(Segment::Index(offset + parent.index)),
        Element::Optional(some_key) => out.push(Segment::Key(some_key)),
        Element::Record => out.push(Segment::Key(pair_key(syntax, document, atom))),
        Element::Value => { },
    }
    return out;
}

pub fn back_locate(syntax: &Syntax, document: &Document, reference: &Reference) -> Option<Target> {
    let start = match reference.id {
        Some(id) => document
            .atoms
            .iter()
            .position(|a| a.unique_id == Some(id))
            .or_else(|| document.atoms.iter().position(|a| a.back_ids.contains(&id)))?,
        None => document.root,
    };
    let (located, units) = walk_atom(syntax, document, start, &reference.path, reference.range.is_some())?;
    let range = match (reference.range, units) {
        (None, _) => None,
        (Some((begin, end)), Units::Elements(offset)) => {
            if begin < offset {
                return None;
            }
            Some((begin - offset, end - offset))
        },
        (Some(range), Units::Chars) => Some(range),
        (Some(_), Units::None) => return None,
    };
    return Some(Target {
        located: located,
        range: range,
    });
}

pub fn back_reference(
    syntax: &Syntax,
    document: &Document,
    located: &Located,
    range: Option<(usize, usize)>,
) -> Reference {
    let (atom, mut path, range) = match located {
        Located::Atom(a) => (*a, back_atom_segments(syntax, document, *a), None),
        Located::Field(a, field) => {
            let mut path = back_atom_segments(syntax, document, *a);
            let (segments, element) = field_segments(syntax, document, *a, field);
            path.extend(segments);
            let range = match element {
                Element::Index(offset) => range.map(|(b, e)| (offset + b, offset + e)),
                Element::Sub(offset, len) => Some(
                    range.map(|(b, e)| (offset + b, offset + e)).unwrap_or((offset, offset + len)),
                ),
                Element::Optional(_) | Element::Record => None,
                Element::Value => range,
            };
            (*a, path, range)
        },
    };
    let mut at = Some(atom);
    let mut id = None;
    while let Some(a) = at {
        let atom = document.document_atom(a);
        if let Some(found) = atom.unique_id {
            id = Some(found);
            let above = back_atom_segments(syntax, document, a).len();
            path.drain(..above);
            break;
        }
        at = atom.parent.as_ref().map(|p| p.atom);
    }
    return Reference {
        id: id,
        path: path,
        range: range,
    };
}

pub enum BackLocation {
    Atom(AtomId),
    Field(AtomId, String, usize),
}

pub fn back_locations(syntax: &Syntax, document: &Document, path: &[Segment]) -> Vec<BackLocation> {
    let mut out = vec![];
    locations_atom(syntax, document, document.root, path, &mut out);
    return out;
}

pub enum Element {
    Index(usize),
    Optional(String),
    Record,
    Sub(usize, usize),
    Value,
}

fn field_len(document: &Document, atom: AtomId, field: &str) -> usize {
    let Some(Field::Array(elements)) = document.document_atom(atom).fields.get(field) else {
        panic!("field `{}` is not an array", field);
    };
    return elements.len();
}

pub fn field_segments(syntax: &Syntax, document: &Document, atom: AtomId, field: &str) -> (Vec<Segment>, Element) {
    let mut prefix = vec![];
    let element =
        find_field(
            document,
            atom,
            &syntax.syntax_type(document.document_atom(atom).type_).back,
            field,
            &mut prefix,
        ).unwrap_or_else(|| panic!("field `{}` is not in its type's back", field));
    return (prefix, element);
}

fn find_field(
    document: &Document,
    atom: AtomId,
    spec: &SpecBack,
    field: &str,
    prefix: &mut Vec<Segment>,
) -> Option<Element> {
    match spec {
        SpecBack::FixedRecord(entries) => {
            for e in entries {
                let Some(value) = &e.value else {
                    continue;
                };
                prefix.push(Segment::Key(e.key.clone()));
                if let Some(found) = find_field(document, atom, value, field, prefix) {
                    return Some(found);
                }
                prefix.pop();
            }
            return None;
        },
        SpecBack::FixedArray(elems) => {
            let mut flat = vec![];
            flatten(elems, &mut flat);
            let mut at = 0;
            for e in flat {
                if let SpecBack::SubArray(f) = e {
                    let len = field_len(document, atom, &f.id);
                    if f.id == field {
                        return Some(Element::Sub(at, len));
                    }
                    at += len;
                    continue;
                }
                prefix.push(Segment::Index(at));
                if let Some(found) = find_field(document, atom, e, field, prefix) {
                    return Some(found);
                }
                prefix.pop();
                at += 1;
            }
            return None;
        },
        SpecBack::Pair(pair) => {
            if let SpecBack::String(f) = &*pair.key {
                if f.id == field {
                    return Some(Element::Value);
                }
            }
            return find_field(document, atom, &pair.value, field, prefix);
        },
        SpecBack::Optional(f) => {
            if f.id == field {
                return Some(Element::Optional(f.some_key.clone()));
            }
            return None;
        },
        SpecBack::Record(f) => {
            if f.id == field {
                return Some(Element::Record);
            }
            return None;
        },
        SpecBack::Array(f) => {
            if f.id == field {
                return Some(Element::Index(0));
            }
            return None;
        },
        SpecBack::Atom(f) => {
            if f.id == field {
                return Some(Element::Value);
            }
            return None;
        },
        SpecBack::String(f) | SpecBack::Number(f) | SpecBack::Literal(f) => {
            if f.id == field {
                return Some(Element::Value);
            }
            return None;
        },
        SpecBack::SubArray(_) |
        SpecBack::FixedSubArray(_) |
        SpecBack::Id(_) |
        SpecBack::FixedString(_) |
        SpecBack::FixedLiteral(_) => return None,
    }
}

fn flatten<'a>(elems: &'a [SpecBack], out: &mut Vec<&'a SpecBack>) {
    for e in elems {
        match e {
            SpecBack::FixedSubArray(inner) => flatten(inner, out),
            other => out.push(other),
        }
    }
}

fn locations_atom(
    syntax: &Syntax,
    document: &Document,
    atom: AtomId,
    segments: &[Segment],
    out: &mut Vec<BackLocation>,
) {
    if segments.is_empty() {
        out.push(BackLocation::Atom(atom));
    }
    locations_spec(
        syntax,
        document,
        atom,
        &syntax.syntax_type(document.document_atom(atom).type_).back,
        segments,
        out,
    );
}

fn locations_spec(
    syntax: &Syntax,
    document: &Document,
    atom: AtomId,
    spec: &SpecBack,
    segments: &[Segment],
    out: &mut Vec<BackLocation>,
) {
    let a = document.document_atom(atom);
    let child_of = |field: &str, index: usize| -> Option<AtomId> {
        let Some(Field::Array(elements)) = a.fields.get(field) else {
            return None;
        };
        return elements.get(index).copied();
    };
    match spec {
        SpecBack::FixedRecord(entries) => {
            let Some((Segment::Key(key), rest)) = segments.split_first() else {
                return;
            };
            let Some(value) = entries.iter().find(|e| e.key == *key).and_then(|e| e.value.as_ref()) else {
                return;
            };
            locations_spec(syntax, document, atom, value, rest, out);
        },
        SpecBack::FixedArray(elems) => {
            let mut flat = vec![];
            flatten(elems, &mut flat);
            let Some((segment, rest)) = segments.split_first() else {
                let mut at = 0;
                for e in flat {
                    if let SpecBack::SubArray(f) = e {
                        out.push(BackLocation::Field(atom, f.id.clone(), at));
                        return;
                    }
                    at += 1;
                }
                return;
            };
            let Segment::Index(index) = segment else {
                return;
            };
            let mut at = 0;
            for e in flat {
                if let SpecBack::SubArray(f) = e {
                    let len = field_len(document, atom, &f.id);
                    if *index < at + len {
                        if let Some(child) = child_of(&f.id, index - at) {
                            locations_atom(syntax, document, child, rest, out);
                        }
                        return;
                    }
                    at += len;
                    continue;
                }
                if *index == at {
                    locations_spec(syntax, document, atom, e, rest, out);
                    return;
                }
                at += 1;
            }
        },
        SpecBack::Pair(pair) => locations_spec(syntax, document, atom, &pair.value, segments, out),
        SpecBack::Optional(f) => match segments.split_first() {
            None => out.push(BackLocation::Field(atom, f.id.clone(), 0)),
            Some((Segment::Key(key), rest)) if *key == f.some_key => {
                if let Some(child) = child_of(&f.id, 0) {
                    locations_atom(syntax, document, child, rest, out);
                }
            },
            _ => { },
        },
        SpecBack::Record(f) => match segments.split_first() {
            None => out.push(BackLocation::Field(atom, f.id.clone(), 0)),
            Some((Segment::Key(key), rest)) => {
                let Some(Field::Array(elements)) = a.fields.get(&f.id) else {
                    return;
                };
                if let Some(child) = elements.iter().find(|c| pair_key(syntax, document, **c) == *key) {
                    locations_atom(syntax, document, *child, rest, out);
                }
            },
            Some((Segment::Index(_), _)) => { },
        },
        SpecBack::Array(f) => match segments.split_first() {
            None => out.push(BackLocation::Field(atom, f.id.clone(), 0)),
            Some((Segment::Index(index), rest)) => {
                if let Some(child) = child_of(&f.id, *index) {
                    locations_atom(syntax, document, child, rest, out);
                }
            },
            Some((Segment::Key(_), _)) => { },
        },
        SpecBack::Atom(f) => {
            if let Some(Field::Atom(child)) = a.fields.get(&f.id) {
                locations_atom(syntax, document, *child, segments, out);
            }
        },
        SpecBack::String(f) | SpecBack::Number(f) | SpecBack::Literal(f) => {
            if segments.is_empty() {
                out.push(BackLocation::Field(atom, f.id.clone(), 0));
            }
        },
        SpecBack::Id(_) |
        SpecBack::FixedString(_) |
        SpecBack::FixedLiteral(_) |
        SpecBack::SubArray(_) |
        SpecBack::FixedSubArray(_) => {

        },
    }
}

pub fn pair_key(syntax: &Syntax, document: &Document, atom: AtomId) -> String {
    let a = document.document_atom(atom);
    let SpecBack::Pair(pair) = &syntax.syntax_type(a.type_).back else {
        panic!("record element type is not a pair; syntax validation should have caught this");
    };
    match &*pair.key {
        SpecBack::FixedString(s) => return s.clone(),
        SpecBack::String(f) => {
            let Some(Field::Primitive(s)) = a.fields.get(&f.id) else {
                panic!("pair key field `{}` is not a primitive", f.id);
            };
            return s.clone();
        },
        _ => panic!("pair key is not a string; syntax validation should have caught this"),
    }
}

pub struct Target {
    pub located: Located,
    pub range: Option<(usize, usize)>,
}

enum Units {
    Chars,
    Elements(usize),
    None,
}

fn walk_atom(
    syntax: &Syntax,
    document: &Document,
    atom: AtomId,
    segments: &[Segment],
    range: bool,
) -> Option<(Located, Units)> {
    return walk_spec(
        syntax,
        document,
        atom,
        &syntax.syntax_type(document.document_atom(atom).type_).back,
        segments,
        range,
    );
}

fn walk_spec(
    syntax: &Syntax,
    document: &Document,
    atom: AtomId,
    spec: &SpecBack,
    segments: &[Segment],
    range: bool,
) -> Option<(Located, Units)> {
    let a = document.document_atom(atom);
    let child_of = |field: &str, index: usize| -> Option<AtomId> {
        let Some(Field::Array(elements)) = a.fields.get(field) else {
            return None;
        };
        return elements.get(index).copied();
    };
    match spec {
        SpecBack::FixedRecord(entries) => {
            let Some((Segment::Key(key), rest)) = segments.split_first() else {
                if segments.is_empty() {
                    return Some((Located::Atom(atom), Units::None));
                }
                return None;
            };
            let value = entries.iter().find(|e| e.key == *key)?.value.as_ref()?;
            return walk_spec(syntax, document, atom, value, rest, range);
        },
        SpecBack::FixedArray(elems) => {
            let mut flat = vec![];
            flatten(elems, &mut flat);
            let Some((Segment::Index(index), rest)) = segments.split_first() else {
                if !segments.is_empty() {
                    return None;
                }
                if range {
                    let mut at = 0;
                    for e in flat {
                        if let SpecBack::SubArray(f) = e {
                            return Some((Located::Field(atom, f.id.clone()), Units::Elements(at)));
                        }
                        at += 1;
                    }
                }
                return Some((Located::Atom(atom), Units::None));
            };
            let mut at = 0;
            for e in flat {
                if let SpecBack::SubArray(f) = e {
                    let len = field_len(document, atom, &f.id);
                    if *index < at + len {
                        return walk_atom(syntax, document, child_of(&f.id, index - at)?, rest, range);
                    }
                    at += len;
                    continue;
                }
                if *index == at {
                    return walk_spec(syntax, document, atom, e, rest, range);
                }
                at += 1;
            }
            return None;
        },
        SpecBack::Pair(pair) => return walk_spec(syntax, document, atom, &pair.value, segments, range),
        SpecBack::Optional(f) => match segments.split_first() {
            None => return Some((Located::Field(atom, f.id.clone()), Units::None)),
            Some((Segment::Key(key), rest)) if *key == f.some_key => {
                return walk_atom(syntax, document, child_of(&f.id, 0)?, rest, range);
            },
            Some((Segment::Key(key), rest)) if *key == f.none_key && rest.is_empty() => {
                return Some((Located::Field(atom, f.id.clone()), Units::None));
            },
            _ => return None,
        },
        SpecBack::Record(f) => {
            let Some((Segment::Key(key), rest)) = segments.split_first() else {
                if segments.is_empty() {
                    return Some((Located::Field(atom, f.id.clone()), Units::None));
                }
                return None;
            };
            let Some(Field::Array(elements)) = a.fields.get(&f.id) else {
                return None;
            };
            let child = *elements.iter().find(|c| pair_key(syntax, document, **c) == *key)?;
            return walk_atom(syntax, document, child, rest, range);
        },
        SpecBack::Array(f) => {
            let Some((Segment::Index(index), rest)) = segments.split_first() else {
                if segments.is_empty() {
                    return Some((Located::Field(atom, f.id.clone()), Units::Elements(0)));
                }
                return None;
            };
            return walk_atom(syntax, document, child_of(&f.id, *index)?, rest, range);
        },
        SpecBack::Atom(f) => {
            let Some(Field::Atom(child)) = a.fields.get(&f.id) else {
                return None;
            };
            return walk_atom(syntax, document, *child, segments, range);
        },
        SpecBack::String(f) | SpecBack::Number(f) | SpecBack::Literal(f) => {
            if !segments.is_empty() {
                return None;
            }
            return Some((Located::Field(atom, f.id.clone()), Units::Chars));
        },
        SpecBack::Id(_) | SpecBack::FixedString(_) | SpecBack::FixedLiteral(_) => {
            if !segments.is_empty() {
                return None;
            }
            return Some((Located::Atom(atom), Units::None));
        },
        SpecBack::SubArray(_) | SpecBack::FixedSubArray(_) => return None,
    }
}
