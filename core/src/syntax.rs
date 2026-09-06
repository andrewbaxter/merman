//! Validated syntax: the spec with names resolved to indices and all
//! cross references checked.
use crate::direction::DirectionConvert;
use crate::measure::FontSpec;
use crate::spec::{
    SpecAlignment, SpecBack, SpecCondition, SpecDirection, SpecDisplayUnit, SpecFront,
    SpecFrontArray, SpecFrontAtom, SpecFrontPrimitive, SpecPadding, SpecSplit, SpecSymbol,
    SpecSyntax, SpecType, SpecTypeRoot,
};
use std::collections::{HashMap, HashSet};
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
    /// Multiplier from display units to px.
    pub to_pixels: f64,
    pub convert: DirectionConvert,
    pub pad: SpecPadding,
    pub course_transverse_stride: f64,
    pub unprintable: String,
}

/// Lengths in px.
pub struct Style {
    pub font: FontSpec,
    pub color: String,
    pub padding: SpecPadding,
    pub ascent: Option<f64>,
    pub descent: Option<f64>,
}

/// CSS px per display unit.
pub fn syntax_to_pixels(unit: SpecDisplayUnit) -> f64 {
    match unit {
        SpecDisplayUnit::Px => return 1.,
        SpecDisplayUnit::Mm => return 96. / 25.4,
    }
}

/// Font px for a size in display units: the size converted to points, used as
/// px (as merman's displays do).
fn font_pixels(unit: SpecDisplayUnit, size: f64) -> f64 {
    let mm_per_unit = syntax_to_pixels(unit) / syntax_to_pixels(SpecDisplayUnit::Mm);
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
    pub field: String,
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
    Text { text: String, style: StyleId },
    Space { width: f64, ascent: f64, descent: f64 },
}

impl Symbol {
    /// Whether this symbol visibly separates its neighbors; used for the
    /// `precedent` condition.
    pub fn symbol_delimits(&self) -> bool {
        match &self.kind {
            SymbolKind::Space { .. } => return false,
            SymbolKind::Text { text, .. } => {
                if text.trim().is_empty() {
                    return false;
                }
            }
        }
        if self.condition.is_some() {
            return false;
        }
        return true;
    }
}

struct Errors(Vec<String>);

impl Errors {
    fn add(&mut self, path: &str, message: impl AsRef<str>) {
        self.0.push(format!("{}: {}", path, message.as_ref()));
    }
}

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
                    errors.add(path, format!("unknown style `{}`", n));
                    return 0;
                }
            },
        }
    }
}

impl Syntax {
    pub fn syntax_resolve(spec: SpecSyntax) -> Result<Syntax, Vec<String>> {
        let mut errors = Errors(vec![]);
        let to_pixels = syntax_to_pixels(spec.display_unit);

        // Directions
        let vertical = |d: SpecDirection| matches!(d, SpecDirection::Up | SpecDirection::Down);
        if vertical(spec.converse_direction) == vertical(spec.transverse_direction) {
            errors.add(
                "transverse_direction",
                format!(
                    "{:?} is not perpendicular to converse direction {:?}",
                    spec.transverse_direction, spec.converse_direction
                ),
            );
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
            let SpecTypeRoot {
                back,
                front,
                alignments,
            } = spec.root;
            types.push(build_type(
                &mut errors,
                &style_table,
                "root",
                SpecType {
                    id: "__root".to_string(),
                    name: Some("root".to_string()),
                    precedence: i64::MIN,
                    associate_forward: false,
                    depth_score: 0,
                    alignments,
                    back,
                    front,
                },
            ));
        }
        for (i, t) in spec.types.into_iter().enumerate() {
            let path = format!("types[{}]({})", i, t.id);
            if t.id.starts_with("__") {
                errors.add(&path, "type ids starting with `__` are reserved");
            }
            if type_ids.insert(t.id.clone(), types.len()).is_some() {
                errors.add(&path, "duplicate type id");
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
                    errors.add(&path, "group id collides with a type id");
                    continue;
                }
                if groups.contains_key(&g.id) {
                    errors.add(&path, "duplicate group id");
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
                            errors.add(
                                path,
                                format!("group reference cycle: {} -> {}", stack.join(" -> "), member),
                            );
                            return;
                        }
                        stack.push(member.to_string());
                        for m in members.iter() {
                            walk(errors, path, spec_groups, type_ids, stack, seen, out, m);
                        }
                        stack.pop();
                        return;
                    }
                    errors.add(path, format!("member `{}` is neither a type nor a group", member));
                }
                for m in &g.members {
                    walk(
                        &mut errors,
                        &path,
                        &spec_groups,
                        &type_ids,
                        &mut stack,
                        &mut seen,
                        &mut out,
                        m,
                    );
                }
                if out.is_empty() {
                    errors.add(&path, "group covers no types");
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
                errors.add(&path, "the root back can't be a `pair`");
            }
            check_back_refs(&mut errors, &format!("{}.back", path), &groups, &types, &t.back);
        }

        if !errors.0.is_empty() {
            return Err(errors.0);
        }
        return Ok(Syntax {
            spec_root: SpecSyntaxSettings {
                background: spec.background,
                to_pixels,
                convert: DirectionConvert::new(spec.converse_direction, spec.transverse_direction),
                pad: scale_padding(&spec.pad, to_pixels),
                course_transverse_stride: spec.course_transverse_stride * to_pixels,
                unprintable: spec.unprintable,
            },
            types,
            groups,
            styles,
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
            errors.add(path, format!("unknown type or group `{}`", name));
            return;
        };
        for m in members {
            if types[*m].is_pair != want_pair {
                if want_pair {
                    errors.add(
                        path,
                        format!(
                            "record element type `{}` must have a `pair` back",
                            types[*m].id
                        ),
                    );
                } else {
                    errors.add(
                        path,
                        format!(
                            "type `{}` has a `pair` back and can only be used as a record element",
                            types[*m].id
                        ),
                    );
                }
            }
        }
    };
    match back {
        SpecBack::FixedString(_)
        | SpecBack::FixedLiteral(_)
        | SpecBack::String(_)
        | SpecBack::Number(_)
        | SpecBack::Literal(_)
        | SpecBack::Discard(_) => {}
        SpecBack::Atom(a) => check_group(errors, path, &a.type_, false),
        SpecBack::Array(a) => check_group(errors, path, &a.element, false),
        SpecBack::Optional(a) => check_group(errors, path, &a.element, false),
        SpecBack::Record(a) => check_group(errors, path, &a.element, true),
        SpecBack::Pair(p) => {
            check_back_refs(errors, &format!("{}.key", path), groups, types, &p.key);
            check_back_refs(errors, &format!("{}.value", path), groups, types, &p.value);
        }
        SpecBack::FixedArray(elems) => {
            for (i, e) in elems.iter().enumerate() {
                check_back_refs(errors, &format!("{}[{}]", path, i), groups, types, e);
            }
        }
        SpecBack::FixedRecord(entries) => {
            for e in entries {
                check_back_refs(errors, &format!("{}.{}", path, e.key), groups, types, &e.value);
            }
        }
    }
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
            errors.add(path, format!("duplicate field id `{}`", id));
        }
    };
    match back {
        SpecBack::FixedString(_) | SpecBack::FixedLiteral(_) | SpecBack::Discard(_) => {}
        SpecBack::String(f) | SpecBack::Number(f) | SpecBack::Literal(f) => {
            put(errors, &f.id, FieldKind::Primitive)
        }
        SpecBack::Atom(a) => put(errors, &a.id, FieldKind::Atom),
        SpecBack::Array(a) | SpecBack::Record(a) => put(errors, &a.id, FieldKind::Array),
        SpecBack::Optional(a) => put(errors, &a.id, FieldKind::Array),
        SpecBack::Pair(p) => {
            if !top {
                errors.add(path, "`pair` is only valid as the whole back of a type");
            }
            match &*p.key {
                SpecBack::FixedString(_) | SpecBack::String(_) => {}
                _ => errors.add(path, "a pair key must be `fixed_string` or `string`"),
            }
            collect_fields(errors, &format!("{}.key", path), &p.key, false, fields);
            collect_fields(errors, &format!("{}.value", path), &p.value, false, fields);
        }
        SpecBack::FixedArray(elems) => {
            for (i, e) in elems.iter().enumerate() {
                collect_fields(errors, &format!("{}[{}]", path, i), e, false, fields);
            }
        }
        SpecBack::FixedRecord(entries) => {
            let mut keys = HashSet::new();
            for e in entries {
                if !keys.insert(&e.key) {
                    errors.add(path, format!("duplicate record key `{}`", e.key));
                }
                collect_fields(errors, &format!("{}.{}", path, e.key), &e.value, false, fields);
            }
        }
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
        SpecSymbol::Text(t) => (
            SymbolKind::Text {
                text: t.text.clone(),
                style: styles.lookup(errors, path, &t.style),
            },
            t.split,
            t.alignment.clone(),
            t.split_alignment.clone(),
            t.condition.clone(),
        ),
        SpecSymbol::Space(t) => (
            SymbolKind::Space {
                width: t.width,
                ascent: t.ascent,
                descent: t.descent,
            },
            t.split,
            t.alignment.clone(),
            t.split_alignment.clone(),
            t.condition.clone(),
        ),
    };
    if let Some(SpecCondition::Empty(c)) = &condition {
        match fields.get(&c.field) {
            Some(FieldKind::Primitive) | Some(FieldKind::Array) => {}
            Some(FieldKind::Atom) => errors.add(
                path,
                format!("condition field `{}` must be a primitive or array", c.field),
            ),
            None => errors.add(path, format!("condition refers to unknown field `{}`", c.field)),
        }
    }
    return Symbol {
        kind,
        split,
        alignment,
        split_alignment,
        condition,
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
            Some(k) if *k == want => {}
            Some(k) => errors.add(
                path,
                format!("field `{}` is {:?} but the front expects {:?}", field, k, want),
            ),
            None => errors.add(path, format!("unknown field `{}`", field)),
        }
    };
    let mut front = vec![];
    for (i, f) in t.front.iter().enumerate() {
        let fpath = format!("{}.front[{}]", path, i);
        match f {
            SpecFront::Symbol(s) => {
                front.push(Front::Symbol(build_symbol(errors, styles, &fpath, &fields, s)))
            }
            SpecFront::Primitive(SpecFrontPrimitive {
                field,
                style,
                split,
                first_alignment,
                first_split_alignment,
                hard_split_alignment,
                soft_split_alignment,
            }) => {
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
            }
            SpecFront::Atom(SpecFrontAtom {
                field,
                forward_alignments,
            }) => {
                check_field(errors, &fpath, field, FieldKind::Atom);
                front.push(Front::Atom(FrontAtom {
                    field: field.clone(),
                    forward_alignments: Rc::new(forward_alignments.iter().cloned().collect()),
                }));
            }
            SpecFront::Array(SpecFrontArray {
                field,
                prefix,
                suffix,
                separator,
                empty,
                forward_alignments,
            }) => {
                check_field(errors, &fpath, field, FieldKind::Array);
                let build = |errors: &mut Errors, name: &str, syms: &Vec<SpecSymbol>| {
                    syms.iter()
                        .enumerate()
                        .map(|(j, s)| {
                            build_symbol(errors, styles, &format!("{}.{}[{}]", fpath, name, j), &fields, s)
                        })
                        .collect::<Vec<_>>()
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
            }
        }
    }
    for (id, _) in &fields {
        if !used.contains(id) {
            errors.add(
                path,
                format!("field `{}` is captured by the back but not shown by the front (use `discard` in the back to drop it)", id),
            );
        }
    }
    for (name, a) in &t.alignments {
        if let SpecAlignment::Relative(r) = a {
            if r.base.is_empty() {
                errors.add(&format!("{}.alignments.{}", path, name), "empty base name");
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
        front,
        fields,
        is_pair,
    };
}
