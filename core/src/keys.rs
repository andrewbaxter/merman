use {
    crate::{
        cursor::CursorKind,
        direction::DirectionKey,
        error::{
            ErrorKind,
            MultiError,
        },
    },
    serde::{
        Deserialize,
        Deserializer,
        Serialize,
        Serializer,
        de::Error as _,
    },
    std::collections::HashMap,
};

pub const ACTIONS: [(Action, &str); 57] =
    [
        (Action::Enter, "enter"),
        (Action::Exit, "exit"),
        (Action::Copy, "copy"),
        (Action::AiOpen, "ai_open"),
        (Action::AiOpenReference, "ai_open_reference"),
        (Action::Window, "window"),
        (Action::NextElement, "next_element"),
        (Action::PreviousElement, "previous_element"),
        (Action::FirstElement, "first_element"),
        (Action::LastElement, "last_element"),
        (Action::NextGlyph, "next_glyph"),
        (Action::PreviousGlyph, "previous_glyph"),
        (Action::FirstGlyph, "first_glyph"),
        (Action::LastGlyph, "last_glyph"),
        (Action::NextWord, "next_word"),
        (Action::PreviousWord, "previous_word"),
        (Action::NextLine, "next_line"),
        (Action::PreviousLine, "previous_line"),
        (Action::LineBegin, "line_begin"),
        (Action::LineEnd, "line_end"),
        (Action::GatherNext, "gather_next"),
        (Action::GatherPrevious, "gather_previous"),
        (Action::GatherFirst, "gather_first"),
        (Action::GatherLast, "gather_last"),
        (Action::GatherNextGlyph, "gather_next_glyph"),
        (Action::GatherPreviousGlyph, "gather_previous_glyph"),
        (Action::GatherNextWord, "gather_next_word"),
        (Action::GatherPreviousWord, "gather_previous_word"),
        (Action::GatherNextLine, "gather_next_line"),
        (Action::GatherPreviousLine, "gather_previous_line"),
        (Action::GatherNextLineEnd, "gather_next_line_end"),
        (Action::GatherPreviousLineStart, "gather_previous_line_start"),
        (Action::ReleaseAll, "release_all"),
        (Action::ReleaseNext, "release_next"),
        (Action::ReleasePrevious, "release_previous"),
        (Action::ReleaseNextGlyph, "release_next_glyph"),
        (Action::ReleasePreviousGlyph, "release_previous_glyph"),
        (Action::ReleaseNextWord, "release_next_word"),
        (Action::ReleasePreviousWord, "release_previous_word"),
        (Action::ReleaseNextLine, "release_next_line"),
        (Action::ReleasePreviousLine, "release_previous_line"),
        (Action::ReleaseNextLineEnd, "release_next_line_end"),
        (Action::ReleasePreviousLineStart, "release_previous_line_start"),
        (Action::SelectNext, "select_next"),
        (Action::SelectPrevious, "select_previous"),
        (Action::SelectNextGlyph, "select_next_glyph"),
        (Action::SelectPreviousGlyph, "select_previous_glyph"),
        (Action::SelectNextWord, "select_next_word"),
        (Action::SelectPreviousWord, "select_previous_word"),
        (Action::ClearWindow, "clear_window"),
        (Action::WindowTowardsRoot, "window_towards_root"),
        (Action::WindowTowardsCursor, "window_towards_cursor"),
        (Action::ScrollNext, "scroll_next"),
        (Action::ScrollNextAlot, "scroll_next_alot"),
        (Action::ScrollPrevious, "scroll_previous"),
        (Action::ScrollPreviousAlot, "scroll_previous_alot"),
        (Action::ScrollReset, "scroll_reset"),
    ];
const NAMED_KEYS: [(&str, KeyName); 164] =
    [
        ("enter", KeyName::Enter),
        ("backspace", KeyName::Backspace),
        ("tab", KeyName::Tab),
        ("cancel", KeyName::Cancel),
        ("clear", KeyName::Clear),
        ("shift", KeyName::Shift),
        ("control", KeyName::Control),
        ("alt", KeyName::Alt),
        ("pause", KeyName::Pause),
        ("caps", KeyName::Caps),
        ("escape", KeyName::Escape),
        ("space", KeyName::Space),
        ("page_up", KeyName::PageUp),
        ("page_down", KeyName::PageDown),
        ("end", KeyName::End),
        ("home", KeyName::Home),
        ("surface", KeyName::Surface),
        ("previous", KeyName::Previous),
        ("dive", KeyName::Dive),
        ("next", KeyName::Next),
        ("numpad_add", KeyName::NumpadAdd),
        ("numpad_changesign", KeyName::NumpadChangesign),
        ("numpad_comma", KeyName::NumpadComma),
        ("numpad_decimal", KeyName::NumpadDecimal),
        ("numpad_divide", KeyName::NumpadDivide),
        ("numpad_enter", KeyName::NumpadEnter),
        ("numpad_equal", KeyName::NumpadEqual),
        ("numpad_multiply", KeyName::NumpadMultiply),
        ("numpad_leftparen", KeyName::NumpadLeftparen),
        ("numpad_rightparen", KeyName::NumpadRightparen),
        ("numpad_subtract", KeyName::NumpadSubtract),
        ("multiply", KeyName::Multiply),
        ("add", KeyName::Add),
        ("separator", KeyName::Separator),
        ("subtract", KeyName::Subtract),
        ("decimal", KeyName::Decimal),
        ("divide", KeyName::Divide),
        ("delete", KeyName::Delete),
        ("num_lock", KeyName::NumLock),
        ("scroll_lock", KeyName::ScrollLock),
        ("printscreen", KeyName::Printscreen),
        ("insert", KeyName::Insert),
        ("help", KeyName::Help),
        ("meta", KeyName::Meta),
        ("kp_up", KeyName::KpUp),
        ("kp_down", KeyName::KpDown),
        ("kp_left", KeyName::KpLeft),
        ("kp_right", KeyName::KpRight),
        ("dead_grave", KeyName::DeadGrave),
        ("dead_acute", KeyName::DeadAcute),
        ("dead_circumflex", KeyName::DeadCircumflex),
        ("dead_tilde", KeyName::DeadTilde),
        ("dead_macron", KeyName::DeadMacron),
        ("dead_breve", KeyName::DeadBreve),
        ("dead_abovedot", KeyName::DeadAbovedot),
        ("dead_diaeresis", KeyName::DeadDiaeresis),
        ("dead_abovering", KeyName::DeadAbovering),
        ("dead_doubleacute", KeyName::DeadDoubleacute),
        ("dead_caron", KeyName::DeadCaron),
        ("dead_cedilla", KeyName::DeadCedilla),
        ("dead_ogonek", KeyName::DeadOgonek),
        ("dead_iota", KeyName::DeadIota),
        ("dead_voiced_sound", KeyName::DeadVoicedSound),
        ("dead_semivoiced_sound", KeyName::DeadSemivoicedSound),
        ("windows", KeyName::Windows),
        ("context_menu", KeyName::ContextMenu),
        ("final", KeyName::Final),
        ("convert", KeyName::Convert),
        ("nonconvert", KeyName::Nonconvert),
        ("accept", KeyName::Accept),
        ("modechange", KeyName::Modechange),
        ("kana", KeyName::Kana),
        ("kanji", KeyName::Kanji),
        ("alphanumeric", KeyName::Alphanumeric),
        ("katakana", KeyName::Katakana),
        ("hiragana", KeyName::Hiragana),
        ("full_width", KeyName::FullWidth),
        ("half_width", KeyName::HalfWidth),
        ("roman_characters", KeyName::RomanCharacters),
        ("all_candidates", KeyName::AllCandidates),
        ("previous_candidate", KeyName::PreviousCandidate),
        ("code_input", KeyName::CodeInput),
        ("japanese_katakana", KeyName::JapaneseKatakana),
        ("japanese_hiragana", KeyName::JapaneseHiragana),
        ("japanese_roman", KeyName::JapaneseRoman),
        ("kana_lock", KeyName::KanaLock),
        ("input_method_on_off", KeyName::InputMethodOnOff),
        ("cut", KeyName::Cut),
        ("copy", KeyName::Copy),
        ("paste", KeyName::Paste),
        ("undo", KeyName::Undo),
        ("again", KeyName::Again),
        ("find", KeyName::Find),
        ("props", KeyName::Props),
        ("stop", KeyName::Stop),
        ("compose", KeyName::Compose),
        ("alt_graph", KeyName::AltGraph),
        ("begin", KeyName::Begin),
        ("undefined", KeyName::Undefined),
        ("game_a", KeyName::GameA),
        ("game_b", KeyName::GameB),
        ("game_c", KeyName::GameC),
        ("game_d", KeyName::GameD),
        ("star", KeyName::Star),
        ("pound", KeyName::Pound),
        ("power", KeyName::Power),
        ("info", KeyName::Info),
        ("colored_key_0", KeyName::ColoredKey0),
        ("colored_key_1", KeyName::ColoredKey1),
        ("colored_key_2", KeyName::ColoredKey2),
        ("colored_key_3", KeyName::ColoredKey3),
        ("eject_toggle", KeyName::EjectToggle),
        ("play", KeyName::Play),
        ("record", KeyName::Record),
        ("fast_fwd", KeyName::FastFwd),
        ("rewind", KeyName::Rewind),
        ("track_prev", KeyName::TrackPrev),
        ("track_next", KeyName::TrackNext),
        ("channel_up", KeyName::ChannelUp),
        ("channel_down", KeyName::ChannelDown),
        ("volume_up", KeyName::VolumeUp),
        ("volume_down", KeyName::VolumeDown),
        ("mute", KeyName::Mute),
        ("command", KeyName::Command),
        ("shortcut", KeyName::Shortcut),
        ("alt_left", KeyName::AltLeft),
        ("alt_right", KeyName::AltRight),
        ("browser_back", KeyName::BrowserBack),
        ("browser_favorites", KeyName::BrowserFavorites),
        ("browser_forward", KeyName::BrowserForward),
        ("browser_home", KeyName::BrowserHome),
        ("browser_refresh", KeyName::BrowserRefresh),
        ("browser_search", KeyName::BrowserSearch),
        ("browser_stop", KeyName::BrowserStop),
        ("control_left", KeyName::ControlLeft),
        ("control_right", KeyName::ControlRight),
        ("intl_hangul_mode", KeyName::IntlHangulMode),
        ("intl_hanja", KeyName::IntlHanja),
        ("intl_back_slash", KeyName::IntlBackSlash),
        ("intl_ro", KeyName::IntlRo),
        ("intl_yen", KeyName::IntlYen),
        ("media_play_pause", KeyName::MediaPlayPause),
        ("media_stop", KeyName::MediaStop),
        ("media_next", KeyName::MediaNext),
        ("media_previous", KeyName::MediaPrevious),
        ("meta_left", KeyName::MetaLeft),
        ("meta_right", KeyName::MetaRight),
        ("open", KeyName::Open),
        ("shift_left", KeyName::ShiftLeft),
        ("shift_right", KeyName::ShiftRight),
        ("select", KeyName::Select),
        ("wake", KeyName::Wake),
        ("lang1", KeyName::Lang1),
        ("lang2", KeyName::Lang2),
        ("launch_media_player", KeyName::LaunchMediaPlayer),
        ("launch_mail", KeyName::LaunchMail),
        ("launch_app2", KeyName::LaunchApp2),
        ("launch_app1", KeyName::LaunchApp1),
        ("os_right", KeyName::OsRight),
        ("os_left", KeyName::OsLeft),
        ("mouse_scroll_left", KeyName::MouseScrollLeft),
        ("mouse_scroll_right", KeyName::MouseScrollRight),
        ("mouse_scroll_in", KeyName::MouseScrollIn),
        ("mouse_scroll_out", KeyName::MouseScrollOut),
    ];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    AiOpen,
    AiOpenReference,
    ClearWindow,
    Copy,
    Enter,
    Exit,
    FirstElement,
    FirstGlyph,
    GatherFirst,
    GatherLast,
    GatherNext,
    GatherNextGlyph,
    GatherNextLine,
    GatherNextLineEnd,
    GatherNextWord,
    GatherPrevious,
    GatherPreviousGlyph,
    GatherPreviousLine,
    GatherPreviousLineStart,
    GatherPreviousWord,
    LastElement,
    LastGlyph,
    LineBegin,
    LineEnd,
    NextElement,
    NextGlyph,
    NextLine,
    NextWord,
    PreviousElement,
    PreviousGlyph,
    PreviousLine,
    PreviousWord,
    ReleaseAll,
    ReleaseNext,
    ReleaseNextGlyph,
    ReleaseNextLine,
    ReleaseNextLineEnd,
    ReleaseNextWord,
    ReleasePrevious,
    ReleasePreviousGlyph,
    ReleasePreviousLine,
    ReleasePreviousLineStart,
    ReleasePreviousWord,
    ScrollNext,
    ScrollNextAlot,
    ScrollPrevious,
    ScrollPreviousAlot,
    ScrollReset,
    SelectNext,
    SelectNextGlyph,
    SelectNextWord,
    SelectPrevious,
    SelectPreviousGlyph,
    SelectPreviousWord,
    Window,
    WindowTowardsCursor,
    WindowTowardsRoot,
}

impl Action {
    pub fn action_id(self) -> &'static str {
        return ACTIONS.iter().find(|(a, _)| *a == self).unwrap().1;
    }
}

const fn ctrl(key: KeyName) -> KeyStroke {
    return KeyStroke {
        ctrl: true,
        ..KeyStroke::key_stroke_new(key)
    };
}

const fn ctrl_shift(key: KeyName) -> KeyStroke {
    return KeyStroke {
        ctrl: true,
        shift: true,
        ..KeyStroke::key_stroke_new(key)
    };
}

fn is_false(v: &bool) -> bool {
    return !*v;
}

#[derive(Clone)]
pub struct Keymap {
    array: Section,
    atom: Section,
    common: Section,
    primitive: Section,
}

impl Keymap {
    pub fn keymap_read(
        &self,
        pending: &mut Vec<KeyStroke>,
        stroke: KeyStroke,
        cursor: Option<CursorKind>,
    ) -> KeyResolve {
        pending.push(stroke);
        let sections: Vec<&Section> = match cursor {
            Some(CursorKind::Atom) => vec![&self.common, &self.atom],
            Some(CursorKind::Array) => vec![&self.common, &self.array],
            Some(CursorKind::Primitive) => vec![&self.common, &self.primitive],
            None => vec![&self.common, &self.atom, &self.array, &self.primitive],
        };
        let mut prefix = false;
        for section in sections {
            for (chord, action) in section {
                if chord.as_slice() == pending.as_slice() {
                    pending.clear();
                    return KeyResolve::Action(*action);
                }
                if chord.len() > pending.len() && &chord[..pending.len()] == pending.as_slice() {
                    prefix = true;
                }
            }
        }
        if prefix {
            return KeyResolve::Pending;
        }
        pending.clear();
        return KeyResolve::Unbound;
    }

    pub fn keymap_resolve(spec: &SpecKeys) -> Result<Keymap, MultiError> {
        let mut errors = MultiError::default();
        let common =
            section_resolve(
                &spec.common,
                &[
                    (Action::Copy, &[&[ctrl(KeyName::Char('c'))], &[KeyStroke {
                        meta: true,
                        ..KeyStroke::key_stroke_new(KeyName::Char('c'))
                    }]]),
                    (Action::AiOpen, &[&[plain(KeyName::Char('a'))]]),
                    (Action::AiOpenReference, &[&[shift(KeyName::Char('a'))]]),
                ],
                &mut errors,
            );
        let atom =
            section_resolve(
                &spec.atom,
                &[
                    (Action::Exit, &[&[plain(KeyName::Surface)], &[plain(KeyName::Char('h'))]]),
                    (Action::Enter, &[&[plain(KeyName::Dive)], &[plain(KeyName::Char('l'))]]),
                    (Action::NextElement, &[&[plain(KeyName::Next)], &[plain(KeyName::Char('j'))]]),
                    (Action::PreviousElement, &[&[plain(KeyName::Previous)], &[plain(KeyName::Char('k'))]]),
                    (Action::Copy, &[&[plain(KeyName::Char('c'))]]),
                ],
                &mut errors,
            );
        let array =
            section_resolve(
                &spec.array,
                &[
                    (Action::Exit, &[&[plain(KeyName::Surface)], &[plain(KeyName::Char('h'))]]),
                    (Action::Enter, &[&[plain(KeyName::Dive)], &[plain(KeyName::Char('l'))]]),
                    (Action::NextElement, &[&[plain(KeyName::Next)], &[plain(KeyName::Char('j'))]]),
                    (Action::PreviousElement, &[&[plain(KeyName::Previous)], &[plain(KeyName::Char('k'))]]),
                    (Action::SelectNext, &[&[shift(KeyName::Next)], &[shift(KeyName::Char('j'))]]),
                    (Action::SelectPrevious, &[&[shift(KeyName::Previous)], &[shift(KeyName::Char('k'))]]),
                    (Action::FirstElement, &[&[plain(KeyName::Char('i'))]]),
                    (Action::LastElement, &[&[plain(KeyName::Char('u'))]]),
                    (Action::Copy, &[&[plain(KeyName::Char('c'))]]),
                ],
                &mut errors,
            );
        let primitive =
            section_resolve(
                &spec.primitive,
                &[
                    (Action::Exit, &[&[plain(KeyName::Escape)]]),
                    (Action::NextGlyph, &[&[plain(KeyName::Dive)]]),
                    (Action::PreviousGlyph, &[&[plain(KeyName::Surface)]]),
                    (Action::SelectNextGlyph, &[&[shift(KeyName::Dive)]]),
                    (Action::SelectPreviousGlyph, &[&[shift(KeyName::Surface)]]),
                    (Action::NextWord, &[&[ctrl(KeyName::Dive)]]),
                    (Action::PreviousWord, &[&[ctrl(KeyName::Surface)]]),
                    (Action::SelectNextWord, &[&[ctrl_shift(KeyName::Dive)]]),
                    (Action::SelectPreviousWord, &[&[ctrl_shift(KeyName::Surface)]]),
                    (Action::LineBegin, &[&[plain(KeyName::Home)]]),
                    (Action::LineEnd, &[&[plain(KeyName::End)]]),
                ],
                &mut errors,
            );
        for section in [&atom, &array, &primitive] {
            let searched = common.iter().chain(section.iter()).collect::<Vec<_>>();
            for (i, (chord, action)) in searched.iter().enumerate() {
                for (other, other_action) in &searched[i + 1..] {
                    let found = if other == chord {
                        ErrorKind::AmbiguousKeyBinding {
                            binding: serde_json::to_string(chord).unwrap(),
                            action: action.action_id().to_string(),
                            other: other_action.action_id().to_string(),
                        }
                    } else if other.starts_with(chord.as_slice()) || chord.starts_with(other.as_slice()) {
                        ErrorKind::ShadowedKeyBinding {
                            binding: serde_json::to_string(chord).unwrap(),
                            action: action.action_id().to_string(),
                            other_binding: serde_json::to_string(other).unwrap(),
                            other: other_action.action_id().to_string(),
                        }
                    } else {
                        continue;
                    };
                    if errors.0.iter().any(|e| e.kind == found) {
                        continue;
                    }
                    errors.multi_error_add("keys", found);
                }
            }
        }
        if !errors.multi_error_is_empty() {
            return Err(errors);
        }
        return Ok(Keymap {
            common: common,
            atom: atom,
            array: array,
            primitive: primitive,
        });
    }
}

impl Default for Keymap {
    fn default() -> Keymap {
        return Keymap::keymap_resolve(&SpecKeys::default()).expect("default key bindings are invalid");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyName {
    Accept,
    Add,
    Again,
    AllCandidates,
    Alphanumeric,
    Alt,
    AltGraph,
    AltLeft,
    AltRight,
    Backspace,
    Begin,
    BrowserBack,
    BrowserFavorites,
    BrowserForward,
    BrowserHome,
    BrowserRefresh,
    BrowserSearch,
    BrowserStop,
    Cancel,
    Caps,
    ChannelDown,
    ChannelUp,
    Char(char),
    Clear,
    CodeInput,
    ColoredKey0,
    ColoredKey1,
    ColoredKey2,
    ColoredKey3,
    Command,
    Compose,
    ContextMenu,
    Control,
    ControlLeft,
    ControlRight,
    Convert,
    Copy,
    Cut,
    DeadAbovedot,
    DeadAbovering,
    DeadAcute,
    DeadBreve,
    DeadCaron,
    DeadCedilla,
    DeadCircumflex,
    DeadDiaeresis,
    DeadDoubleacute,
    DeadGrave,
    DeadIota,
    DeadMacron,
    DeadOgonek,
    DeadSemivoicedSound,
    DeadTilde,
    DeadVoicedSound,
    Decimal,
    Delete,
    Dive,
    Divide,
    EjectToggle,
    End,
    Enter,
    Escape,
    FastFwd,
    Final,
    Find,
    FullWidth,
    Function(u8),
    GameA,
    GameB,
    GameC,
    GameD,
    HalfWidth,
    Help,
    Hiragana,
    Home,
    Info,
    InputMethodOnOff,
    Insert,
    IntlBackSlash,
    IntlHangulMode,
    IntlHanja,
    IntlRo,
    IntlYen,
    JapaneseHiragana,
    JapaneseKatakana,
    JapaneseRoman,
    Kana,
    KanaLock,
    Kanji,
    Katakana,
    KpDown,
    KpLeft,
    KpRight,
    KpUp,
    Lang1,
    Lang2,
    LaunchApp1,
    LaunchApp2,
    LaunchMail,
    LaunchMediaPlayer,
    MediaNext,
    MediaPlayPause,
    MediaPrevious,
    MediaStop,
    Meta,
    MetaLeft,
    MetaRight,
    Modechange,
    Mouse(u8),
    MouseScrollIn,
    MouseScrollLeft,
    MouseScrollOut,
    MouseScrollRight,
    Multiply,
    Mute,
    Next,
    Nonconvert,
    NumLock,
    Numpad(u8),
    NumpadAdd,
    NumpadChangesign,
    NumpadComma,
    NumpadDecimal,
    NumpadDivide,
    NumpadEnter,
    NumpadEqual,
    NumpadLeftparen,
    NumpadMultiply,
    NumpadRightparen,
    NumpadSubtract,
    Open,
    OsLeft,
    OsRight,
    PageDown,
    PageUp,
    Paste,
    Pause,
    Play,
    Pound,
    Power,
    Previous,
    PreviousCandidate,
    Printscreen,
    Props,
    Record,
    Rewind,
    RomanCharacters,
    ScrollLock,
    Select,
    Separator,
    Shift,
    ShiftLeft,
    ShiftRight,
    Shortcut,
    Softkey(u8),
    Space,
    Star,
    Stop,
    Subtract,
    Surface,
    Tab,
    TrackNext,
    TrackPrev,
    Undefined,
    Undo,
    VolumeDown,
    VolumeUp,
    Wake,
    Windows,
}

impl From<DirectionKey> for KeyName {
    fn from(d: DirectionKey) -> KeyName {
        match d {
            DirectionKey::Dive => return KeyName::Dive,
            DirectionKey::Surface => return KeyName::Surface,
            DirectionKey::Next => return KeyName::Next,
            DirectionKey::Previous => return KeyName::Previous,
        }
    }
}

impl Serialize for KeyName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            KeyName::Char(c) => return serializer.serialize_str(&c.to_string()),
            KeyName::Function(n) => return serializer.serialize_str(&format!("f{}", n)),
            KeyName::Numpad(n) => return serializer.serialize_str(&format!("numpad{}", n)),
            KeyName::Mouse(n) => return serializer.serialize_str(&format!("mouse_{}", n)),
            KeyName::Softkey(n) => return serializer.serialize_str(&format!("softkey_{}", n)),
            other => return serializer.serialize_str(NAMED_KEYS.iter().find(|(_, k)| k == other).unwrap().0),
        }
    }
}

impl<'de> Deserialize<'de> for KeyName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<KeyName, D::Error> {
        let text = String::deserialize(deserializer)?;
        if let Some((_, named)) = NAMED_KEYS.iter().find(|(n, _)| *n == text) {
            return Ok(*named);
        }
        let char_keys: [(&str, char); 30] =
            [
                ("comma", ','),
                ("minus", '-'),
                ("period", '.'),
                ("slash", '/'),
                ("semicolon", ';'),
                ("equals", '='),
                ("open_bracket", '['),
                ("back_slash", '\\'),
                ("close_bracket", ']'),
                ("back_quote", '`'),
                ("quote", '\''),
                ("ampersand", '&'),
                ("asterisk", '*'),
                ("quotedbl", '"'),
                ("less", '<'),
                ("greater", '>'),
                ("braceleft", '{'),
                ("braceright", '}'),
                ("at", '@'),
                ("colon", ':'),
                ("circumflex", '^'),
                ("dollar", '$'),
                ("euro_sign", '€'),
                ("exclamation_mark", '!'),
                ("inverted_exclamation_mark", '¡'),
                ("left_parenthesis", '('),
                ("number_sign", '#'),
                ("plus", '+'),
                ("right_parenthesis", ')'),
                ("underscore", '_'),
            ];
        if let Some((_, c)) = char_keys.iter().find(|(n, _)| *n == text) {
            return Ok(KeyName::Char(*c));
        }
        let counted_keys: [(&str, u8, u8, fn(u8) -> KeyName); 4] =
            [
                ("f", 1, 24, KeyName::Function),
                ("numpad", 0, 9, KeyName::Numpad),
                ("mouse_", 1, 5, KeyName::Mouse),
                ("softkey_", 0, 9, KeyName::Softkey),
            ];
        for (prefix, low, high, build) in counted_keys {
            let Some(number) = text.strip_prefix(prefix) else {
                continue;
            };
            let Ok(n) = number.parse::<u8>() else {
                continue;
            };
            if n < low || n > high {
                continue;
            }
            return Ok(build(n));
        }
        let mut chars = text.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return Err(
                D::Error::custom(
                    format!(
                        "unknown key `{}`; use a single character or one of {}",
                        text,
                        NAMED_KEYS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                    ),
                ),
            );
        };
        return Ok(KeyName::Char(c.to_lowercase().next().unwrap_or(c)));
    }
}

pub enum KeyResolve {
    Action(Action),
    Pending,
    Unbound,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeyStroke {
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ctrl: bool,
    pub key: KeyName,
    #[serde(default, skip_serializing_if = "is_false")]
    pub meta: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
}

impl KeyStroke {
    pub const fn key_stroke_new(key: KeyName) -> KeyStroke {
        return KeyStroke {
            key: key,
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
        };
    }
}

const fn plain(key: KeyName) -> KeyStroke {
    return KeyStroke::key_stroke_new(key);
}

type Section = Vec<(Vec<KeyStroke>, Action)>;

fn section_resolve(spec: &SpecSection, defaults: &[(Action, &[&[KeyStroke]])], errors: &mut MultiError) -> Section {
    let mut out = vec![];
    for (action, chords) in defaults {
        match spec.get(action.action_id()) {
            Some(configured) => {
                for binding in configured {
                    let chord = match binding {
                        SpecBinding::Stroke(stroke) => vec![*stroke],
                        SpecBinding::Chord(strokes) => strokes.clone(),
                    };
                    if chord.is_empty() {
                        errors.multi_error_add(
                            "keys",
                            ErrorKind::EmptyKeyBinding { action: action.action_id().to_string() },
                        );
                        continue;
                    }
                    out.push((chord, *action));
                }
            },
            None => {
                for chord in *chords {
                    out.push((chord.to_vec(), *action));
                }
            },
        }
    }
    for (id, configured) in spec {
        if defaults.iter().any(|(action, _)| action.action_id() == id) {
            continue;
        }
        let Some((action, _)) = ACTIONS.iter().find(|(_, known)| known == id) else {
            errors.multi_error_add("keys", ErrorKind::UnknownAction {
                action: id.clone(),
                known: ACTIONS.iter().map(|(_, known)| known.to_string()).collect(),
            });
            continue;
        };
        for binding in configured {
            let chord = match binding {
                SpecBinding::Stroke(stroke) => vec![*stroke],
                SpecBinding::Chord(strokes) => strokes.clone(),
            };
            if chord.is_empty() {
                errors.multi_error_add("keys", ErrorKind::EmptyKeyBinding { action: id.clone() });
                continue;
            }
            out.push((chord, *action));
        }
    }
    return out;
}

const fn shift(key: KeyName) -> KeyStroke {
    return KeyStroke {
        shift: true,
        ..KeyStroke::key_stroke_new(key)
    };
}

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum SpecBinding {
    Chord(Vec<KeyStroke>),
    Stroke(KeyStroke),
}

impl<'de> Deserialize<'de> for SpecBinding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<SpecBinding, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.is_array() {
            return Ok(SpecBinding::Chord(serde_json::from_value(value).map_err(D::Error::custom)?));
        }
        return Ok(SpecBinding::Stroke(serde_json::from_value(value).map_err(D::Error::custom)?));
    }
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SpecKeys {
    #[serde(default)]
    pub array: SpecSection,
    #[serde(default)]
    pub atom: SpecSection,
    #[serde(default)]
    pub common: SpecSection,
    #[serde(default)]
    pub primitive: SpecSection,
}

pub type SpecSection = HashMap<String, Vec<SpecBinding>>;
