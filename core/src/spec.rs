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
        gap_key: None,
    });
}

fn default_empty_gap_symbol(text: &str) -> Vec<SpecSymbol> {
    return vec![SpecSymbol::Text(SpecSymbolText {
        text: text.to_string(),
        style: None,
        split: SpecSplit::Never,
        alignment: None,
        split_alignment: None,
        condition: Some(SpecCondition::Empty(SpecConditionEmpty {
            field: "gap".to_string(),
            invert: false,
        })),
        gap_key: None,
    })];
}

fn default_gap() -> SpecGap {
    return SpecGap {
        prefix: vec![],
        style: None,
        suffix: default_gap_suffix(),
    };
}

fn default_gap_suffix() -> Vec<SpecSymbol> {
    return default_empty_gap_symbol("￮");
}

pub fn default_invalid_style() -> SpecTextStyle {
    return SpecTextStyle {
        ascent: None,
        color: "#ff8080".to_string(),
        descent: None,
        font_family: None,
        font_size: None,
        padding: SpecPadding::default(),
    };
}

fn default_one() -> f64 {
    return 1.;
}

fn default_precedence() -> i64 {
    return i64::MAX;
}

fn default_suffix_gap() -> SpecSuffixGap {
    return SpecSuffixGap {
        preceding_prefix: vec![],
        prefix: vec![],
        style: None,
        suffix: default_suffix_gap_suffix(),
    };
}

fn default_suffix_gap_suffix() -> Vec<SpecSymbol> {
    return default_empty_gap_symbol("▹");
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
    pub pattern: Option<SpecPattern>,
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
    pub invalid_style: Option<String>,
    /// Alignment of lines created by wrapping.
    pub soft_split_alignment: Option<String>,
    #[serde(default)]
    pub split: SpecSplit,
    pub style: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecGap {
    #[serde(default)]
    pub prefix: Vec<SpecSymbol>,
    pub style: Option<String>,
    #[serde(default = "default_gap_suffix")]
    pub suffix: Vec<SpecSymbol>,
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
    pub line_thickness_px: f64,
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
            line_thickness_px: 1.,
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecPattern {
    Any,
    CharacterClass(Vec<(String, String)>),
    Digits,
    Integer,
    JsonDecimal,
    Letters,
    Maybe(Box<SpecPattern>),
    Repeat0(Box<SpecPattern>),
    Repeat1(Box<SpecPattern>),
    Sequence(Vec<SpecPattern>),
    String(String),
    SymbolCharacter,
    Union(Vec<SpecPattern>),
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
#[serde(default, deny_unknown_fields)]
pub struct SpecSpacing {
    pub ai_input: f64,
    pub ai_message_gap: f64,
    pub ai_toolbar: f64,
    pub details: f64,
    pub error: f64,
    pub inline_gap: f64,
    pub panel: f64,
    pub row_converse: f64,
    pub row_transverse: f64,
    pub status_back_gap: f64,
    pub status_icon: f64,
    pub status_top: f64,
    pub toolbar_gap: f64,
}

impl Default for SpecSpacing {
    fn default() -> Self {
        return SpecSpacing {
            ai_input: 1.,
            ai_message_gap: 2.,
            ai_toolbar: 1.5,
            details: 2.,
            error: 4.,
            inline_gap: 1.5,
            panel: 3.,
            row_converse: 0.8,
            row_transverse: 0.25,
            status_back_gap: 5.,
            status_icon: 2.,
            status_top: 2.,
            toolbar_gap: 2.,
        };
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecSuffixGap {
    #[serde(default)]
    pub preceding_prefix: Vec<SpecSymbol>,
    #[serde(default)]
    pub prefix: Vec<SpecSymbol>,
    pub style: Option<String>,
    #[serde(default = "default_suffix_gap_suffix")]
    pub suffix: Vec<SpecSymbol>,
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
    pub gap_key: Option<String>,
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
    pub gap_key: Option<String>,
    #[serde(default)]
    pub split: SpecSplit,
    pub split_alignment: Option<String>,
    pub style: Option<String>,
    pub text: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecCompression {
    #[default]
    None,
    Zstd,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecSyntax {
    #[serde(default)]
    pub compression: SpecCompression,
    #[serde(default = "default_converse_direction")]
    pub converse_direction: SpecDirection,
    #[serde(default)]
    pub course_transverse_stride: f64,
    #[serde(default = "default_gap")]
    pub gap: SpecGap,
    #[serde(default)]
    pub groups: Vec<SpecGroup>,
    #[serde(default)]
    pub pad: SpecPadding,
    pub root: SpecTypeRoot,
    #[serde(default = "default_suffix_gap")]
    pub suffix_gap: SpecSuffixGap,
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
    pub details_background: String,
    pub font_family: String,
    pub font_size: f64,
    pub hover: SpecObbox,
    pub icon_color: String,
    pub line_gap: f64,
    #[serde(default)]
    pub spacing: SpecSpacing,
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
            line_thickness_px: 1.,
            line_color: line_color.to_string(),
            ..SpecObbox::default()
        };
        return SpecTheme {
            background: "#2b2b2b".to_string(),
            cursor: obbox("#ffffff"),
            details_background: "#19191980".to_string(),
            font_family: "monospace".to_string(),
            font_size: 4.,
            hover: obbox("#888888"),
            icon_color: "#e0c060".to_string(),
            line_gap: 0.,
            spacing: SpecSpacing::default(),
            text_color: "#cacaca".to_string(),
            text_styles: BTreeMap::from(
                [
                    ("ai_assistant", "#b4c6ec"),
                    ("ai_code", "#79bf97"),
                    ("ai_error", "#ff5555"),
                    ("ai_heading", "#8fb4ff"),
                    ("ai_link", "#7dd4fb"),
                    ("ai_user", "#e6d3a3"),
                ].map(|(name, color)| (name.to_string(), SpecTextStyle {
                    color: color.to_string(),
                    ..default_invalid_style()
                })),
            ),
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
    #[serde(default = "default_true")]
    pub auto_choose_unambiguous: bool,
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
    #[serde(default)]
    pub suffix_on_pattern_mismatch: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SpecTypeRoot {
    #[serde(default)]
    pub alignments: BTreeMap<String, SpecAlignment>,
    pub back: SpecBack,
    pub front: Vec<SpecFront>,
}
