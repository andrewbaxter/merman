use {
    crate::panels::{
        Panel,
        PanelChange,
        PanelResult,
        panel_path_parent,
    },
    rooting::{
        El,
        el,
    },
    web_sys::{
        KeyboardEvent,
        MouseEvent,
    },
};

pub struct ErrorPanel {
    dir: bool,
    message: String,
    path: String,
}

impl ErrorPanel {
    pub fn error_new(path: String, dir: bool, message: &str) -> ErrorPanel {
        return ErrorPanel {
            path: path,
            dir: dir,
            message: message.to_string(),
        };
    }
}

impl Panel for ErrorPanel {
    fn panel_attach(&self) -> El {
        return el("pre").classes(&["merman_panel", "merman_error"]).text(&self.message);
    }

    fn panel_changed(&self, path: &str) -> Option<PanelChange> {
        if path == self.path || (self.dir && panel_path_parent(path) == Some(self.path.as_str())) {
            return Some(PanelChange::Reload(self.dir));
        }
        return None;
    }

    fn panel_cursor_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_detach(&self) { }

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
        return self.path.clone();
    }

    fn panel_reference(&self) -> Option<String> {
        return None;
    }

    fn panel_selection(&self) -> Option<(bool, String)> {
        return None;
    }

    fn panel_size(&self) -> f64 {
        return 5.;
    }
}
