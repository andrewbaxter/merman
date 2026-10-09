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
    merman_core::keys::{
        Action,
        Keymap,
    },
    rooting::El,
    std::cell::RefCell,
    web_sys::{
        KeyboardEvent,
        MouseEvent,
    },
};

pub struct ConflictPanel {
    list: RefCell<List>,
    resolve: Box<dyn Fn(bool)>,
}

impl ConflictPanel {
    pub fn conflict_new(keys: Keymap, resolve: Box<dyn Fn(bool)>) -> ConflictPanel {
        return ConflictPanel {
            list: RefCell::new(List::list_new(keys, "merman_panel_conflict", "", vec![ListRow {
                spans: vec![],
                icon: Some("\u{e161}"),
                text: "Keep my changes (overwrite the file)".to_string(),
            }, ListRow {
                spans: vec![],
                icon: Some("\u{e5d5}"),
                text: "Discard my changes (reload the file)".to_string(),
            }], Some(0))),
            resolve: resolve,
        };
    }
}

impl Panel for ConflictPanel {
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
            PanelResult::Unused(Action::Enter) => {
                let Some(selected) = self.list.borrow().selected else {
                    return PanelResult::Used;
                };
                (self.resolve)(selected == 0);
                return PanelResult::Unused(Action::Exit);
            },
            result => return result,
        }
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        return self.list.borrow_mut().list_mouse(e);
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
