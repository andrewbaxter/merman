use crate::client::client_send;
use crate::panels::{
    panel_key_stroke,
    PanelKey,
};
use futures::channel::oneshot::Receiver;
use gloo_utils::window;
use merman3_api::{
    ListEntry,
    ReqList,
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
    spawn_rooted,
    El,
};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{
    HtmlElement,
    KeyboardEvent,
    MouseEvent,
};

struct State {
    keys: Keymap,
    pending: Vec<KeyStroke>,
    parent: Option<String>,
    entries: Vec<ListEntry>,
    selected: Option<usize>,
    panel: El,
    rows: El,
    request: Option<Receiver<()>>,
    on_open: Rc<dyn Fn(&str)>,
}

fn draw(state: &Rc<RefCell<State>>) {
    let rows;
    {
        let s = state.borrow();
        if s.entries.is_empty() {
            s.rows.ref_clear();
            s.rows.ref_push(el("div").classes(&["merman_row_empty"]).text("empty"));
            return;
        }
        rows = s.entries.iter().enumerate().map(|(i, entry)| {
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
            row.ref_on("mousedown", {
                let state = state.clone();
                move |e| {
                    let e: &MouseEvent = e.dyn_ref().unwrap();
                    if e.button() != 0 {
                        return;
                    }
                    e.prevent_default();
                    state.borrow_mut().selected = Some(i);
                    draw(&state);
                }
            });
            return row;
        }).collect::<Vec<_>>();
    }
    let s = state.borrow();
    s.rows.ref_clear();
    s.rows.ref_extend(rows);
    let panel = s.panel.raw();
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

pub struct FilesystemPanel(Rc<RefCell<State>>);

impl FilesystemPanel {
    pub fn filesystem_new(keys: Keymap, on_open: impl Fn(&str) + 'static) -> FilesystemPanel {
        let rows = el("div").classes(&["merman_rows"]);
        let panel = el("div").classes(&["merman_panel", "merman_panel_filesystem"]).push(rows.clone());
        let state = Rc::new(RefCell::new(State {
            keys: keys,
            pending: vec![],
            parent: None,
            entries: vec![],
            selected: None,
            panel: panel.clone(),
            rows: rows,
            request: None,
            on_open: Rc::new(on_open),
        }));
        return FilesystemPanel(state);
    }

    pub fn filesystem_element(&self) -> El {
        return self.0.borrow().panel.clone();
    }

    pub fn filesystem_show(&self, dir: &str) {
        let state = self.0.clone();
        let dir = dir.to_string();
        let request = spawn_rooted(async move {
            match client_send(ReqList { dir: dir }).await {
                Ok(listing) => {
                    {
                        let mut s = state.borrow_mut();
                        s.parent = listing.parent;
                        s.entries = listing.entries;
                        s.selected = if s.entries.is_empty() {
                            None
                        } else {
                            Some(0)
                        };
                    }
                    state.borrow().panel.raw().scroll_to_with_x_and_y(0., 0.);
                    draw(&state);
                },
                Err(e) => {
                    let s = state.borrow();
                    s.rows.ref_clear();
                    s.rows.ref_push(el("pre").classes(&["merman_error"]).text(&e));
                },
            }
        });
        self.0.borrow_mut().request = Some(request);
    }

    pub fn filesystem_key(&self, e: &KeyboardEvent) -> PanelKey {
        let Some(stroke) =
            panel_key_stroke(e, DirectionConvert::new(SpecDirection::Right, SpecDirection::Down)) else {
                return PanelKey::Ignored;
            };
        let resolved = {
            let mut s = self.0.borrow_mut();
            let mut pending = std::mem::take(&mut s.pending);
            let resolved = s.keys.keymap_read(&mut pending, stroke, Some(CursorKind::Array));
            s.pending = pending;
            resolved
        };
        let action = match resolved {
            KeyResolve::Unbound => return PanelKey::Ignored,
            KeyResolve::Pending => return PanelKey::Used,
            KeyResolve::Action(a) => a,
        };
        let (count, selected, parent) = {
            let s = self.0.borrow();
            (s.entries.len(), s.selected, s.parent.clone())
        };
        let select = |new: usize| -> PanelKey {
            if Some(new) == selected {
                return PanelKey::Used;
            }
            self.0.borrow_mut().selected = Some(new);
            draw(&self.0);
            return PanelKey::Used;
        };
        match action {
            Action::Exit => {
                let Some(parent) = parent else {
                    return PanelKey::Unused(action);
                };
                self.filesystem_show(&parent);
                return PanelKey::Used;
            },
            _ => { },
        }
        let Some(selected) = selected else {
            return PanelKey::Unused(action);
        };
        match action {
            Action::Enter => {
                let (dir, path, on_open) = {
                    let s = self.0.borrow();
                    let entry = &s.entries[selected];
                    (entry.dir, entry.path.clone(), s.on_open.clone())
                };
                if dir {
                    self.filesystem_show(&path);
                } else {
                    on_open(&path);
                }
                return PanelKey::Used;
            },
            Action::NextElement | Action::SelectNext => return select((selected + 1) % count),
            Action::PreviousElement | Action::SelectPrevious => return select((selected + count - 1) % count),
            Action::FirstElement => return select(0),
            Action::LastElement => return select(count - 1),
            Action::Copy => {
                let path = self.0.borrow().entries[selected].path.clone();
                let _ = window().navigator().clipboard().write_text(&path);
                return PanelKey::Used;
            },
            Action::Exit => unreachable!(),
            _ => return PanelKey::Unused(action),
        }
    }
}
