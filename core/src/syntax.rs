//! Validated syntax: the spec with names resolved to indices and all cross
//! references checked.
use crate::direction::DirectionConvert;
use {
    crate::{
        display::PIXELS_PER_MM,
        error::{
            ErrorKind,
            MultiError,
        },
        measure::FontSpec,
        pattern::PatternMatcher,
        spec::{
            SpecAlignment,
            SpecBack,
            SpecCondition,
            SpecDirection,
            SpecFront,
            SpecFrontArray,
            SpecFrontArrayAsAtom,
            SpecFrontAtom,
            SpecFrontPrimitive,
            SpecPattern,
            SpecBackArray,
            SpecBackEntry,
            SpecBackField,
            SpecBackPair,
            SpecObbox,
            SpecPadding,
            SpecSplit,
            SpecSymbol,
            SpecSyntax,
            SpecTheme,
            SpecType,
            SpecTypeRoot,
            default_invalid_style,
        },
    },
    std::{
        collections::{
            BTreeMap,
            HashMap,
            HashSet,
        },
        rc::Rc,
    },
};

/// Type index of the document root.
pub const TYPE_ROOT: TypeId = 0;
pub const GROUP_ANY: &str = "__any";
pub const GAP_PAIR_KEY_PREFIX: &str = "__gap_pair_";
pub const GROUP_JSON: &str = "__json";
pub const GROUP_JSON_PAIRS: &str = "__json_pairs";
pub const STYLE_INVALID: &str = "invalid";

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

fn build_symbol(
    errors: &mut Errors,
    styles: &StyleTable,
    path: &str,
    fields: &HashMap<String, FieldKind>,
    s: &SpecSymbol,
) -> Symbol {
    let (kind, split, alignment, split_alignment, condition, gap_key) = match s {
        SpecSymbol::Text(t) => (SymbolKind::Text {
            text: t.text.clone(),
            style: styles.lookup(errors, path, &t.style),
        }, t.split, t.alignment.clone(), t.split_alignment.clone(), t.condition.clone(), t.gap_key.clone()),
        SpecSymbol::Space(t) => (SymbolKind::Space {
            width: t.width,
            ascent: t.ascent,
            descent: t.descent,
        }, t.split, t.alignment.clone(), t.split_alignment.clone(), t.condition.clone(), t.gap_key.clone()),
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
        gap_key: gap_key,
    };
}

fn build_type(errors: &mut Errors, styles: &StyleTable, path: &str, t: SpecType, gap: GapKind) -> TypeDef {
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
                    invalid_style,
                },
            ) => {
                check_field(errors, &fpath, field, FieldKind::Primitive);
                let style = styles.lookup(errors, &fpath, style);
                front.push(Front::Primitive(FrontPrimitive {
                    field: field.clone(),
                    invalid_style: match invalid_style {
                        Some(_) => styles.lookup(errors, &fpath, invalid_style),
                        None => styles.invalid,
                    },
                    style: style,
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
    if let Some(field) = &t.default_selection {
        if !fields.contains_key(field) {
            errors.multi_error_add(
                format!("{}.default_selection", path),
                ErrorKind::NonexistentDefaultSelection { field: field.clone() },
            );
        }
    }
    for (name, a) in &t.alignments {
        if let SpecAlignment::Relative(r) = a {
            if r.base.is_empty() {
                errors.multi_error_add(format!("{}.alignments.{}", path, name), ErrorKind::EmptyAlignmentBase);
            }
        }
    }
    let mut patterns = HashMap::new();
    for (id, kind) in &fields {
        if *kind != FieldKind::Primitive {
            continue;
        }
        let one = |s: &str| SpecPattern::String(s.to_string());
        let pattern = match crate::cursor::back_of_field(&t.back, id) {
            Some(SpecBack::String(f)) => f.pattern.clone(),
            Some(SpecBack::Number(f)) => Some(f.pattern.clone().unwrap_or(SpecPattern::JsonDecimal)),
            Some(SpecBack::Literal(f)) => Some(
                f
                    .pattern
                    .clone()
                    .unwrap_or(
                        SpecPattern::Union(vec![one("null"), one("true"), one("false"), SpecPattern::JsonDecimal]),
                    ),
            ),
            _ => None,
        };
        if let Some(pattern) = pattern {
            patterns.insert(id.clone(), Rc::new(PatternMatcher::pattern_new(&pattern)));
        }
    }
    return TypeDef {
        patterns: patterns,
        suffix_on_pattern_mismatch: t.suffix_on_pattern_mismatch,
        name: t.name.clone().unwrap_or_else(|| t.id.clone()),
        id: t.id,
        precedence: t.precedence,
        associate_forward: t.associate_forward,
        auto_choose_unambiguous: t.auto_choose_unambiguous,
        default_selection: t.default_selection,
        depth_score: t.depth_score,
        alignments: t.alignments.into_iter().collect(),
        back: t.back,
        front: front,
        fields: fields,
        is_pair: is_pair,
        gap: gap,
    };
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

type Errors = MultiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Array,
    Atom,
    Primitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    Gap,
    GapPair,
    None,
    SuffixGap,
}

pub enum Front {
    Array(FrontArray),
    Atom(FrontAtom),
    Primitive(FrontPrimitive),
    Symbol(Symbol),
}

pub struct FrontArray {
    pub empty: Option<Symbol>,
    pub field: String,
    pub forward_alignments: Rc<HashSet<String>>,
    pub prefix: Vec<Symbol>,
    pub separator: Vec<Symbol>,
    pub suffix: Vec<Symbol>,
}

pub struct FrontAtom {
    pub ellipsis: Symbol,
    pub field: String,
    pub forward_alignments: Rc<HashSet<String>>,
    pub from_array: bool,
}

pub struct FrontPrimitive {
    pub field: String,
    pub first_alignment: Option<String>,
    pub invalid_style: StyleId,
    pub first_split_alignment: Option<String>,
    pub hard_split_alignment: Option<String>,
    pub soft_split_alignment: Option<String>,
    pub split: SpecSplit,
    pub style: StyleId,
}

fn scale_obbox(o: &SpecObbox, to_pixels: f64) -> SpecObbox {
    return SpecObbox {
        padding: scale_padding(&o.padding, to_pixels),
        round_radius: o.round_radius * to_pixels,
        line_thickness: o.line_thickness * to_pixels,
        ..o.clone()
    };
}

fn scale_padding(p: &SpecPadding, to_pixels: f64) -> SpecPadding {
    return SpecPadding {
        converse_start: p.converse_start * to_pixels,
        converse_end: p.converse_end * to_pixels,
        transverse_start: p.transverse_start * to_pixels,
        transverse_end: p.transverse_end * to_pixels,
    };
}

/// Global settings copied out of the spec, lengths converted to px.
pub struct SpecSyntaxSettings {
    pub background: String,
    pub convert: DirectionConvert,
    pub course_transverse_stride: f64,
    pub cursor: SpecObbox,
    pub hover: SpecObbox,
    pub pad: SpecPadding,
    pub unprintable: String,
}

/// Lengths in px.
#[derive(Clone)]
pub struct Style {
    pub ascent: Option<f64>,
    pub color: String,
    pub descent: Option<f64>,
    pub font: FontSpec,
    pub padding: SpecPadding,
}

pub type StyleId = usize;

struct StyleTable {
    ids: HashMap<String, StyleId>,
    invalid: StyleId,
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

pub struct Symbol {
    pub alignment: Option<String>,
    pub condition: Option<SpecCondition>,
    pub gap_key: Option<String>,
    pub kind: SymbolKind,
    pub split: SpecSplit,
    pub split_alignment: Option<String>,
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

pub enum SymbolKind {
    Space {
        width: f64,
        ascent: f64,
        descent: f64,
    },
    Text {
        text: String,
        style: StyleId,
    },
}

pub struct Syntax {
    pub groups: HashMap<String, Vec<TypeId>>,
    pub spec_root: SpecSyntaxSettings,
    pub styles: Vec<Style>,
    pub type_gap: TypeId,
    pub type_json_root: TypeId,
    pub type_gap_pair: TypeId,
    pub type_suffix_gap: TypeId,
    pub types: Vec<TypeDef>,
}

impl Syntax {
    pub fn syntax_resolve(spec: SpecSyntax, theme: &SpecTheme) -> Result<Syntax, MultiError> {
        let mut errors = Errors::default();
        let to_pixels = PIXELS_PER_MM;

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
                family: theme.font_family.clone(),
                size: theme.font_size * to_pixels,
            },
            color: theme.text_color.clone(),
            padding: SpecPadding::default(),
            ascent: None,
            descent: None,
        }];
        let mut style_ids = HashMap::new();
        let mut text_styles = theme.text_styles.clone();
        text_styles.entry(STYLE_INVALID.to_string()).or_insert_with(default_invalid_style);
        for (name, s) in &text_styles {
            style_ids.insert(name.clone(), styles.len());
            styles.push(Style {
                font: FontSpec {
                    family: s.font_family.clone().unwrap_or_else(|| theme.font_family.clone()),
                    size: s.font_size.unwrap_or(theme.font_size) * to_pixels,
                },
                color: s.color.clone(),
                padding: scale_padding(&s.padding, to_pixels),
                ascent: s.ascent.map(|a| a * to_pixels),
                descent: s.descent.map(|d| d * to_pixels),
            });
        }
        let invalid = style_ids[STYLE_INVALID];
        let style_table = StyleTable {
            ids: style_ids,
            invalid: invalid,
        };

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
                auto_choose_unambiguous: false,
                suffix_on_pattern_mismatch: false,
                default_selection: None,
                depth_score: 0,
                alignments: alignments,
                back: back,
                front: front,
            }, GapKind::None));
        }
        for (i, t) in spec.types.into_iter().enumerate() {
            let path = format!("types[{}]({})", i, t.id);
            if t.id.starts_with("__") {
                errors.multi_error_add(&path, ErrorKind::ReservedAtomTypeId { atom_type: t.id.clone() });
            }
            if type_ids.insert(t.id.clone(), types.len()).is_some() {
                errors.multi_error_add(&path, ErrorKind::DuplicateAtomTypeIds { atom_type: t.id.clone() });
            }
            types.push(build_type(&mut errors, &style_table, &path, t, GapKind::None));
        }
        let gap_text = |style: &Option<String>| SpecFront::Primitive(SpecFrontPrimitive {
            field: "gap".to_string(),
            style: style.clone(),
            split: SpecSplit::Never,
            first_alignment: None,
            first_split_alignment: None,
            hard_split_alignment: None,
            soft_split_alignment: None,
            invalid_style: None,
        });
        let gap_back = || SpecBack::FixedRecord(vec![SpecBackEntry {
            key: "__gap".to_string(),
            value: Some(SpecBack::String(SpecBackField {
                id: "gap".to_string(),
                pattern: None,
            })),
        }]);
        let symbols =
            |symbols: &Vec<SpecSymbol>| symbols.iter().cloned().map(SpecFront::Symbol).collect::<Vec<_>>();
        let builtin = |id: &str, name: &str, back: SpecBack, front: Vec<SpecFront>| SpecType {
            id: id.to_string(),
            name: Some(name.to_string()),
            precedence: 1_000_000,
            associate_forward: false,
            auto_choose_unambiguous: false,
            suffix_on_pattern_mismatch: false,
            default_selection: None,
            depth_score: 0,
            alignments: BTreeMap::new(),
            back: back,
            front: front,
        };
        let gap_front =
            [symbols(&spec.gap.prefix), vec![gap_text(&spec.gap.style)], symbols(&spec.gap.suffix)].concat();
        let type_gap = types.len();
        types.push(
            build_type(
                &mut errors,
                &style_table,
                "gap",
                builtin("__gap", "Gap", gap_back(), gap_front.clone()),
                GapKind::Gap,
            ),
        );
        let type_gap_pair = types.len();
        types.push(
            build_type(
                &mut Errors::default(),
                &style_table,
                "gap",
                builtin("__gap_pair", "Gap", SpecBack::Pair(SpecBackPair {
                    key: Box::new(SpecBack::String(SpecBackField {
                        id: "key".to_string(),
                        pattern: None,
                    })),
                    value: Box::new(gap_back()),
                }), gap_front),
                GapKind::GapPair,
            ),
        );
        let container_alignments = serde_json::json!({
            "base": {
                "relative": {
                    "base": "indent",
                    "offset": 0
                }
            },
            "indent": {
                "relative": {
                    "base": "indent",
                    "offset": 9.6,
                    "collapse": true
                }
            }
        });
        let container_front = |open: &str, field: &str, close: &str| serde_json::json!([{
            "symbol": {
                "text": {
                    "text": open,
                    "style": STYLE_INVALID
                }
            }
        }, {
            "array": {
                "field": field,
                "prefix":[{
                    "space": {
                        "split": "compact",
                        "split_alignment": "indent"
                    }
                }],
                "separator":[{
                    "text": {
                        "text": ", ",
                        "style": STYLE_INVALID
                    }
                }],
                "forward_alignments":["indent"]
            }
        }, {
            "symbol": {
                "text": {
                    "text": close,
                    "style": STYLE_INVALID,
                    "split": "compact",
                    "split_alignment": "base"
                }
            }
        }]);
        let literal = |text: &str| serde_json::json!({
            "back": {
                "fixed_literal": text
            },
            "front":[{
                "symbol": {
                    "text": {
                        "text": text,
                        "style": STYLE_INVALID
                    }
                }
            }]
        });
        let json_types =
            [
                ("__json_object", "JSON object", serde_json::json!({
                    "back": {
                        "record": {
                            "id": "entries",
                            "element": GROUP_JSON_PAIRS
                        }
                    },
                    "alignments": container_alignments,
                    "front": container_front("{", "entries", "}")
                })),
                ("__json_pair", "JSON entry", serde_json::json!({
                    "back": {
                        "pair": {
                            "key": {
                                "string": {
                                    "id": "key"
                                }
                            },
                            "value": {
                                "atom": {
                                    "id": "value",
                                    "type": GROUP_JSON
                                }
                            }
                        }
                    },
                    "front":[{
                        "primitive": {
                            "field": "key",
                            "style": STYLE_INVALID
                        }
                    }, {
                        "symbol": {
                            "text": {
                                "text": ": ",
                                "style": STYLE_INVALID
                            }
                        }
                    }, {
                        "atom": {
                            "field": "value"
                        }
                    }]
                })),
                ("__json_array", "JSON array", serde_json::json!({
                    "back": {
                        "array": {
                            "id": "elements",
                            "element": GROUP_JSON
                        }
                    },
                    "alignments": container_alignments,
                    "front": container_front("[", "elements", "]")
                })),
                ("__json_string", "JSON string", serde_json::json!({
                    "back": {
                        "string": {
                            "id": "value"
                        }
                    },
                    "front":[{
                        "symbol": {
                            "text": {
                                "text": "\"",
                                "style": STYLE_INVALID
                            }
                        }
                    }, {
                        "primitive": {
                            "field": "value",
                            "style": STYLE_INVALID,
                            "soft_split_alignment": "indent"
                        }
                    }, {
                        "symbol": {
                            "text": {
                                "text": "\"",
                                "style": STYLE_INVALID
                            }
                        }
                    }]
                })),
                ("__json_number", "JSON number", serde_json::json!({
                    "back": {
                        "number": {
                            "id": "value"
                        }
                    },
                    "front":[{
                        "primitive": {
                            "field": "value",
                            "style": STYLE_INVALID
                        }
                    }]
                })),
                ("__json_true", "JSON true", literal("true")),
                ("__json_false", "JSON false", literal("false")),
                ("__json_null", "JSON null", literal("null")),
            ];
        let mut json_group = vec![];
        let mut json_pairs = vec![];
        for (id, name, mut spec) in json_types {
            spec["id"] = serde_json::Value::String(id.to_string());
            spec["name"] = serde_json::Value::String(name.to_string());
            let spec: SpecType = serde_json::from_value(spec).expect("built in JSON type is invalid");
            if matches!(spec.back, SpecBack::Pair(_)) {
                json_pairs.push(types.len());
            } else {
                json_group.push(types.len());
            }
            types.push(build_type(&mut errors, &style_table, "json", spec, GapKind::None));
        }
        let type_json_root = types.len();
        types.push(build_type(&mut errors, &style_table, "json", serde_json::from_value(serde_json::json!({
            "id": "__json_root",
            "name": "JSON",
            "back": {
                "atom": {
                    "id": "value",
                    "type": GROUP_JSON
                }
            },
            "front":[{
                "atom": {
                    "field": "value"
                }
            }]
        })).expect("built in JSON type is invalid"), GapKind::None));
        let type_suffix_gap = types.len();
        types.push(
            build_type(
                &mut errors,
                &style_table,
                "suffix_gap",
                builtin(
                    "__suffix_gap",
                    "Suffix gap",
                    SpecBack::FixedRecord(vec![SpecBackEntry {
                        key: "__suffix_gap".to_string(),
                        value: Some(SpecBack::FixedRecord(vec![SpecBackEntry {
                            key: "text".to_string(),
                            value: Some(SpecBack::String(SpecBackField {
                                id: "gap".to_string(),
                                pattern: None,
                            })),
                        }, SpecBackEntry {
                            key: "preceding".to_string(),
                            value: Some(SpecBack::Array(SpecBackArray {
                                id: "preceding".to_string(),
                                element: GROUP_ANY.to_string(),
                            })),
                        }])),
                    }]),
                    [
                        vec![SpecFront::Array(SpecFrontArray {
                            field: "preceding".to_string(),
                            empty: None,
                            forward_alignments: vec![],
                            prefix: spec.suffix_gap.preceding_prefix.clone(),
                            separator: vec![],
                            suffix: vec![],
                        })],
                        symbols(&spec.suffix_gap.prefix),
                        vec![gap_text(&spec.suffix_gap.style)],
                        symbols(&spec.suffix_gap.suffix),
                    ].concat(),
                ),
                GapKind::SuffixGap,
            ),
        );

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
        groups.insert(GROUP_JSON.to_string(), json_group);
        groups.insert(GROUP_JSON_PAIRS.to_string(), json_pairs);
        groups.insert(
            GROUP_ANY.to_string(),
            (0 .. types.len())
                .filter(
                    |t| *t != TYPE_ROOT && *t != type_json_root && types[*t].gap == GapKind::None &&
                        !types[*t].is_pair,
                )
                .collect(),
        );

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
                background: theme.background.clone(),
                convert: DirectionConvert::new(spec.converse_direction, spec.transverse_direction),
                pad: scale_padding(&spec.pad, to_pixels),
                course_transverse_stride: spec.course_transverse_stride * to_pixels,
                unprintable: spec.unprintable,
                cursor: scale_obbox(&theme.cursor, to_pixels),
                hover: scale_obbox(&theme.hover, to_pixels),
            },
            types: types,
            groups: groups,
            styles: styles,
            type_gap: type_gap,
            type_json_root: type_json_root,
            type_gap_pair: type_gap_pair,
            type_suffix_gap: type_suffix_gap,
        });
    }

    pub fn syntax_group_types(&self, group: &str) -> Vec<TypeId> {
        let members = self.groups.get(group).unwrap_or_else(|| panic!("unknown group `{}`", group));
        let mut out = members.clone();
        if members.first().is_some_and(|t| self.types[*t].is_pair) {
            out.push(self.type_gap_pair);
        } else {
            out.push(self.type_gap);
            out.push(self.type_suffix_gap);
        }
        return out;
    }

    pub fn syntax_primitive_valid(&self, type_: TypeId, field: &str, text: &str) -> bool {
        let t = self.syntax_type(type_);
        let Some(pattern) = t.patterns.get(field) else {
            return true;
        };
        let glyphs: Vec<String> =
            unicode_segmentation::UnicodeSegmentation::graphemes(text, true).map(|g| g.to_string()).collect();
        if !pattern.pattern_matches(&glyphs, false) {
            return false;
        }
        match crate::cursor::back_of_field(&t.back, field) {
            Some(SpecBack::Number(_)) => return !crate::serialize::serialize_literal(text, true).is_string(),
            Some(SpecBack::Literal(_)) => return !crate::serialize::serialize_literal(text, false).is_string(),
            _ => return true,
        }
    }

    pub fn syntax_style(&self, id: StyleId) -> &Style {
        return &self.styles[id];
    }

    pub fn syntax_type(&self, id: TypeId) -> &TypeDef {
        return &self.types[id];
    }
}

pub struct TypeDef {
    pub alignments: Vec<(String, SpecAlignment)>,
    pub associate_forward: bool,
    pub auto_choose_unambiguous: bool,
    pub back: SpecBack,
    pub default_selection: Option<String>,
    pub depth_score: i64,
    pub fields: HashMap<String, FieldKind>,
    pub front: Vec<Front>,
    pub gap: GapKind,
    pub id: String,
    pub is_pair: bool,
    pub name: String,
    pub patterns: HashMap<String, Rc<PatternMatcher>>,
    pub precedence: i64,
    pub suffix_on_pattern_mismatch: bool,
}

pub type TypeId = usize;
