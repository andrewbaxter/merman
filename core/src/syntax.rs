//! Validated syntax: the spec with names resolved to indices and all cross
//! references checked.
use crate::direction::DirectionConvert;
use crate::error::{
    ErrorKind,
    MultiError,
};
use crate::display::display_unit_to_pixels;
use crate::measure::FontSpec;
use crate::spec::{
    SpecAlignment,
    SpecBack,
    SpecCondition,
    SpecDirection,
    SpecDisplayUnit,
    SpecFront,
    SpecFrontArray,
    SpecFrontArrayAsAtom,
    SpecFrontAtom,
    SpecFrontPrimitive,
    SpecObbox,
    SpecPadding,
    SpecSplit,
    SpecSymbol,
    SpecSyntax,
    SpecType,
    SpecTypeRoot,
};
use std::collections::{
    HashMap,
    HashSet,
};
use std::rc::Rc;

pub type TypeId = usize;
pub type StyleId = usize;

/// Type index of the document root.
pub const TYPE_ROOT: TypeId = 0;

pub struct Syntax {
    pub spec_root: SpecSyntaxSettings,
    /// Index 0 is the root type.
    pub types: Vec<TypeDef>,
    /// Type and group ids to the concrete types they cover, in match order.
    pub groups: HashMap<String, Vec<TypeId>>,
    /// Index 0 is the unstyled default.
    pub styles: Vec<Style>,
}

/// Global settings copied out of the spec, lengths converted to px.
pub struct SpecSyntaxSettings {
    pub background: String,
    pub display_unit: SpecDisplayUnit,
    pub to_pixels: f64,
    pub convert: DirectionConvert,
    pub pad: SpecPadding,
    pub course_transverse_stride: f64,
    pub unprintable: String,
    pub cursor: SpecObbox,
    pub hover: SpecObbox,
}

fn scale_obbox(o: &SpecObbox, to_pixels: f64) -> SpecObbox {
    return SpecObbox {
        padding: scale_padding(&o.padding, to_pixels),
        round_radius: o.round_radius * to_pixels,
        line_thickness: o.line_thickness * to_pixels,
        ..o.clone()
    };
}

/// Lengths in px.
#[derive(Clone)]
pub struct Style {
    pub font: FontSpec,
    pub color: String,
    pub padding: SpecPadding,
    pub ascent: Option<f64>,
    pub descent: Option<f64>,
}

/// Font px for a size in display units: the size converted to points, used as px
/// (as merman's displays do).
fn font_pixels(unit: SpecDisplayUnit, size: f64) -> f64 {
    let mm_per_unit = display_unit_to_pixels(unit) / display_unit_to_pixels(SpecDisplayUnit::Mm);
    return size * mm_per_unit * 72. / 25.4;
}

fn scale_padding(p: &SpecPadding, to_pixels: f64) -> SpecPadding {
    return SpecPadding {
        converse_start: p.converse_start * to_pixels,
        converse_end: p.converse_end * to_pixels,
        transverse_start: p.transverse_start * to_pixels,
        transverse_end: p.transverse_end * to_pixels,
    };
}

pub struct TypeDef {
    pub id: String,
    pub name: String,
    pub precedence: i64,
    pub associate_forward: bool,
    pub depth_score: i64,
    /// In declaration order.
    pub alignments: Vec<(String, SpecAlignment)>,
    pub back: SpecBack,
    pub front: Vec<Front>,
    pub fields: HashMap<String, FieldKind>,
    /// True if `back` is a `pair`, i.e. this type only appears as a record entry.
    pub is_pair: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Primitive,
    Atom,
    Array,
}

pub enum Front {
    Symbol(Symbol),
    Primitive(FrontPrimitive),
    Atom(FrontAtom),
    Array(FrontArray),
}

pub struct FrontPrimitive {
    pub field: String,
    pub style: StyleId,
    pub split: SpecSplit,
    pub first_alignment: Option<String>,
    pub first_split_alignment: Option<String>,
    pub hard_split_alignment: Option<String>,
    pub soft_split_alignment: Option<String>,
}

pub struct FrontAtom {
    pub ellipsis: Symbol,
    pub field: String,
    pub from_array: bool,
    pub forward_alignments: Rc<HashSet<String>>,
}

pub struct FrontArray {
    pub field: String,
    pub prefix: Vec<Symbol>,
    pub suffix: Vec<Symbol>,
    pub separator: Vec<Symbol>,
    pub empty: Option<Symbol>,
    pub forward_alignments: Rc<HashSet<String>>,
}

pub struct Symbol {
    pub kind: SymbolKind,
    pub split: SpecSplit,
    pub alignment: Option<String>,
    pub split_alignment: Option<String>,
    pub condition: Option<SpecCondition>,
}

pub enum SymbolKind {
    Text {
        text: String,
        style: StyleId,
    },
    Space {
        width: f64,
        ascent: f64,
        descent: f64,
    },
}

impl Symbol {
    /// Whether this symbol visibly separates its neighbors; used for the `precedent`
    /// condition.
    pub fn symbol_delimits(&self) -> bool {
        match &self.kind {
            SymbolKind::Space { .. } => return false,
            SymbolKind::Text { text, .. } => {
                if text.trim().is_empty() {
                    return false;
                }
            },
        }
        if self.condition.is_some() {
            return false;
        }
        return true;
    }
}

type Errors = MultiError;

struct StyleTable {
    ids: HashMap<String, StyleId>,
}

impl StyleTable {
    fn lookup(&self, errors: &mut Errors, path: &str, name: &Option<String>) -> StyleId {
        match name {
            None => return 0,
            Some(n) => match self.ids.get(n) {
                Some(i) => return *i,
                None => {
                    errors.multi_error_add(path, ErrorKind::UnknownStyle { style: n.clone() });
                    return 0;
                },
            },
        }
    }
}

impl Syntax {
    pub fn syntax_resolve(spec: SpecSyntax) -> Result<Syntax, MultiError> {
        let mut errors = Errors::default();
        let to_pixels = display_unit_to_pixels(spec.display_unit);

        // Directions
        let vertical = |d: SpecDirection| matches!(d, SpecDirection::Up | SpecDirection::Down);
        if vertical(spec.converse_direction) == vertical(spec.transverse_direction) {
            errors.multi_error_add("transverse_direction", ErrorKind::NotTransverse {
                converse: spec.converse_direction,
                transverse: spec.transverse_direction,
            });
        }

        // Styles
        let mut styles = vec![Style {
            font: FontSpec {
                family: spec.font_family.clone(),
                size: font_pixels(spec.display_unit, spec.font_size),
            },
            color: spec.foreground.clone(),
            padding: SpecPadding::default(),
            ascent: None,
            descent: None,
        }];
        let mut style_ids = HashMap::new();
        for (name, s) in &spec.styles {
            style_ids.insert(name.clone(), styles.len());
            styles.push(Style {
                font: FontSpec {
                    family: s.font_family.clone().unwrap_or_else(|| spec.font_family.clone()),
                    size: font_pixels(spec.display_unit, s.font_size.unwrap_or(spec.font_size)),
                },
                color: s.color.clone(),
                padding: scale_padding(&s.padding, to_pixels),
                ascent: s.ascent.map(|a| a * to_pixels),
                descent: s.descent.map(|d| d * to_pixels),
            });
        }
        let style_table = StyleTable { ids: style_ids };

        // Types
        let mut types = vec![];
        let mut type_ids: HashMap<String, TypeId> = HashMap::new();
        {
            let SpecTypeRoot { back, front, alignments } = spec.root;
            types.push(build_type(&mut errors, &style_table, "root", SpecType {
                id: "__root".to_string(),
                name: Some("root".to_string()),
                precedence: i64::MIN,
                associate_forward: false,
                depth_score: 0,
                alignments: alignments,
                back: back,
                front: front,
            }));
        }
        for (i, t) in spec.types.into_iter().enumerate() {
            let path = format!("types[{}]({})", i, t.id);
            if t.id.starts_with("__") {
                errors.multi_error_add(&path, ErrorKind::ReservedAtomTypeId { atom_type: t.id.clone() });
            }
            if type_ids.insert(t.id.clone(), types.len()).is_some() {
                errors.multi_error_add(&path, ErrorKind::DuplicateAtomTypeIds { atom_type: t.id.clone() });
            }
            types.push(build_type(&mut errors, &style_table, &path, t));
        }

        // Groups
        let mut groups: HashMap<String, Vec<TypeId>> = HashMap::new();
        for (id, t) in &type_ids {
            groups.insert(id.clone(), vec![*t]);
        }
        {
            let spec_groups: HashMap<&String, &Vec<String>> =
                spec.groups.iter().map(|g| (&g.id, &g.members)).collect();
            for (i, g) in spec.groups.iter().enumerate() {
                let path = format!("groups[{}]({})", i, g.id);
                if type_ids.contains_key(&g.id) {
                    errors.multi_error_add(&path, ErrorKind::DuplicateAtomTypeIdsInGroup { group: g.id.clone() });
                    continue;
                }
                if groups.contains_key(&g.id) {
                    errors.multi_error_add(&path, ErrorKind::DuplicateGroupId { group: g.id.clone() });
                    continue;
                }
                let mut out = vec![];
                let mut seen = HashSet::new();
                let mut stack: Vec<String> = vec![g.id.clone()];

                fn walk(
                    errors: &mut Errors,
                    path: &str,
                    spec_groups: &HashMap<&String, &Vec<String>>,
                    type_ids: &HashMap<String, TypeId>,
                    stack: &mut Vec<String>,
                    seen: &mut HashSet<TypeId>,
                    out: &mut Vec<TypeId>,
                    member: &str,
                ) {
                    if let Some(t) = type_ids.get(member) {
                        if seen.insert(*t) {
                            out.push(*t);
                        }
                        return;
                    }
                    if let Some(members) = spec_groups.get(&member.to_string()) {
                        if stack.iter().any(|s| s == member) {
                            errors.multi_error_add(path, ErrorKind::TypeCircularReference {
                                stack: stack.clone(),
                                member: member.to_string(),
                            });
                            return;
                        }
                        stack.push(member.to_string());
                        for m in members.iter() {
                            walk(errors, path, spec_groups, type_ids, stack, seen, out, m);
                        }
                        stack.pop();
                        return;
                    }
                    errors.multi_error_add(path, ErrorKind::GroupChildDoesntExist { member: member.to_string() });
                }

                for m in &g.members {
                    walk(&mut errors, &path, &spec_groups, &type_ids, &mut stack, &mut seen, &mut out, m);
                }
                if out.is_empty() {
                    errors.multi_error_add(&path, ErrorKind::EmptyGroup { group: g.id.clone() });
                }
                groups.insert(g.id.clone(), out);
            }
        }

        // Cross references between types
        for (ti, t) in types.iter().enumerate() {
            let path = if ti == TYPE_ROOT {
                "root".to_string()
            } else {
                format!("types[{}]({})", ti - 1, t.id)
            };
            if ti == TYPE_ROOT && t.is_pair {
                errors.multi_error_add(&path, ErrorKind::RootBackIsKey);
            }
            check_back_refs(&mut errors, &format!("{}.back", path), &groups, &types, &t.back);
        }
        if !errors.multi_error_is_empty() {
            return Err(errors);
        }
        return Ok(Syntax {
            spec_root: SpecSyntaxSettings {
                background: spec.background,
                display_unit: spec.display_unit,
                to_pixels: to_pixels,
                convert: DirectionConvert::new(spec.converse_direction, spec.transverse_direction),
                pad: scale_padding(&spec.pad, to_pixels),
                course_transverse_stride: spec.course_transverse_stride * to_pixels,
                unprintable: spec.unprintable,
                cursor: scale_obbox(&spec.cursor, to_pixels),
                hover: scale_obbox(&spec.hover, to_pixels),
            },
            types: types,
            groups: groups,
            styles: styles,
        });
    }

    pub fn syntax_type(&self, id: TypeId) -> &TypeDef {
        return &self.types[id];
    }

    pub fn syntax_style(&self, id: StyleId) -> &Style {
        return &self.styles[id];
    }
}

fn check_back_refs(
    errors: &mut Errors,
    path: &str,
    groups: &HashMap<String, Vec<TypeId>>,
    types: &[TypeDef],
    back: &SpecBack,
) {
    let check_group = |errors: &mut Errors, path: &str, name: &str, want_pair: bool| {
        let Some(members) = groups.get(name) else {
            errors.multi_error_add(path, ErrorKind::AtomTypeDoesntExist { candidate_type: name.to_string() });
            return;
        };
        for m in members {
            if types[*m].is_pair != want_pair {
                if want_pair {
                    errors.multi_error_add(
                        path,
                        ErrorKind::NonKeyInvalidAtLocation { atom_type: types[*m].id.clone() },
                    );
                } else {
                    errors.multi_error_add(
                        path,
                        ErrorKind::KeyInvalidForGroupMember { atom_type: types[*m].id.clone() },
                    );
                }
            }
        }
    };
    match back {
        SpecBack::FixedString(_) |
        SpecBack::FixedLiteral(_) |
        SpecBack::String(_) |
        SpecBack::Number(_) |
        SpecBack::Literal(_) => {

        },
        SpecBack::Atom(a) => check_group(errors, path, &a.type_, false),
        SpecBack::Array(a) => check_group(errors, path, &a.element, false),
        SpecBack::Optional(a) => check_group(errors, path, &a.element, false),
        SpecBack::Record(a) => check_group(errors, path, &a.element, true),
        SpecBack::Pair(p) => {
            check_back_refs(errors, &format!("{}.key", path), groups, types, &p.key);
            check_back_refs(errors, &format!("{}.value", path), groups, types, &p.value);
        },
        SpecBack::FixedArray(elems) | SpecBack::FixedSubArray(elems) => {
            for (i, e) in elems.iter().enumerate() {
                check_back_refs(errors, &format!("{}[{}]", path, i), groups, types, e);
            }
        },
        SpecBack::SubArray(a) => check_group(errors, path, &a.element, false),
        SpecBack::Id(_) => { },
        SpecBack::FixedRecord(entries) => {
            for e in entries {
                if let Some(v) = &e.value {
                    check_back_refs(errors, &format!("{}.{}", path, e.key), groups, types, v);
                }
            }
        },
    }
}

pub fn back_sub_array_slots(elems: &[SpecBack]) -> (usize, Option<usize>) {
    let mut fixed = 0;
    let mut variable = None;
    for e in elems {
        match e {
            SpecBack::SubArray(_) => {
                if variable.is_none() {
                    variable = Some(fixed);
                }
            },
            SpecBack::FixedSubArray(inner) => {
                let (inner_fixed, inner_variable) = back_sub_array_slots(inner);
                if let Some(at) = inner_variable {
                    if variable.is_none() {
                        variable = Some(fixed + at);
                    }
                }
                fixed += inner_fixed;
            },
            _ => fixed += 1,
        }
    }
    return (fixed, variable);
}

fn count_variable_runs(elems: &[SpecBack]) -> usize {
    let mut out = 0;
    for e in elems {
        match e {
            SpecBack::SubArray(_) => out += 1,
            SpecBack::FixedSubArray(inner) => out += count_variable_runs(inner),
            _ => { },
        }
    }
    return out;
}

fn collect_fields(
    errors: &mut Errors,
    path: &str,
    back: &SpecBack,
    top: bool,
    fields: &mut HashMap<String, FieldKind>,
) {
    let mut put = |errors: &mut Errors, id: &str, kind: FieldKind| {
        if fields.insert(id.to_string(), kind).is_some() {
            errors.multi_error_add(path, ErrorKind::DuplicateBackId { id: id.to_string() });
        }
    };
    match back {
        SpecBack::FixedString(_) | SpecBack::FixedLiteral(_) => { },
        SpecBack::String(f) | SpecBack::Number(f) | SpecBack::Literal(f) => {
            put(errors, &f.id, FieldKind::Primitive)
        },
        SpecBack::Atom(a) => put(errors, &a.id, FieldKind::Atom),
        SpecBack::Array(a) | SpecBack::Record(a) | SpecBack::SubArray(a) => {
            put(errors, &a.id, FieldKind::Array)
        },
        SpecBack::Id(_) => { },
        SpecBack::Optional(a) => put(errors, &a.id, FieldKind::Array),
        SpecBack::Pair(p) => {
            if !top {
                errors.multi_error_add(path, ErrorKind::KeyInvalidAtLocation);
            }
            match &*p.key {
                SpecBack::FixedString(_) | SpecBack::String(_) => { },
                _ => errors.multi_error_add(path, ErrorKind::BackFieldWrongType {
                    field: "key".to_string(),
                    found: "other".to_string(),
                    expected: "fixed_string or string".to_string(),
                }),
            }
            collect_fields(errors, &format!("{}.key", path), &p.key, false, fields);
            collect_fields(errors, &format!("{}.value", path), &p.value, false, fields);
        },
        SpecBack::FixedArray(elems) | SpecBack::FixedSubArray(elems) => {
            if let SpecBack::FixedArray(_) = back {
                if count_variable_runs(elems) > 1 {
                    errors.multi_error_add(path, ErrorKind::ArrayMultipleAtoms);
                }
            }
            for (i, e) in elems.iter().enumerate() {
                match e {
                    SpecBack::SubArray(_) | SpecBack::FixedSubArray(_) => { },
                    _ => { },
                }
                collect_fields(errors, &format!("{}[{}]", path, i), e, false, fields);
            }
        },
        SpecBack::FixedRecord(entries) => {
            let mut keys = HashSet::new();
            for e in entries {
                if !keys.insert(&e.key) {
                    errors.multi_error_add(path, ErrorKind::RecordDiscardDuplicateKey { key: e.key.clone() });
                }
                if let Some(v) = &e.value {
                    collect_fields(errors, &format!("{}.{}", path, e.key), v, false, fields);
                }
            }
        },
    }
}

fn build_symbol(
    errors: &mut Errors,
    styles: &StyleTable,
    path: &str,
    fields: &HashMap<String, FieldKind>,
    s: &SpecSymbol,
) -> Symbol {
    let (kind, split, alignment, split_alignment, condition) = match s {
        SpecSymbol::Text(t) => (SymbolKind::Text {
            text: t.text.clone(),
            style: styles.lookup(errors, path, &t.style),
        }, t.split, t.alignment.clone(), t.split_alignment.clone(), t.condition.clone()),
        SpecSymbol::Space(t) => (SymbolKind::Space {
            width: t.width,
            ascent: t.ascent,
            descent: t.descent,
        }, t.split, t.alignment.clone(), t.split_alignment.clone(), t.condition.clone()),
    };
    if let Some(SpecCondition::Empty(c)) = &condition {
        match fields.get(&c.field) {
            Some(FieldKind::Primitive) | Some(FieldKind::Array) => { },
            Some(FieldKind::Atom) => errors.multi_error_add(path, ErrorKind::BackFieldWrongType {
                field: c.field.clone(),
                found: "Atom".to_string(),
                expected: "Primitive or Array".to_string(),
            }),
            None => errors.multi_error_add(path, ErrorKind::MissingBack { field: c.field.clone() }),
        }
    }
    return Symbol {
        kind: kind,
        split: split,
        alignment: alignment,
        split_alignment: split_alignment,
        condition: condition,
    };
}

fn build_type(errors: &mut Errors, styles: &StyleTable, path: &str, t: SpecType) -> TypeDef {
    let mut fields = HashMap::new();
    collect_fields(errors, &format!("{}.back", path), &t.back, true, &mut fields);
    let is_pair = matches!(t.back, SpecBack::Pair(_));
    let mut used = HashSet::new();
    let mut check_field = |errors: &mut Errors, path: &str, field: &str, want: FieldKind| {
        used.insert(field.to_string());
        match fields.get(field) {
            Some(k) if *k == want => { },
            Some(k) => errors.multi_error_add(path, ErrorKind::BackFieldWrongType {
                field: field.to_string(),
                found: format!("{:?}", k),
                expected: format!("{:?}", want),
            }),
            None => errors.multi_error_add(path, ErrorKind::MissingBack { field: field.to_string() }),
        }
    };
    let mut front = vec![];
    for (i, f) in t.front.iter().enumerate() {
        let fpath = format!("{}.front[{}]", path, i);
        match f {
            SpecFront::Symbol(s) => {
                front.push(Front::Symbol(build_symbol(errors, styles, &fpath, &fields, s)))
            },
            SpecFront::Primitive(
                SpecFrontPrimitive {
                    field,
                    style,
                    split,
                    first_alignment,
                    first_split_alignment,
                    hard_split_alignment,
                    soft_split_alignment,
                },
            ) => {
                check_field(errors, &fpath, field, FieldKind::Primitive);
                front.push(Front::Primitive(FrontPrimitive {
                    field: field.clone(),
                    style: styles.lookup(errors, &fpath, style),
                    split: *split,
                    first_alignment: first_alignment.clone(),
                    first_split_alignment: first_split_alignment.clone(),
                    hard_split_alignment: hard_split_alignment.clone(),
                    soft_split_alignment: soft_split_alignment.clone(),
                }));
            },
            SpecFront::Atom(SpecFrontAtom { field, ellipsis, forward_alignments }) => {
                check_field(errors, &fpath, field, FieldKind::Atom);
                front.push(Front::Atom(FrontAtom {
                    ellipsis: build_symbol(errors, styles, &format!("{}.ellipsis", fpath), &fields, ellipsis),
                    field: field.clone(),
                    from_array: false,
                    forward_alignments: Rc::new(forward_alignments.iter().cloned().collect()),
                }));
            },
            SpecFront::ArrayAsAtom(SpecFrontArrayAsAtom { field, ellipsis, forward_alignments }) => {
                check_field(errors, &fpath, field, FieldKind::Array);
                front.push(Front::Atom(FrontAtom {
                    ellipsis: build_symbol(errors, styles, &format!("{}.ellipsis", fpath), &fields, ellipsis),
                    field: field.clone(),
                    from_array: true,
                    forward_alignments: Rc::new(forward_alignments.iter().cloned().collect()),
                }));
            },
            SpecFront::Array(SpecFrontArray { field, prefix, suffix, separator, empty, forward_alignments }) => {
                check_field(errors, &fpath, field, FieldKind::Array);
                let build = |errors: &mut Errors, name: &str, syms: &Vec<SpecSymbol>| {
                    return syms.iter().enumerate().map(|(j, s)| {
                        return build_symbol(errors, styles, &format!("{}.{}[{}]", fpath, name, j), &fields, s);
                    }).collect::<Vec<_>>();
                };
                front.push(Front::Array(FrontArray {
                    field: field.clone(),
                    prefix: build(errors, "prefix", prefix),
                    suffix: build(errors, "suffix", suffix),
                    separator: build(errors, "separator", separator),
                    empty: empty
                        .as_ref()
                        .map(|s| build_symbol(errors, styles, &format!("{}.empty", fpath), &fields, s)),
                    forward_alignments: Rc::new(forward_alignments.iter().cloned().collect()),
                }));
            },
        }
    }
    for (id, _) in &fields {
        if !used.contains(id) {
            errors.multi_error_add(path, ErrorKind::UnusedBackData { unused: id.clone() });
        }
    }
    for (name, a) in &t.alignments {
        if let SpecAlignment::Relative(r) = a {
            if r.base.is_empty() {
                errors.multi_error_add(format!("{}.alignments.{}", path, name), ErrorKind::EmptyAlignmentBase);
            }
        }
    }
    return TypeDef {
        name: t.name.clone().unwrap_or_else(|| t.id.clone()),
        id: t.id,
        precedence: t.precedence,
        associate_forward: t.associate_forward,
        depth_score: t.depth_score,
        alignments: t.alignments.into_iter().collect(),
        back: t.back,
        front: front,
        fields: fields,
        is_pair: is_pair,
    };
}
