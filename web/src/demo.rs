use {
    crate::panels::{
        Panel,
        PanelResult,
        code::CodePanel,
        panel_theme_apply,
    },
    gloo_events::{
        EventListener,
        EventListenerOptions,
    },
    gloo_utils::document,
    merman_core::{
        keys::{
            Keymap,
            SpecKeys,
        },
        matcher::{
            match_document,
            source_parse,
        },
        spec::{
            SpecSyntax,
            SpecTheme,
        },
        syntax::Syntax,
    },
    rooting::{
        el,
        set_root,
    },
    std::rc::Rc,
    wasm_bindgen::{
        JsCast,
        prelude::*,
    },
    web_sys::KeyboardEvent,
};

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
        let theme: SpecTheme =
            serde_json::from_str(
                &read_embedded("merman-theme")?,
            ).map_err(|e| format!("Error parsing theme JSON: {}", e))?;
        panel_theme_apply(&theme);
        let syntax = Rc::new(Syntax::syntax_resolve(spec, &theme).map_err(|e| format!("Syntax errors:\n{}", e))?);
        let keys = match document().get_element_by_id("merman-keys").map(|e| e.text_content().unwrap_or_default()) {
            Some(text) if !text.trim().is_empty() => serde_json::from_str::<SpecKeys>(
                &text,
            ).map_err(|e| format!("Error parsing keys JSON: {}", e))?,
            _ => SpecKeys::default(),
        };
        let keys = Keymap::keymap_resolve(&keys).map_err(|e| format!("Errors in key bindings:\n{}", e))?;
        let value: serde_json::Value =
            source_parse(
                &read_embedded("merman-source")?,
            ).map_err(|e| format!("Error parsing source JSON: {}", e))?;
        let document_ =
            match_document(
                &syntax,
                &value,
            ).map_err(|e| format!("Source doesn't match syntax:\n{}", e.mismatch_format()))?;
        let panel = CodePanel::code_new(keys, String::new(), syntax, document_, None, None);
        let element = panel.panel_attach();
        element.ref_classes(&["merman_panel_focus"]);
        element.ref_own(
            |_| EventListener::new_with_options(
                &document(),
                "keydown",
                EventListenerOptions::enable_prevent_default(),
                {
                    let panel = panel.clone();
                    move |e| {
                        let e: &KeyboardEvent = e.dyn_ref().unwrap();
                        if let PanelResult::Ignored = panel.panel_key(e) {
                            return;
                        }
                        e.prevent_default();
                    }
                },
            ),
        );
        set_root(vec![el("div").classes(&["merman_panels"]).push(element)]);
        return Ok(());
    })() {
        set_root(vec![el("pre").classes(&["merman_error"]).text(&e)]);
    }
}
