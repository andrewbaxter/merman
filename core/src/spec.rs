use {
    serde::{
        Deserialize,
        Serialize,
    },
    std::collections::BTreeMap,
};

fn default_black() -> String {
    return "#000000".to_string();
}

fn default_converse_direction() -> SpecDirection {
    return SpecDirection::Right;
}

fn default_ellipsis() -> SpecSymbol {
    return SpecSymbol::Text(SpecSymbolText {
        text: "...".to_string(),
        style: None,
        split: SpecSplit::Never,
        alignment: None,
        split_alignment: None,
        condition: None,
    });
}

fn default_one() -> f64 {
    return 1.;
}

fn default_precedence() -> i64 {
    return i64::MAX;
}

fn default_transverse_direction() -> SpecDirection {
    return SpecDirection::Down;
}

fn default_true() -> bool {
    return true;
}

fn default_unprintable() -> String {
    return "▢".to_string();
}

fn default_white() -> String {
    return "#ffffff".to_string();
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecAlignment {
    /// Positioned at the largest natural position of all the lines using it.
    Concensus(SpecAlignmentConcensus),
    Relative(SpecAlignmentRelative),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecAlignmentConcensus {}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecAlignmentRelative {
    pub base: String,
    /// If true, the offset only applies while some line actually starts at this
    /// alignment.
    #[serde(default)]
    pub collapse: bool,
    #[serde(default)]
    pub offset: f64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecBack {
    Array(SpecBackArray),
    Atom(SpecBackAtom),
    FixedArray(Vec<SpecBack>),
    /// A JSON null, boolean or number whose source text is exactly this.
    FixedLiteral(String),
    FixedRecord(Vec<SpecBackEntry>),
    FixedString(String),
    FixedSubArray(Vec<SpecBack>),
    Id(SpecBackId),
    /// Any JSON null, boolean or number, captured as primitive field `id`.
    Literal(SpecBackField),
    Number(SpecBackField),
    /// A single-key JSON object: `{some_key: X}` yields a one element array field,
    /// `{none_key: null}` an empty array field.
    Optional(SpecBackOptional),
    /// Only valid as the back of a `record` element type: `key` matches the entry key
    /// (as a JSON string), `value` the entry value.
    Pair(SpecBackPair),
    Record(SpecBackArray),
    String(SpecBackField),
    SubArray(SpecBackArray),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackArray {
    pub element: String,
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
pub struct SpecBackEntry {
    pub key: String,
    #[serde(default)]
    pub value: Option<SpecBack>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackField {
    pub id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackId {
    #[serde(default)]
    pub unique: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackOptional {
    pub element: String,
    pub id: String,
    pub none_key: String,
    pub some_key: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecBackPair {
    pub key: Box<SpecBack>,
    pub value: Box<SpecBack>,
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

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpecDirection {
    Down,
    Left,
    Right,
    Up,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecFront {
    Array(SpecFrontArray),
    ArrayAsAtom(SpecFrontArrayAsAtom),
    Atom(SpecFrontAtom),
    Primitive(SpecFrontPrimitive),
    Symbol(SpecSymbol),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecFrontArray {
    pub empty: Option<SpecSymbol>,
    pub field: String,
    #[serde(default)]
    pub forward_alignments: Vec<String>,
    #[serde(default)]
    pub prefix: Vec<SpecSymbol>,
    #[serde(default)]
    pub separator: Vec<SpecSymbol>,
    #[serde(default)]
    pub suffix: Vec<SpecSymbol>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecFrontArrayAsAtom {
    #[serde(default = "default_ellipsis")]
    pub ellipsis: SpecSymbol,
    pub field: String,
    #[serde(default)]
    pub forward_alignments: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecFrontAtom {
    #[serde(default = "default_ellipsis")]
    pub ellipsis: SpecSymbol,
    pub field: String,
    #[serde(default)]
    pub forward_alignments: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecFrontPrimitive {
    pub field: String,
    pub first_alignment: Option<String>,
    pub first_split_alignment: Option<String>,
    /// Alignment of lines after a newline in the text.
    pub hard_split_alignment: Option<String>,
    /// Alignment of lines created by wrapping.
    pub soft_split_alignment: Option<String>,
    #[serde(default)]
    pub split: SpecSplit,
    pub style: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecGroup {
    pub id: String,
    /// Names of this atom's alignments visible to the nested atom.
    pub members: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecObbox {
    #[serde(default)]
    pub fill: bool,
    #[serde(default = "default_white")]
    pub fill_color: String,
    #[serde(default = "default_true")]
    pub line: bool,
    #[serde(default = "default_black")]
    pub line_color: String,
    #[serde(default = "default_one")]
    pub line_thickness: f64,
    #[serde(default)]
    pub padding: SpecPadding,
    #[serde(default)]
    pub round_concave: bool,
    #[serde(default)]
    pub round_end: bool,
    #[serde(default)]
    pub round_inner_corners: bool,
    #[serde(default)]
    pub round_outer_corners: bool,
    #[serde(default)]
    pub round_radius: f64,
    #[serde(default)]
    pub round_start: bool,
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

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct SpecPadding {
    #[serde(default)]
    pub converse_end: f64,
    #[serde(default)]
    pub converse_start: f64,
    #[serde(default)]
    pub transverse_end: f64,
    #[serde(default)]
    pub transverse_start: f64,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpecSplit {
    Always,
    Compact,
    #[default]
    Never,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecTextStyle {
    pub ascent: Option<f64>,
    pub color: String,
    pub descent: Option<f64>,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    #[serde(default)]
    pub padding: SpecPadding,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecSymbol {
    Space(SpecSymbolSpace),
    Text(SpecSymbolText),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecSymbolSpace {
    pub alignment: Option<String>,
    #[serde(default)]
    pub ascent: f64,
    pub condition: Option<SpecCondition>,
    #[serde(default)]
    pub descent: f64,
    #[serde(default)]
    pub split: SpecSplit,
    pub split_alignment: Option<String>,
    #[serde(default)]
    pub width: f64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecSymbolText {
    pub alignment: Option<String>,
    pub condition: Option<SpecCondition>,
    #[serde(default)]
    pub split: SpecSplit,
    pub split_alignment: Option<String>,
    pub style: Option<String>,
    pub text: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecSyntax {
    #[serde(default = "default_converse_direction")]
    pub converse_direction: SpecDirection,
    #[serde(default)]
    pub course_transverse_stride: f64,
    #[serde(default)]
    pub groups: Vec<SpecGroup>,
    #[serde(default)]
    pub pad: SpecPadding,
    pub root: SpecTypeRoot,
    #[serde(default = "default_transverse_direction")]
    pub transverse_direction: SpecDirection,
    pub types: Vec<SpecType>,
    #[serde(default = "default_unprintable")]
    pub unprintable: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecTheme {
    pub background: String,
    pub cursor: SpecObbox,
    pub error_color: String,
    pub font_family: String,
    pub font_size: f64,
    pub hover: SpecObbox,
    pub icon_color: String,
    pub text_color: String,
    pub text_styles: BTreeMap<String, SpecTextStyle>,
}

impl Default for SpecTheme {
    fn default() -> Self {
        let obbox = |line_color: &str| SpecObbox {
            padding: SpecPadding {
                converse_start: 1.,
                converse_end: 1.,
                transverse_start: 1.,
                transverse_end: 1.,
            },
            round_start: true,
            round_end: true,
            round_radius: 3.,
            line_thickness: 0.3,
            line_color: line_color.to_string(),
            ..SpecObbox::default()
        };
        return SpecTheme {
            background: "#2b2b2b".to_string(),
            cursor: obbox("#ffffff"),
            error_color: "#ff8080".to_string(),
            font_family: "monospace".to_string(),
            font_size: 4.,
            hover: obbox("#888888"),
            icon_color: "#e0c060".to_string(),
            text_color: "#cacaca".to_string(),
            text_styles: BTreeMap::new(),
        };
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecType {
    #[serde(default)]
    pub alignments: BTreeMap<String, SpecAlignment>,
    #[serde(default)]
    pub associate_forward: bool,
    pub back: SpecBack,
    #[serde(default)]
    pub default_selection: Option<String>,
    #[serde(default)]
    pub depth_score: i64,
    pub front: Vec<SpecFront>,
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_precedence")]
    pub precedence: i64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecTypeRoot {
    #[serde(default)]
    pub alignments: BTreeMap<String, SpecAlignment>,
    pub back: SpecBack,
    pub front: Vec<SpecFront>,
}
