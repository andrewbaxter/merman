use {
    crate::panels::{
        Panel,
        PanelChange,
        PanelResult,
        list::{
            List,
            ListRow,
        },
        panel_path_parent,
    },
    gloo_utils::window,
    merman_api::{
        ListEntry,
        RespList,
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

pub struct FilesystemPanel(RefCell<State>);

impl FilesystemPanel {
    pub fn filesystem_new(keys: Keymap, listing: RespList, select: Option<String>) -> FilesystemPanel {
        let selected = select.and_then(|path| listing.entries.iter().position(|e| e.path == path));
        let rows = listing.entries.iter().map(|entry| ListRow {
            spans: vec![],
            icon: Some(if entry.dir {
                "\u{e2c7}"
            } else {
                ""
            }),
            text: entry.name.clone(),
        }).collect();
        let state = State {
            list: List::list_new(keys, "merman_panel_filesystem", "empty", rows, selected),
            dir: listing.dir,
            parent: listing.parent,
            entries: listing.entries,
        };
        return FilesystemPanel(RefCell::new(state));
    }
}

impl Panel for FilesystemPanel {
    fn panel_attach(&self) -> El {
        return self.0.borrow_mut().list.list_attach();
    }

    fn panel_changed(&self, path: &str) -> Option<PanelChange> {
        let s = self.0.borrow();
        if path == s.dir || panel_path_parent(path) == Some(s.dir.as_str()) {
            return Some(PanelChange::Reload(true));
        }
        return None;
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_detach(&self) {
        self.0.borrow_mut().list.list_detach();
        return;
    }

    fn panel_editable(&self, _editable: bool) { }

    fn panel_lang_errors(&self, _path: &str, _errors: &[merman_langserver::CompileError]) { }

    fn panel_focusable(&self) -> bool {
        return true;
    }

    fn panel_focused(&self, focused: bool) {
        self.0.borrow_mut().list.list_focused(focused);
        return;
    }

    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult {
        let mut s = self.0.borrow_mut();
        match s.list.list_key(e) {
            PanelResult::Unused(Action::Copy) => {
                let Some(selected) = s.list.selected else {
                    return PanelResult::Unused(Action::Copy);
                };
                let _ = window().navigator().clipboard().write_text(&s.entries[selected].path);
                return PanelResult::Used;
            },
            result => return result,
        }
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        return self.0.borrow_mut().list.list_mouse(e);
    }

    fn panel_parent(&self) -> Option<String> {
        return self.0.borrow().parent.clone();
    }

    fn panel_path(&self) -> String {
        return self.0.borrow().dir.clone();
    }

    fn panel_reference(&self) -> Option<String> {
        return self.panel_selection().map(|(_, path)| path);
    }

    fn panel_select(&self, location: &str) {
        let mut s = self.0.borrow_mut();
        let Some(index) = s.entries.iter().position(|e| e.path == location) else {
            return;
        };
        s.list.list_select(index);
        return;
    }

    fn panel_selection(&self) -> Option<(bool, String)> {
        let s = self.0.borrow();
        return s.list.selected.map(|i| (s.entries[i].dir, s.entries[i].path.clone()));
    }

    fn panel_size(&self) -> f64 {
        return 8.;
    }
}

struct State {
    dir: String,
    entries: Vec<ListEntry>,
    list: List,
    parent: Option<String>,
}
