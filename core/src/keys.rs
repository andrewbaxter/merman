use crate::direction::DirectionKey;
use serde::de::Error as _;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Enter,
    Exit,
    Escape,
    Next,
    Previous,
    NextWord,
    PreviousWord,
    First,
    Last,
    SelectNext,
    SelectPrevious,
    Copy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyName {
    Char(char),
    Dive,
    Surface,
    Next,
    Previous,
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
}

const NAMED_KEYS: [(&str, KeyName); 14] =
    [
        ("dive", KeyName::Dive),
        ("surface", KeyName::Surface),
        ("next", KeyName::Next),
        ("previous", KeyName::Previous),
        ("enter", KeyName::Enter),
        ("escape", KeyName::Escape),
        ("space", KeyName::Space),
        ("tab", KeyName::Tab),
        ("backspace", KeyName::Backspace),
        ("delete", KeyName::Delete),
        ("home", KeyName::Home),
        ("end", KeyName::End),
        ("page_up", KeyName::PageUp),
        ("page_down", KeyName::PageDown),
    ];

impl Serialize for KeyName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            KeyName::Char(c) => return serializer.serialize_str(&c.to_string()),
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

fn is_false(v: &bool) -> bool {
    return !*v;
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeyStroke {
    pub key: KeyName,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ctrl: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub meta: bool,
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

    pub const fn key_stroke_ctrl(key: KeyName) -> KeyStroke {
        return KeyStroke {
            ctrl: true,
            ..KeyStroke::key_stroke_new(key)
        };
    }

    pub const fn key_stroke_shift(key: KeyName) -> KeyStroke {
        return KeyStroke {
            shift: true,
            ..KeyStroke::key_stroke_new(key)
        };
    }

    pub const fn key_stroke_meta(key: KeyName) -> KeyStroke {
        return KeyStroke {
            meta: true,
            ..KeyStroke::key_stroke_new(key)
        };
    }
}

const fn char_stroke(c: char) -> KeyStroke {
    return KeyStroke::key_stroke_new(KeyName::Char(c));
}

pub const ACTIONS: [(Action, &str, &[&[KeyStroke]]); 12] =
    [
        (Action::Enter, "enter", &[&[KeyStroke::key_stroke_new(KeyName::Dive)], &[char_stroke('l')]]),
        (Action::Exit, "exit", &[&[KeyStroke::key_stroke_new(KeyName::Surface)], &[char_stroke('h')]]),
        (Action::Escape, "escape", &[&[KeyStroke::key_stroke_new(KeyName::Escape)]]),
        (Action::Next, "next", &[&[KeyStroke::key_stroke_new(KeyName::Next)], &[char_stroke('j')]]),
        (Action::Previous, "previous", &[&[KeyStroke::key_stroke_new(KeyName::Previous)], &[char_stroke('k')]]),
        (Action::NextWord, "next_word", &[&[KeyStroke::key_stroke_ctrl(KeyName::Dive)], &[char_stroke('w')]]),
        (
            Action::PreviousWord,
            "previous_word",
            &[&[KeyStroke::key_stroke_ctrl(KeyName::Surface)], &[char_stroke('b')]],
        ),
        (Action::First, "first", &[&[KeyStroke::key_stroke_new(KeyName::Home)], &[char_stroke('i')]]),
        (Action::Last, "last", &[&[KeyStroke::key_stroke_new(KeyName::End)], &[char_stroke('u')]]),
        (
            Action::SelectNext,
            "select_next",
            &[
                &[KeyStroke::key_stroke_shift(KeyName::Next)],
                &[KeyStroke::key_stroke_shift(KeyName::Dive)],
                &[KeyStroke::key_stroke_shift(KeyName::Char('j'))],
                &[KeyStroke::key_stroke_shift(KeyName::Char('l'))],
            ],
        ),
        (
            Action::SelectPrevious,
            "select_previous",
            &[
                &[KeyStroke::key_stroke_shift(KeyName::Previous)],
                &[KeyStroke::key_stroke_shift(KeyName::Surface)],
                &[KeyStroke::key_stroke_shift(KeyName::Char('k'))],
                &[KeyStroke::key_stroke_shift(KeyName::Char('h'))],
            ],
        ),
        (
            Action::Copy,
            "copy",
            &[
                &[KeyStroke::key_stroke_ctrl(KeyName::Char('c'))],
                &[KeyStroke::key_stroke_meta(KeyName::Char('c'))],
                &[char_stroke('y')],
            ],
        ),
    ];

impl Action {
    pub fn action_id(self) -> &'static str {
        return ACTIONS.iter().find(|(a, _, _)| *a == self).unwrap().1;
    }
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

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum SpecBinding {
    Stroke(KeyStroke),
    Chord(Vec<KeyStroke>),
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
#[serde(transparent)]
pub struct SpecKeys(pub HashMap<String, Vec<SpecBinding>>);

#[derive(Clone)]
pub struct Keymap {
    bindings: Vec<(Vec<KeyStroke>, Action)>,
}

pub enum KeyMatch {
    None,
    Prefix,
    Action(Action),
}

impl Keymap {
    pub fn keymap_resolve(spec: &SpecKeys) -> Result<Keymap, Vec<String>> {
        let mut errors = vec![];
        let mut bindings = vec![];
        for (action, id, defaults) in ACTIONS {
            match spec.0.get(id) {
                Some(configured) => {
                    for binding in configured {
                        let chord = match binding {
                            SpecBinding::Stroke(stroke) => vec![*stroke],
                            SpecBinding::Chord(strokes) => strokes.clone(),
                        };
                        if chord.is_empty() {
                            errors.push(format!("Empty key binding for `{}`", id));
                            continue;
                        }
                        bindings.push((chord, action));
                    }
                },
                None => {
                    for chord in defaults {
                        bindings.push((chord.to_vec(), action));
                    }
                },
            }
        }
        for id in spec.0.keys() {
            if ACTIONS.iter().any(|(_, known, _)| known == id) {
                continue;
            }
            errors.push(
                format!(
                    "Unknown action `{}` in keys; known actions are {}",
                    id,
                    ACTIONS.iter().map(|(_, known, _)| *known).collect::<Vec<_>>().join(", ")
                ),
            );
        }
        for (i, (chord, action)) in bindings.iter().enumerate() {
            for (other, other_action) in &bindings[i + 1..] {
                if other == chord {
                    errors.push(
                        format!(
                            "Key binding {} is bound to both `{}` and `{}`",
                            serde_json::to_string(chord).unwrap(),
                            action.action_id(),
                            other_action.action_id()
                        ),
                    );
                } else if other.starts_with(chord.as_slice()) || chord.starts_with(other.as_slice()) {
                    errors.push(
                        format!(
                            "Key binding {} (`{}`) is the start of {} (`{}`), so one of them can never fire",
                            serde_json::to_string(chord).unwrap(),
                            action.action_id(),
                            serde_json::to_string(other).unwrap(),
                            other_action.action_id()
                        ),
                    );
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        return Ok(Keymap { bindings: bindings });
    }

    pub fn keymap_match(&self, sequence: &[KeyStroke]) -> KeyMatch {
        let mut prefix = false;
        for (chord, action) in &self.bindings {
            if chord.as_slice() == sequence {
                return KeyMatch::Action(*action);
            }
            if chord.len() > sequence.len() && &chord[..sequence.len()] == sequence {
                prefix = true;
            }
        }
        if prefix {
            return KeyMatch::Prefix;
        }
        return KeyMatch::None;
    }
}

impl Default for Keymap {
    fn default() -> Keymap {
        return Keymap::keymap_resolve(&SpecKeys::default()).expect("default key bindings are invalid");
    }
}
