use {
    crate::{
        ai::Ai,
        panels::{
            Panel,
            PanelChange,
            PanelResult,
            list::{
                List,
                ListRow,
            },
        },
    },
    merman_core::keys::{
        Action,
        Keymap,
    },
    rooting::El,
    std::{
        cell::RefCell,
        rc::Weak,
    },
    web_sys::{
        KeyboardEvent,
        MouseEvent,
    },
};

const MODELS: [(&str, &str); 4] = [("", "Default"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")];

pub struct ModelsPanel {
    ai: Weak<Ai>,
    list: RefCell<List>,
}

impl ModelsPanel {
    pub fn models_new(keys: Keymap, ai: Weak<Ai>) -> ModelsPanel {
        let rows = MODELS.iter().map(|(_, name)| ListRow {
            icon: None,
            spans: vec![],
            text: name.to_string(),
        }).collect();
        return ModelsPanel {
            ai: ai,
            list: RefCell::new(List::list_new(keys, "merman_panel_models", "", rows, Some(0))),
        };
    }

    fn models_choose(&self) -> PanelResult {
        let Some(selected) = self.list.borrow().selected else {
            return PanelResult::Unused(Action::Enter);
        };
        if let Some(ai) = self.ai.upgrade() {
            ai.ai_send(format!("/model {}", MODELS[selected].0).trim_end().to_string());
        }
        return PanelResult::Unused(Action::Exit);
    }
}

impl Panel for ModelsPanel {
    fn panel_attach(&self) -> El {
        return self.list.borrow_mut().list_attach();
    }

    fn panel_changed(&self, _path: &str) -> Option<PanelChange> {
        return None;
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_detach(&self) {
        self.list.borrow_mut().list_detach();
        return;
    }

    fn panel_focusable(&self) -> bool {
        return true;
    }

    fn panel_focused(&self, focused: bool) {
        self.list.borrow_mut().list_focused(focused);
        return;
    }

    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult {
        let result = self.list.borrow_mut().list_key(e);
        match result {
            PanelResult::Unused(Action::Enter) => return self.models_choose(),
            result => return result,
        }
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        let result = self.list.borrow_mut().list_mouse(e);
        match result {
            PanelResult::Selected | PanelResult::Used => return self.models_choose(),
            result => return result,
        }
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

    fn panel_select(&self, _location: &str) { }

    fn panel_selection(&self) -> Option<(bool, String)> {
        return None;
    }

    fn panel_size(&self) -> f64 {
        return 8.;
    }
}
