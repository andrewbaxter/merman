use crate::client::client_send;
use crate::panels::code::CodePanel;
use crate::panels::filesystem::FilesystemPanel;
use crate::panels::PanelKey;
use futures::channel::oneshot::Receiver;
use gloo_events::EventListener;
use gloo_utils::document;
use merman3_api::{
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

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Filesystem,
    Code,
}

struct Editor {
    keys: Keymap,
    panels: El,
    filesystem: RefCell<Option<FilesystemPanel>>,
    code: RefCell<Option<CodePanel>>,
    right: RefCell<Option<El>>,
    focus: Cell<Focus>,
    request: RefCell<Option<Receiver<()>>>,
}

fn editor_right(editor: &Rc<Editor>, element: El) {
    let current = editor.right.borrow().as_ref().map(|e| e.ptr_id());
    if current == Some(element.ptr_id()) {
        return;
    }
    let replace = current.is_some();
    editor.panels.ref_splice(1, if replace {
        1
    } else {
        0
    }, vec![element.clone()]);
    *editor.right.borrow_mut() = Some(element);
}

fn editor_focus(editor: &Rc<Editor>, focus: Focus) {
    editor.focus.set(focus);
    if let Some(filesystem) = editor.filesystem.borrow().as_ref() {
        filesystem.filesystem_element().ref_modify_classes(&[("merman_panel_focus", focus == Focus::Filesystem)]);
    }
    if let Some(right) = editor.right.borrow().as_ref() {
        right.ref_modify_classes(&[("merman_panel_focus", focus == Focus::Code)]);
    }
}

fn editor_open(editor: &Rc<Editor>, path: &str) {
    let path = path.to_string();
    let request = spawn_rooted({
        let editor = editor.clone();
        async move {
            let opened = match client_send(ReqOpen { path: path }).await {
                Ok(opened) => opened,
                Err(e) => {
                    editor_right(&editor, el("pre").classes(&["merman_panel", "merman_error"]).text(&e));
                    editor_focus(&editor, Focus::Code);
                    return;
                },
            };
            let built = (|| -> Result<_, String> {
                let spec: SpecSyntax =
                    serde_json::from_str(&opened.syntax).map_err(|e| format!("Error parsing syntax JSON: {}", e))?;
                let syntax = Rc::new(Syntax::syntax_resolve(spec).map_err(|e| format!("Syntax errors:\n{}", e))?);
                let value: serde_json::Value =
                    serde_json::from_str(&opened.source).map_err(|e| format!("Error parsing source JSON: {}", e))?;
                let document =
                    Rc::new(
                        match_document(
                            &syntax,
                            &value,
                        ).map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?,
                    );
                return Ok((syntax, document));
            })();
            let (syntax, document) = match built {
                Ok(v) => v,
                Err(e) => {
                    editor_right(&editor, el("pre").classes(&["merman_panel", "merman_error"]).text(&e));
                    editor_focus(&editor, Focus::Code);
                    return;
                },
            };
            let element = {
                let mut code = editor.code.borrow_mut();
                match code.as_ref() {
                    Some(code) => {
                        code.code_set(syntax, document);
                        code.code_element()
                    },
                    None => {
                        let panel = CodePanel::code_new(editor.keys.clone(), syntax, document);
                        let element = panel.code_element();
                        element.ref_on("mousedown", {
                            let editor = editor.clone();
                            move |e| {
                                let e: &MouseEvent = e.dyn_ref().unwrap();
                                if e.button() != 0 {
                                    return;
                                }
                                editor_focus(&editor, Focus::Code);
                            }
                        });
                        *code = Some(panel);
                        element
                    },
                }
            };
            editor_right(&editor, element);
            editor_focus(&editor, Focus::Code);
        }
    });
    *editor.request.borrow_mut() = Some(request);
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
            keys: keys.clone(),
            panels: el("div").classes(&["merman_panels"]),
            filesystem: RefCell::new(None),
            code: RefCell::new(None),
            right: RefCell::new(None),
            focus: Cell::new(Focus::Filesystem),
            request: RefCell::new(None),
        });
        let filesystem = FilesystemPanel::filesystem_new(keys, {
            let editor: Weak<Editor> = Rc::downgrade(&editor);
            move |path| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                editor_open(&editor, path);
            }
        });
        let element = filesystem.filesystem_element();
        element.ref_on("mousedown", {
            let editor = editor.clone();
            move |e| {
                let e: &MouseEvent = e.dyn_ref().unwrap();
                if e.button() != 0 {
                    return;
                }
                editor_focus(&editor, Focus::Filesystem);
            }
        });
        editor.panels.ref_push(element);
        *editor.filesystem.borrow_mut() = Some(filesystem);
        editor.panels.ref_own(|_| EventListener::new(&document(), "keydown", {
            let editor = editor.clone();
            move |e| {
                let e: &KeyboardEvent = e.dyn_ref().unwrap();
                let editor = &editor;
                let focus = editor.focus.get();
                let handled = match focus {
                    Focus::Filesystem => {
                        let filesystem = editor.filesystem.borrow();
                        match filesystem.as_ref() {
                            Some(filesystem) => filesystem.filesystem_key(e),
                            None => PanelKey::Ignored,
                        }
                    },
                    Focus::Code => {
                        let code = editor.code.borrow();
                        match code.as_ref() {
                            Some(code) => code.code_key(e),
                            None => PanelKey::Ignored,
                        }
                    },
                };
                match handled {
                    PanelKey::Ignored => return,
                    PanelKey::Used => {
                        e.prevent_default();
                        return;
                    },
                    PanelKey::Unused(action) => {
                        e.prevent_default();
                        if focus == Focus::Code && action == Action::Exit {
                            editor_focus(editor, Focus::Filesystem);
                        }
                    },
                }
            }
        }));
        set_root(vec![editor.panels.clone()]);
        editor.filesystem.borrow().as_ref().unwrap().filesystem_show(&start.dir);
        editor_focus(&editor, Focus::Filesystem);
        if let Some(file) = &start.file {
            editor_open(&editor, file);
        }
        set_root_non_dom(editor);
    });
}
