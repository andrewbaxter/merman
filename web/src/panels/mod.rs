pub mod code;
pub mod error;
pub mod filesystem;
pub mod list;
pub mod sessions;

use {
    merman_core::{
        direction::DirectionConvert,
        keys::{
            Action,
            KeyName,
            KeyStroke,
        },
        spec::{
            SpecDirection,
            SpecTheme,
        },
    },
    gloo_utils::document,
    rooting::El,
    std::rc::Rc,
    wasm_bindgen::JsCast,
    web_sys::{
        HtmlElement,
        KeyboardEvent,
        MouseEvent,
    },
};

pub trait Panel {
    fn panel_attach(&self) -> El;
    fn panel_changed(&self, path: &str) -> Option<bool>;
    fn panel_cursor_reference(&self) -> Option<String>;
    fn panel_detach(&self);
    fn panel_focusable(&self) -> bool;
    fn panel_focused(&self, focused: bool);
    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult;
    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult;
    fn panel_parent(&self) -> Option<String>;
    fn panel_path(&self) -> String;
    fn panel_reference(&self) -> Option<String>;
    fn panel_selection(&self) -> Option<(bool, String)>;
    fn panel_size(&self) -> f64;
}

pub fn panel_key_stroke(e: &KeyboardEvent, convert: DirectionConvert) -> Option<KeyStroke> {
    let key = e.key();
    let stroke = |name: KeyName| -> Option<KeyStroke> {
        return Some(KeyStroke {
            ctrl: e.ctrl_key(),
            alt: e.alt_key(),
            shift: e.shift_key(),
            meta: e.meta_key(),
            key: name,
        });
    };
    let name = match key.as_str() {
        "ArrowUp" => convert.direction_convert_cardinal(SpecDirection::Up).into(),
        "ArrowDown" => convert.direction_convert_cardinal(SpecDirection::Down).into(),
        "ArrowLeft" => convert.direction_convert_cardinal(SpecDirection::Left).into(),
        "ArrowRight" => convert.direction_convert_cardinal(SpecDirection::Right).into(),
        other => {
            let named: [(&str, KeyName); 74] =
                [
                    ("Enter", KeyName::Enter),
                    ("Escape", KeyName::Escape),
                    (" ", KeyName::Space),
                    ("Tab", KeyName::Tab),
                    ("Backspace", KeyName::Backspace),
                    ("Delete", KeyName::Delete),
                    ("Home", KeyName::Home),
                    ("End", KeyName::End),
                    ("PageUp", KeyName::PageUp),
                    ("PageDown", KeyName::PageDown),
                    ("Insert", KeyName::Insert),
                    ("Clear", KeyName::Clear),
                    ("Cancel", KeyName::Cancel),
                    ("Pause", KeyName::Pause),
                    ("CapsLock", KeyName::Caps),
                    ("NumLock", KeyName::NumLock),
                    ("ScrollLock", KeyName::ScrollLock),
                    ("PrintScreen", KeyName::Printscreen),
                    ("Help", KeyName::Help),
                    ("ContextMenu", KeyName::ContextMenu),
                    ("AltGraph", KeyName::AltGraph),
                    ("Meta", KeyName::Meta),
                    ("Select", KeyName::Select),
                    ("Open", KeyName::Open),
                    ("Cut", KeyName::Cut),
                    ("Copy", KeyName::Copy),
                    ("Paste", KeyName::Paste),
                    ("Undo", KeyName::Undo),
                    ("Again", KeyName::Again),
                    ("Find", KeyName::Find),
                    ("Props", KeyName::Props),
                    ("Compose", KeyName::Compose),
                    ("MediaPlayPause", KeyName::MediaPlayPause),
                    ("MediaStop", KeyName::MediaStop),
                    ("MediaTrackNext", KeyName::MediaNext),
                    ("MediaTrackPrevious", KeyName::MediaPrevious),
                    ("MediaPlay", KeyName::Play),
                    ("MediaRecord", KeyName::Record),
                    ("MediaFastForward", KeyName::FastFwd),
                    ("MediaRewind", KeyName::Rewind),
                    ("AudioVolumeUp", KeyName::VolumeUp),
                    ("AudioVolumeDown", KeyName::VolumeDown),
                    ("AudioVolumeMute", KeyName::Mute),
                    ("ChannelUp", KeyName::ChannelUp),
                    ("ChannelDown", KeyName::ChannelDown),
                    ("Eject", KeyName::EjectToggle),
                    ("Power", KeyName::Power),
                    ("Info", KeyName::Info),
                    ("WakeUp", KeyName::Wake),
                    ("BrowserBack", KeyName::BrowserBack),
                    ("BrowserForward", KeyName::BrowserForward),
                    ("BrowserHome", KeyName::BrowserHome),
                    ("BrowserFavorites", KeyName::BrowserFavorites),
                    ("BrowserRefresh", KeyName::BrowserRefresh),
                    ("BrowserSearch", KeyName::BrowserSearch),
                    ("BrowserStop", KeyName::BrowserStop),
                    ("LaunchMediaPlayer", KeyName::LaunchMediaPlayer),
                    ("LaunchMail", KeyName::LaunchMail),
                    ("LaunchApplication1", KeyName::LaunchApp1),
                    ("LaunchApplication2", KeyName::LaunchApp2),
                    ("Convert", KeyName::Convert),
                    ("NonConvert", KeyName::Nonconvert),
                    ("Accept", KeyName::Accept),
                    ("ModeChange", KeyName::Modechange),
                    ("KanaMode", KeyName::Kana),
                    ("KanjiMode", KeyName::Kanji),
                    ("HangulMode", KeyName::IntlHangulMode),
                    ("HanjaMode", KeyName::IntlHanja),
                    ("Alphanumeric", KeyName::Alphanumeric),
                    ("Katakana", KeyName::Katakana),
                    ("Hiragana", KeyName::Hiragana),
                    ("ZenkakuHankaku", KeyName::FullWidth),
                    ("CodeInput", KeyName::CodeInput),
                    ("AllCandidates", KeyName::AllCandidates),
                ];
            if let Some((_, name)) = named.iter().find(|(browser, _)| *browser == other) {
                return stroke(*name);
            }
            if let Some(number) = other.strip_prefix('F') {
                if let Ok(n) = number.parse::<u8>() {
                    if n >= 1 && n <= 24 {
                        return stroke(KeyName::Function(n));
                    }
                }
            }
            if let Some(number) = e.code().strip_prefix("Numpad") {
                if let Ok(n) = number.parse::<u8>() {
                    if n <= 9 {
                        return stroke(KeyName::Numpad(n));
                    }
                }
            }
            let mut chars = other.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            KeyName::Char(c.to_lowercase().next().unwrap_or(c))
        },
    };
    return stroke(name);
}

pub fn panel_theme_apply(theme: &SpecTheme) {
    let style = document().document_element().unwrap().dyn_into::<HtmlElement>().unwrap().style();
    for (
        name,
        value,
    ) in [
        ("--merman-background", theme.background.clone()),
        ("--merman-cursor-color", theme.cursor.line_color.clone()),
        ("--merman-error-color", theme.error_color.clone()),
        ("--merman-font-family", theme.font_family.clone()),
        ("--merman-font-size", format!("{}mm", theme.font_size)),
        ("--merman-hover-color", theme.hover.line_color.clone()),
        ("--merman-icon-color", theme.icon_color.clone()),
        ("--merman-line-thickness", format!("{}mm", theme.cursor.line_thickness)),
        ("--merman-text-color", theme.text_color.clone()),
    ] {
        style.set_property(name, &value).unwrap();
    }
    return;
}

pub fn panel_path_parent(path: &str) -> Option<&str> {
    let (parent, _) = path.rsplit_once('/')?;
    if parent.is_empty() {
        return Some("/");
    }
    return Some(parent);
}

pub enum PanelResult {
    Ignored,
    Open(Rc<dyn Panel>),
    Selected,
    Unused(Action),
    Used,
}
