use {
    rooting::{
        El,
        el,
    },
    wasm_bindgen::JsCast,
    web_sys::{
        Element,
        MouseEvent,
    },
};

pub fn toolbar_action(e: &MouseEvent) -> Option<String> {
    if e.button() != 0 {
        return None;
    }
    let button =
        e.target().and_then(|t| t.dyn_into::<Element>().ok())?.closest(".merman_toolbar_button").ok().flatten()?;
    return button.get_attribute("data-action");
}

pub fn toolbar_new(buttons: &[(&str, &str, &str)]) -> El {
    return el("div").classes(&["merman_toolbar"]).extend(buttons.iter().map(|(action, glyph, title)| {
        return el("div")
            .classes(&["merman_toolbar_button", "merman_icon"])
            .attr("data-action", action)
            .attr("title", title)
            .text(glyph);
    }).collect());
}
