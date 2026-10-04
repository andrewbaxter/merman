use {
    crate::panels::{
        Panel,
        PanelChange,
        PanelResult,
        list::{
            List,
            ListRow,
        },
    },
    merman_core::{
        gap::GapChoices,
        keys::Keymap,
    },
    merman_core::syntax::Syntax,
    rooting::El,
    std::{
        cell::RefCell,
        rc::Rc,
    },
    web_sys::{
        KeyboardEvent,
        MouseEvent,
    },
};

pub struct ChoicesPanel(RefCell<List>, Rc<Syntax>);

impl ChoicesPanel {
    pub fn choices_new(keys: Keymap, syntax: Rc<Syntax>) -> ChoicesPanel {
        return ChoicesPanel(
            RefCell::new(List::list_new(keys, "merman_panel_choices", "Nothing matches", vec![], None)),
            syntax,
        );
    }

    pub fn choices_set(&self, choices: &GapChoices) {
        let mut list = self.0.borrow_mut();
        list.rows = choices.choices.iter().map(|c| ListRow {
            icon: None,
            spans: c.preview.iter().map(|(text, style)| {
                let style = self.1.syntax_style(style.unwrap_or(0));
                return (text.clone(), style.color.clone(), style.font.family.clone());
            }).collect(),
            text: if c.preview.is_empty() {
                c.name.clone()
            } else {
                format!(" \u{2014} {}", c.name)
            },
        }).collect();
        list.selected = (!list.rows.is_empty()).then_some(choices.index);
        list.list_draw();
        return;
    }
}

impl Panel for ChoicesPanel {
    fn panel_attach(&self) -> El {
        return self.0.borrow_mut().list_attach();
    }

    fn panel_changed(&self, _path: &str) -> Option<PanelChange> {
        return None;
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_detach(&self) {
        self.0.borrow_mut().list_detach();
        return;
    }

    fn panel_focusable(&self) -> bool {
        return false;
    }

    fn panel_focused(&self, _focused: bool) { }

    fn panel_key(&self, _e: &KeyboardEvent) -> PanelResult {
        return PanelResult::Ignored;
    }

    fn panel_mouse(&self, _e: &MouseEvent) -> PanelResult {
        return PanelResult::Ignored;
    }

    fn panel_parent(&self) -> Option<String> {
        return None;
    }

    fn panel_path(&self) -> String {
        return String::new();
    }

    fn panel_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_selection(&self) -> Option<(bool, String)> {
        return None;
    }

    fn panel_size(&self) -> f64 {
        return 2.;
    }
}
