use crate::panels::code::CodePanel;
use crate::panels::PanelKey;
use gloo_events::EventListener;
use gloo_utils::document;
use merman3_core::keys::{
    Keymap,
    SpecKeys,
};
use merman3_core::matcher::match_document;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use rooting::{
    el,
    set_root,
};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;

fn read_embedded(id: &str) -> Result<String, String> {
    let Some(e) = document().get_element_by_id(id) else {
        return Err(format!("Page is missing embedded data element `{}`", id));
    };
    return Ok(e.text_content().unwrap_or_default());
}

#[wasm_bindgen]
pub fn start_demo() {
    console_error_panic_hook::set_once();
    if let Err(e) = (|| -> Result<(), String> {
        let spec: SpecSyntax =
            serde_json::from_str(
                &read_embedded("merman-syntax")?,
            ).map_err(|e| format!("Error parsing syntax JSON: {}", e))?;
        let syntax = Rc::new(Syntax::syntax_resolve(spec).map_err(|e| format!("Syntax errors:\n{}", e))?);
        let keys = match document().get_element_by_id("merman-keys").map(|e| e.text_content().unwrap_or_default()) {
            Some(text) if !text.trim().is_empty() => serde_json::from_str::<SpecKeys>(
                &text,
            ).map_err(|e| format!("Error parsing keys JSON: {}", e))?,
            _ => SpecKeys::default(),
        };
        let keys = Keymap::keymap_resolve(&keys).map_err(|e| format!("Errors in key bindings:\n{}", e))?;
        let value: serde_json::Value =
            serde_json::from_str(
                &read_embedded("merman-source")?,
            ).map_err(|e| format!("Error parsing source JSON: {}", e))?;
        let document_ =
            Rc::new(
                match_document(
                    &syntax,
                    &value,
                ).map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?,
            );
        let panel = Rc::new(CodePanel::code_new(keys, syntax, document_));
        let element = panel.code_element();
        element.ref_classes(&["merman_panel_focus"]);
        element.ref_own(|_| EventListener::new(&document(), "keydown", {
            let panel = panel.clone();
            move |e| {
                let e: &KeyboardEvent = e.dyn_ref().unwrap();
                if let PanelKey::Ignored = panel.code_key(e) {
                    return;
                }
                e.prevent_default();
            }
        }));
        set_root(vec![el("div").classes(&["merman_panels"]).push(element)]);
        return Ok(());
    })() {
        set_root(vec![el("pre").classes(&["merman_error"]).text(&e)]);
    }
}
