use crate::client::client_send;
use crate::panels::code::CodePanel;
use crate::panels::error::ErrorPanel;
use crate::panels::filesystem::FilesystemPanel;
use crate::panels::{
    Panel,
    PanelResult,
};
use futures::channel::oneshot::Receiver;
use gloo_events::EventListener;
use gloo_utils::document;
use merman3_api::{
    ReqList,
    ReqOpen,
    ReqStart,
};
use merman3_core::keys::{
    Action,
    Keymap,
    SpecKeys,
};
use merman3_core::matcher::match_document;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use rooting::{
    el,
    set_root,
    set_root_non_dom,
    spawn_rooted,
    El,
};
use std::cell::{
    Cell,
    RefCell,
};
use std::collections::HashMap;
use std::rc::{
    Rc,
    Weak,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    KeyboardEvent,
    MouseEvent,
};

struct Editor {
    keys: Keymap,
    element: El,
    panels: RefCell<Vec<Rc<dyn Panel>>>,
    shown: RefCell<Vec<(Rc<dyn Panel>, El)>>,
    focus: Cell<usize>,
    focus_next: Cell<bool>,
    selections: RefCell<HashMap<String, String>>,
    child_request: RefCell<Option<Receiver<()>>>,
    parent_request: RefCell<Option<Receiver<()>>>,
}

async fn editor_list(editor: &Editor, dir: String, select: Option<String>) -> Rc<dyn Panel> {
    let keys = editor.keys.clone();
    let select = select.or_else(|| editor.selections.borrow().get(&dir).cloned());
    match client_send(ReqList { dir: dir.clone() }).await {
        Ok(listing) => return Rc::new(FilesystemPanel::filesystem_new(keys, listing, select)),
        Err(e) => return Rc::new(ErrorPanel::error_new(dir, &e)),
    }
}

fn panel_same(a: &Rc<dyn Panel>, b: &Rc<dyn Panel>) -> bool {
    return std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b));
}

fn editor_index(editor: &Editor, panel: &Rc<dyn Panel>) -> Option<usize> {
    return editor.panels.borrow().iter().position(|p| panel_same(p, panel));
}

fn editor_show(editor: &Rc<Editor>) {
    let panels = editor.panels.borrow();
    let wanted = &panels[panels.len().saturating_sub(2)..];
    let mut shown = editor.shown.borrow_mut();
    let mut i = 0;
    while i < shown.len() {
        if wanted.iter().any(|w| panel_same(w, &shown[i].0)) {
            i += 1;
            continue;
        }
        editor.element.ref_splice(i, 1, vec![]);
        let (panel, _) = shown.remove(i);
        panel.panel_detach();
    }
    for (i, panel) in wanted.iter().enumerate() {
        if shown.iter().any(|(p, _)| panel_same(p, panel)) {
            continue;
        }
        let element = panel.panel_attach();
        element.ref_on("mousedown", {
            let editor: Weak<Editor> = Rc::downgrade(editor);
            let panel = panel.clone();
            move |e| {
                let e: &MouseEvent = e.dyn_ref().unwrap();
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                let Some(index) = editor_index(&editor, &panel) else {
                    return;
                };
                if e.button() == 0 {
                    editor_focus(&editor, index);
                }
                if editor_result(&editor, index, panel.panel_mouse(e)) {
                    e.prevent_default();
                }
            }
        });
        editor.element.ref_splice(i, 0, vec![element.clone()]);
        shown.insert(i, (panel.clone(), element));
    }
    for (panel, element) in shown.iter() {
        let focused = panels.get(editor.focus.get()).is_some_and(|f| panel_same(panel, f));
        element.ref_modify_classes(&[("merman_panel_focus", focused)]);
        if focused {
            element.raw().scroll_into_view();
        }
    }
    return;
}

fn editor_focus(editor: &Rc<Editor>, index: usize) {
    editor.focus.set(index);
    editor.focus_next.set(false);
    let count = editor.panels.borrow().len();
    editor_splice(editor, (index + 2).min(count), count.saturating_sub(index + 2), vec![]);
    let (panel, child) = {
        let panels = editor.panels.borrow();
        (panels[index].clone(), panels.get(index + 1).cloned())
    };
    if let Some(child) = child {
        child.panel_focused(false);
    }
    panel.panel_focused(true);
    editor_show(editor);
    editor_sync(editor, index);
    return;
}

fn editor_splice(editor: &Rc<Editor>, offset: usize, remove: usize, add: Vec<Rc<dyn Panel>>) {
    let focus = editor.focus.get();
    if focus >= offset + remove {
        editor.focus.set(focus - remove + add.len());
    }
    editor.panels.borrow_mut().splice(offset .. offset + remove, add);
    editor_show(editor);
    return;
}

fn editor_sync(editor: &Rc<Editor>, index: usize) {
    let panel = editor.panels.borrow()[index].clone();
    let count = editor.panels.borrow().len();
    let Some((dir, path)) = panel.panel_selection() else {
        editor_splice(editor, index + 1, count - index - 1, vec![]);
        return;
    };
    *editor.child_request.borrow_mut() = None;
    let current = editor.panels.borrow().get(index + 1).map(|p| p.panel_path());
    if current.as_deref() == Some(path.as_str()) {
        return;
    }
    editor_splice(editor, index + 1, count - index - 1, vec![]);
    editor.selections.borrow_mut().insert(panel.panel_path(), path.clone());
    let request = spawn_rooted({
        let keys = editor.keys.clone();
        let editor: Weak<Editor> = Rc::downgrade(editor);
        async move {
            let child: Rc<dyn Panel> = if dir {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                editor_list(&editor, path, None).await
            } else {
                let built = match client_send(ReqOpen { path: path.clone() }).await {
                    Ok(opened) => (|| -> Result<_, String> {
                        let spec: SpecSyntax =
                            serde_json::from_str(
                                &opened.syntax,
                            ).map_err(|e| format!("Error parsing syntax JSON: {}", e))?;
                        let syntax =
                            Rc::new(Syntax::syntax_resolve(spec).map_err(|e| format!("Syntax errors:\n{}", e))?);
                        let value: serde_json::Value =
                            serde_json::from_str(
                                &opened.source,
                            ).map_err(|e| format!("Error parsing source JSON: {}", e))?;
                        let document =
                            Rc::new(
                                match_document(
                                    &syntax,
                                    &value,
                                ).map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?,
                            );
                        return Ok((syntax, document));
                    })(),
                    Err(e) => Err(e),
                };
                match built {
                    Ok((syntax, document)) => Rc::new(CodePanel::code_new(keys, path, syntax, document)),
                    Err(e) => Rc::new(ErrorPanel::error_new(path, &e)),
                }
            };
            let Some(editor) = editor.upgrade() else {
                return;
            };
            let Some(index) = editor_index(&editor, &panel) else {
                return;
            };
            let jump = editor.focus.get() == index && editor.focus_next.get() && child.panel_focusable();
            let count = editor.panels.borrow().len();
            editor_splice(&editor, index + 1, count - index - 1, vec![child]);
            if jump {
                editor_focus(&editor, index + 1);
            }
        }
    });
    *editor.child_request.borrow_mut() = Some(request);
    return;
}

fn editor_result(editor: &Rc<Editor>, index: usize, result: PanelResult) -> bool {
    let action = match result {
        PanelResult::Ignored => return false,
        PanelResult::Used => return true,
        PanelResult::Selected => {
            editor_sync(editor, index);
            return true;
        },
        PanelResult::Unused(action) => action,
    };
    match action {
        Action::Enter => {
            let child = editor.panels.borrow().get(index + 1).cloned();
            match child {
                Some(child) => {
                    if child.panel_focusable() {
                        editor_focus(editor, index + 1);
                    }
                },
                None => editor.focus_next.set(true),
            }
        },
        Action::Exit => {
            if index > 0 {
                editor_focus(editor, index - 1);
                return true;
            }
            let panel = editor.panels.borrow()[0].clone();
            let Some(parent) = panel.panel_parent() else {
                return true;
            };
            let request = spawn_rooted({
                let editor: Weak<Editor> = Rc::downgrade(editor);
                async move {
                    let Some(editor) = editor.upgrade() else {
                        return;
                    };
                    let parent = editor_list(&editor, parent, Some(panel.panel_path())).await;
                    if editor_index(&editor, &panel) != Some(0) {
                        return;
                    }
                    editor_splice(&editor, 0, 0, vec![parent]);
                    editor_focus(&editor, 0);
                }
            });
            *editor.parent_request.borrow_mut() = Some(request);
        },
        _ => { },
    }
    return true;
}

#[wasm_bindgen]
pub fn start_editor() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        let start = match client_send(ReqStart {}).await {
            Ok(start) => start,
            Err(e) => {
                set_root(vec![el("pre").classes(&["merman_error"]).text(&e)]);
                return;
            },
        };
        let keys = (|| -> Result<Keymap, String> {
            let spec =
                serde_json::from_str::<SpecKeys>(
                    &start.keys,
                ).map_err(|e| format!("Error parsing keys JSON: {}", e))?;
            return Keymap::keymap_resolve(&spec).map_err(|e| format!("Errors in key bindings:\n{}", e));
        })();
        let keys = match keys {
            Ok(keys) => keys,
            Err(e) => {
                set_root(vec![el("pre").classes(&["merman_error"]).text(&e)]);
                return;
            },
        };
        let editor = Rc::new(Editor {
            keys: keys,
            element: el("div").classes(&["merman_panels"]),
            panels: RefCell::new(vec![]),
            shown: RefCell::new(vec![]),
            focus: Cell::new(0),
            focus_next: Cell::new(false),
            selections: RefCell::new(HashMap::new()),
            child_request: RefCell::new(None),
            parent_request: RefCell::new(None),
        });
        let root = editor_list(&editor, start.dir, start.file.clone()).await;
        editor.element.ref_own(|_| EventListener::new(&document(), "keydown", {
            let editor = editor.clone();
            move |e| {
                let e: &KeyboardEvent = e.dyn_ref().unwrap();
                let index = editor.focus.get();
                let panel = editor.panels.borrow().get(index).cloned();
                let Some(panel) = panel else {
                    return;
                };
                if editor_result(&editor, index, panel.panel_key(e)) {
                    e.prevent_default();
                }
            }
        }));
        set_root(vec![editor.element.clone()]);
        editor_splice(&editor, 0, 0, vec![root]);
        editor_focus(&editor, 0);
        editor.focus_next.set(start.file.is_some());
        set_root_non_dom(editor);
    });
}
