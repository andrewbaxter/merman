//! Serde model of the syntax JSON file. Every enum is externally tagged with
//! snake_case names, e.g. `{"fixed_record": [...]}`.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn default_font_family() -> String {
    "monospace".to_string()
}

fn default_font_size() -> f64 {
    6.
}

fn default_converse_direction() -> SpecDirection {
    SpecDirection::Right
}

fn default_transverse_direction() -> SpecDirection {
    SpecDirection::Down
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpecDisplayUnit {
    Px,
    #[default]
    Mm,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpecDirection {
    Up,
    Down,
    Left,
    Right,
}

fn default_foreground() -> String {
    "#000000".to_string()
}

fn default_unprintable() -> String {
    "▢".to_string()
}

fn default_precedence() -> i64 {
    i64::MAX
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecSyntax {
    /// CSS color of the page.
    pub background: String,
    /// CSS color of text with no style.
    #[serde(default = "default_foreground")]
    pub foreground: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    /// In display units; like merman, the font's pixel size is the point size
    /// of this length (mm * 72 / 25.4).
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    /// Unit of all lengths in the syntax (font sizes, padding, alignment offsets).
    #[serde(default)]
    pub display_unit: SpecDisplayUnit,
    /// Direction text flows along a line.
    #[serde(default = "default_converse_direction")]
    pub converse_direction: SpecDirection,
    /// Direction lines stack; must be perpendicular to `converse_direction`.
    #[serde(default = "default_transverse_direction")]
    pub transverse_direction: SpecDirection,
    /// Space around the document.
    #[serde(default)]
    pub pad: SpecPadding,
    /// Fixed distance between line starts; 0 means each line takes its own
    /// ascent + descent.
    #[serde(default)]
    pub course_transverse_stride: f64,
    /// Replacement for control characters in primitives.
    #[serde(default = "default_unprintable")]
    pub unprintable: String,
    #[serde(default)]
    pub styles: BTreeMap<String, SpecStyle>,
    /// Box drawn around the selection.
    #[serde(default)]
    pub cursor: SpecObbox,
    /// Box drawn around what the mouse is over.
    #[serde(default)]
    pub hover: SpecObbox,
    /// Groups are ordered: when matching, members are tried first to last.
    #[serde(default)]
    pub groups: Vec<SpecGroup>,
    pub root: SpecTypeRoot,
    pub types: Vec<SpecType>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecStyle {
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub color: String,
    #[serde(default)]
    pub padding: SpecPadding,
    /// Override the font's ascent.
    pub ascent: Option<f64>,
    /// Override the font's descent.
    pub descent: Option<f64>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct SpecPadding {
    #[serde(default)]
    pub converse_start: f64,
    #[serde(default)]
    pub converse_end: f64,
    #[serde(default)]
    pub transverse_start: f64,
    #[serde(default)]
    pub transverse_end: f64,
}

fn default_true() -> bool {
    true
}

fn default_one() -> f64 {
    1.
}

fn default_black() -> String {
    "#000000".to_string()
}

fn default_white() -> String {
    "#ffffff".to_string()
}

/// Style of a box drawn around a range of bricks (merman's `ObboxStyle`).
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecObbox {
    #[serde(default)]
    pub padding: SpecPadding,
    #[serde(default)]
    pub round_start: bool,
    #[serde(default)]
    pub round_end: bool,
    #[serde(default)]
    pub round_outer_corners: bool,
    #[serde(default)]
    pub round_inner_corners: bool,
    #[serde(default)]
    pub round_concave: bool,
    #[serde(default)]
    pub round_radius: f64,
    #[serde(default = "default_true")]
    pub line: bool,
    #[serde(default = "default_black")]
    pub line_color: String,
    #[serde(default = "default_one")]
    pub line_thickness: f64,
    #[serde(default)]
    pub fill: bool,
    #[serde(default = "default_white")]
    pub fill_color: String,
}

impl Default for SpecObbox {
    fn default() -> Self {
        return SpecObbox {
            padding: SpecPadding::default(),
            round_start: false,
            round_end: false,
            round_outer_corners: false,
            round_inner_corners: false,
            round_concave: false,
            round_radius: 0.,
            line: true,
            line_color: default_black(),
            line_thickness: 1.,
            fill: false,
            fill_color: default_white(),
        };
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecGroup {
    pub id: String,
    /// Type ids or other group ids.
    pub members: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecTypeRoot {
    pub back: SpecBack,
    pub front: Vec<SpecFront>,
    #[serde(default)]
    pub alignments: BTreeMap<String, SpecAlignment>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecType {
    pub id: String,
    /// Human readable name.
    #[serde(default)]
    pub name: Option<String>,
    /// Lower precedence atoms are wrapped first and get parentheses (via the
    /// `precedent` condition) when nested in higher precedence atoms.
    #[serde(default = "default_precedence")]
    pub precedence: i64,
    #[serde(default)]
    pub associate_forward: bool,
    /// Added to the nesting score used to break ties when choosing what to wrap.
    #[serde(default)]
    pub depth_score: i64,
    #[serde(default)]
    pub alignments: BTreeMap<String, SpecAlignment>,
    pub back: SpecBack,
    pub front: Vec<SpecFront>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecAlignment {
    /// Positioned relative to the nearest alignment named `base` in an ancestor
    /// atom (an alignment may be based on one with the same name).
    Relative(SpecAlignmentRelative),
    /// Positioned at the largest natural position of all the lines using it.
    Concensus(SpecAlignmentConcensus),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecAlignmentRelative {
    pub base: String,
    #[serde(default)]
    pub offset: f64,
    /// If true, the offset only applies while some line actually starts at this
    /// alignment.
    #[serde(default)]
    pub collapse: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecAlignmentConcensus {}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecBack {
    /// A JSON string with exactly this content.
    FixedString(String),
    /// A JSON null, boolean or number whose source text is exactly this.
    FixedLiteral(String),
    /// Any JSON string, captured as primitive field `id`.
    String(SpecBackField),
    /// Any JSON number, captured as primitive field `id`.
    Number(SpecBackField),
    /// Any JSON null, boolean or number, captured as primitive field `id`.
    Literal(SpecBackField),
    /// A nested atom of type or group `type`, captured as atom field `id`.
    Atom(SpecBackAtom),
    /// A JSON array of atoms of type or group `element`, captured as array field
    /// `id`.
    Array(SpecBackArray),
    /// A single-key JSON object: `{some_key: X}` yields a one element array
    /// field, `{none_key: null}` an empty array field.
    Optional(SpecBackOptional),
    /// A JSON object with arbitrary keys, each entry matched by an atom of
    /// type or group `element` whose back is a `pair`. Captured as array field
    /// `id`.
    Record(SpecBackArray),
    /// Only valid as the back of a `record` element type: `key` matches the
    /// entry key (as a JSON string), `value` the entry value.
    Pair(SpecBackPair),
    /// A JSON array with exactly these elements.
    FixedArray(Vec<SpecBack>),
    /// A JSON object with exactly these keys, in any order.
    FixedRecord(Vec<SpecBackEntry>),
    /// Any JSON value, ignored.
    Discard(SpecBackDiscard),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackField {
    pub id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackAtom {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackArray {
    pub id: String,
    pub element: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackOptional {
    pub id: String,
    pub element: String,
    pub some_key: String,
    pub none_key: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackPair {
    pub key: Box<SpecBack>,
    pub value: Box<SpecBack>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackEntry {
    pub key: String,
    pub value: SpecBack,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackDiscard {}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecFront {
    Symbol(SpecSymbol),
    Primitive(SpecFrontPrimitive),
    Atom(SpecFrontAtom),
    Array(SpecFrontArray),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecSymbol {
    Text(SpecSymbolText),
    Space(SpecSymbolSpace),
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpecSplit {
    #[default]
    Never,
    /// Starts a new line when the owning atom has been compacted.
    Compact,
    Always,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecSymbolText {
    pub text: String,
    pub style: Option<String>,
    #[serde(default)]
    pub split: SpecSplit,
    /// Alignment used when this symbol doesn't start a line.
    pub alignment: Option<String>,
    /// Alignment used when this symbol starts a line.
    pub split_alignment: Option<String>,
    pub condition: Option<SpecCondition>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecSymbolSpace {
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub ascent: f64,
    #[serde(default)]
    pub descent: f64,
    #[serde(default)]
    pub split: SpecSplit,
    pub alignment: Option<String>,
    pub split_alignment: Option<String>,
    pub condition: Option<SpecCondition>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecCondition {
    /// Show if the primitive or array field is empty.
    Empty(SpecConditionEmpty),
    /// Show if this atom binds tighter than its parent (no parentheses needed).
    Precedent(SpecConditionPrecedent),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecConditionEmpty {
    pub field: String,
    #[serde(default)]
    pub invert: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecConditionPrecedent {
    #[serde(default)]
    pub invert: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecFrontPrimitive {
    pub field: String,
    pub style: Option<String>,
    #[serde(default)]
    pub split: SpecSplit,
    pub first_alignment: Option<String>,
    pub first_split_alignment: Option<String>,
    /// Alignment of lines after a newline in the text.
    pub hard_split_alignment: Option<String>,
    /// Alignment of lines created by wrapping.
    pub soft_split_alignment: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecFrontAtom {
    pub field: String,
    /// Names of this atom's alignments visible to the nested atom.
    #[serde(default)]
    pub forward_alignments: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecFrontArray {
    pub field: String,
    #[serde(default)]
    pub prefix: Vec<SpecSymbol>,
    #[serde(default)]
    pub suffix: Vec<SpecSymbol>,
    #[serde(default)]
    pub separator: Vec<SpecSymbol>,
    /// Shown instead of the elements when the array is empty.
    pub empty: Option<SpecSymbol>,
    #[serde(default)]
    pub forward_alignments: Vec<String>,
}
