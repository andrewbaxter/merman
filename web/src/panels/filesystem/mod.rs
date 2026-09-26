use crate::panels::{
    panel_key_stroke,
    panel_path_parent,
    Panel,
    PanelResult,
};
use gloo_utils::window;
use merman3_api::{
    ListEntry,
    RespList,
};
use merman3_core::direction::DirectionConvert;
use merman3_core::cursor::CursorKind;
use merman3_core::keys::{
    Action,
    KeyResolve,
    KeyStroke,
    Keymap,
};
use merman3_core::spec::SpecDirection;
use rooting::{
    el,
    El,
};
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use web_sys::{
    Element,
    HtmlElement,
    KeyboardEvent,
    MouseEvent,
};

struct State {
    keys: Keymap,
    pending: Vec<KeyStroke>,
    dir: String,
    parent: Option<String>,
    entries: Vec<ListEntry>,
    selected: Option<usize>,
    remembered: Option<usize>,
    attached: Option<(El, El)>,
}

fn draw(s: &State) {
    let Some((panel, rows)) = s.attached.as_ref() else {
        return;
    };
    if s.entries.is_empty() {
        rows.ref_clear();
        rows.ref_push(el("div").classes(&["merman_row_empty"]).text("empty"));
        return;
    }
    let new_rows = s.entries.iter().enumerate().map(|(i, entry)| {
        let row = el("div").classes(&["merman_row"]);
        if s.selected == Some(i) {
            row.ref_classes(&["merman_row_select"]);
        }
        row.ref_push(el("span").classes(&["merman_icon"]).text(if entry.dir {
            "\u{e2c7}"
        } else {
            ""
        }));
        row.ref_push(el("span").classes(&["merman_name"]).text(&entry.name));
        return row;
    }).collect::<Vec<_>>();
    rows.ref_clear();
    rows.ref_extend(new_rows);
    let panel = panel.raw();
    let Some(row) = panel.query_selector(".merman_row_select").ok().flatten() else {
        return;
    };
    let row: HtmlElement = row.dyn_into().unwrap();
    let top = row.offset_top() as f64;
    let bottom = top + row.offset_height() as f64;
    let view_top = panel.scroll_top() as f64;
    let view_height = panel.client_height() as f64;
    if top < view_top {
        panel.scroll_to_with_x_and_y(0., top);
    } else if bottom > view_top + view_height {
        panel.scroll_to_with_x_and_y(0., bottom - view_height);
    }
}

pub struct FilesystemPanel(RefCell<State>);

impl FilesystemPanel {
    pub fn filesystem_new(keys: Keymap, listing: RespList, select: Option<String>) -> FilesystemPanel {
        let selected = select.and_then(|path| listing.entries.iter().position(|e| e.path == path));
        let state = State {
            keys: keys,
            pending: vec![],
            dir: listing.dir,
            parent: listing.parent,
            entries: listing.entries,
            selected: selected,
            remembered: selected,
            attached: None,
        };
        return FilesystemPanel(RefCell::new(state));
    }

    fn filesystem_select(&self, index: usize) -> PanelResult {
        let mut s = self.0.borrow_mut();
        if s.selected == Some(index) {
            return PanelResult::Used;
        }
        s.selected = Some(index);
        s.remembered = Some(index);
        draw(&s);
        return PanelResult::Selected;
    }
}

impl Panel for FilesystemPanel {
    fn panel_attach(&self) -> El {
        let rows = el("div").classes(&["merman_rows"]);
        let panel = el("div").classes(&["merman_panel", "merman_panel_filesystem"]).push(rows.clone());
        let mut s = self.0.borrow_mut();
        s.attached = Some((panel.clone(), rows));
        draw(&s);
        return panel;
    }

    fn panel_detach(&self) {
        self.0.borrow_mut().attached = None;
        return;
    }

    fn panel_path(&self) -> String {
        return self.0.borrow().dir.clone();
    }

    fn panel_parent(&self) -> Option<String> {
        return self.0.borrow().parent.clone();
    }

    fn panel_selection(&self) -> Option<(bool, String)> {
        let s = self.0.borrow();
        return s.selected.map(|i| (s.entries[i].dir, s.entries[i].path.clone()));
    }

    fn panel_focusable(&self) -> bool {
        return true;
    }

    fn panel_focused(&self, focused: bool) {
        let mut s = self.0.borrow_mut();
        if focused {
            if s.selected.is_some() || s.entries.is_empty() {
                return;
            }
            s.selected = Some(s.remembered.unwrap_or(0));
        } else {
            if s.selected.is_none() {
                return;
            }
            s.selected = None;
        }
        draw(&s);
        return;
    }

    fn panel_key(&self, e: &KeyboardEvent) -> PanelResult {
        let Some(stroke) =
            panel_key_stroke(e, DirectionConvert::new(SpecDirection::Right, SpecDirection::Down)) else {
                return PanelResult::Ignored;
            };
        let resolved = {
            let mut s = self.0.borrow_mut();
            let mut pending = std::mem::take(&mut s.pending);
            let resolved = s.keys.keymap_read(&mut pending, stroke, Some(CursorKind::Array));
            s.pending = pending;
            resolved
        };
        let action = match resolved {
            KeyResolve::Unbound => return PanelResult::Ignored,
            KeyResolve::Pending => return PanelResult::Used,
            KeyResolve::Action(a) => a,
        };
        let (count, selected) = {
            let s = self.0.borrow();
            (s.entries.len(), s.selected)
        };
        if count == 0 {
            return PanelResult::Unused(action);
        }
        let Some(selected) = selected else {
            match action {
                Action::NextElement |
                Action::SelectNext |
                Action::PreviousElement |
                Action::SelectPrevious |
                Action::FirstElement => return self.filesystem_select(
                    0,
                ),
                Action::LastElement => return self.filesystem_select(count - 1),
                _ => return PanelResult::Unused(action),
            }
        };
        match action {
            Action::NextElement | Action::SelectNext => return self.filesystem_select((selected + 1) % count),
            Action::PreviousElement | Action::SelectPrevious => return self.filesystem_select(
                (selected + count - 1) % count,
            ),
            Action::FirstElement => return self.filesystem_select(0),
            Action::LastElement => return self.filesystem_select(count - 1),
            Action::Copy => {
                let path = self.0.borrow().entries[selected].path.clone();
                let _ = window().navigator().clipboard().write_text(&path);
                return PanelResult::Used;
            },
            _ => return PanelResult::Unused(action),
        }
    }

    fn panel_mouse(&self, e: &MouseEvent) -> PanelResult {
        if e.button() != 0 {
            return PanelResult::Ignored;
        }
        let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return PanelResult::Ignored;
        };
        let Some(row) = target.closest(".merman_row").ok().flatten() else {
            return PanelResult::Ignored;
        };
        let index = {
            let s = self.0.borrow();
            let Some((_, rows)) = s.attached.as_ref() else {
                return PanelResult::Ignored;
            };
            let children = rows.raw().children();
            (0 .. children.length()).find(|i| children.item(*i).is_some_and(|c| c.is_same_node(Some(&row))))
        };
        let Some(index) = index else {
            return PanelResult::Ignored;
        };
        return self.filesystem_select(index as usize);
    }

    fn panel_changed(&self, path: &str) -> Option<bool> {
        let s = self.0.borrow();
        if path == s.dir || panel_path_parent(path) == Some(s.dir.as_str()) {
            return Some(true);
        }
        return None;
    }

    fn panel_cursor_path(&self) -> Option<Vec<String>> {
        return None;
    }

    fn panel_reference(&self) -> Option<String> {
        return self.panel_selection().map(|(_, path)| path);
    }
}
