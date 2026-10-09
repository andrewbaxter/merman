use {
    crate::panels::{
        PanelResult,
        panel_key_stroke,
    },
    merman_core::{
        cursor::CursorKind,
        direction::DirectionConvert,
        keys::{
            Action,
            KeyResolve,
            KeyStroke,
            Keymap,
        },
        spec::SpecDirection,
    },
    rooting::{
        El,
        el,
    },
    wasm_bindgen::JsCast,
    web_sys::{
        Element,
        HtmlElement,
        KeyboardEvent,
        MouseEvent,
    },
};

pub struct ListRow {
    pub icon: Option<&'static str>,
    pub spans: Vec<(String, String, String)>,
    pub text: String,
}

pub struct List {
    attached: Option<(El, El)>,
    class: &'static str,
    pub empty: &'static str,
    keys: Keymap,
    pending: Vec<KeyStroke>,
    remembered: Option<usize>,
    pub rows: Vec<ListRow>,
    pub selected: Option<usize>,
}

impl List {
    pub fn list_new(
        keys: Keymap,
        class: &'static str,
        empty: &'static str,
        rows: Vec<ListRow>,
        selected: Option<usize>,
    ) -> List {
        return List {
            attached: None,
            class: class,
            empty: empty,
            keys: keys,
            pending: vec![],
            remembered: selected,
            rows: rows,
            selected: selected,
        };
    }

    pub fn list_attach(&mut self) -> El {
        let rows = el("div").classes(&["merman_rows"]);
        let panel = el("div").classes(&["merman_panel", self.class]).push(rows.clone());
        self.attached = Some((panel.clone(), rows));
        self.list_draw();
        return panel;
    }

    pub fn list_detach(&mut self) {
        self.attached = None;
        return;
    }

    pub fn list_draw(&self) {
        let Some((panel, rows)) = self.attached.as_ref() else {
            return;
        };
        if self.rows.is_empty() {
            rows.ref_clear();
            rows.ref_push(el("div").classes(&["merman_row_empty"]).text(self.empty));
            return;
        }
        let new_rows = self.rows.iter().enumerate().map(|(i, entry)| {
            let row = el("div").classes(&["merman_row"]);
            if self.selected == Some(i) {
                row.ref_classes(&["merman_row_select"]);
            }
            if let Some(icon) = entry.icon {
                row.ref_push(el("span").classes(&["merman_icon"]).text(icon));
            }
            for (text, color, font) in &entry.spans {
                row.ref_push(
                    el("span")
                        .classes(&["merman_span"])
                        .attr("style", &format!("color: {}; font-family: {}; white-space: pre", color, font))
                        .text(text),
                );
            }
            row.ref_push(el("span").classes(&["merman_name"]).text(&entry.text));
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
        return;
    }

    pub fn list_focused(&mut self, focused: bool) {
        if focused {
            if self.selected.is_some() || self.rows.is_empty() {
                return;
            }
            self.selected = Some(self.remembered.unwrap_or(0));
        } else {
            if self.selected.is_none() {
                return;
            }
            self.selected = None;
        }
        self.list_draw();
        return;
    }

    pub fn list_key(&mut self, e: &KeyboardEvent) -> PanelResult {
        let Some(stroke) =
            panel_key_stroke(e, DirectionConvert::new(SpecDirection::Right, SpecDirection::Down)) else {
                return PanelResult::Ignored;
            };
        let action = match self.keys.keymap_read(&mut self.pending, stroke, Some(CursorKind::Array), false) {
            KeyResolve::Unbound | KeyResolve::Type => return PanelResult::Ignored,
            KeyResolve::Pending => return PanelResult::Used,
            KeyResolve::Action(a) => a,
        };
        let count = self.rows.len();
        if count == 0 {
            return PanelResult::Unused(action);
        }
        let Some(selected) = self.selected else {
            match action {
                Action::NextElement |
                Action::SelectNext |
                Action::PreviousElement |
                Action::SelectPrevious |
                Action::FirstElement => return self.list_select(
                    0,
                ),
                Action::LastElement => return self.list_select(count - 1),
                _ => return PanelResult::Unused(action),
            }
        };
        match action {
            Action::NextElement | Action::SelectNext => return self.list_select((selected + 1) % count),
            Action::PreviousElement | Action::SelectPrevious => return self.list_select(
                (selected + count - 1) % count,
            ),
            Action::FirstElement => return self.list_select(0),
            Action::LastElement => return self.list_select(count - 1),
            _ => return PanelResult::Unused(action),
        }
    }

    pub fn list_mouse(&mut self, e: &MouseEvent) -> PanelResult {
        if e.button() != 0 {
            return PanelResult::Ignored;
        }
        let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return PanelResult::Ignored;
        };
        let Some(row) = target.closest(".merman_row").ok().flatten() else {
            return PanelResult::Ignored;
        };
        let Some((_, rows)) = self.attached.as_ref() else {
            return PanelResult::Ignored;
        };
        let children = rows.raw().children();
        let index =
            (0 .. children.length()).find(|i| children.item(*i).is_some_and(|c| c.is_same_node(Some(&row))));
        let Some(index) = index else {
            return PanelResult::Ignored;
        };
        return self.list_select(index as usize);
    }

    pub fn list_select(&mut self, index: usize) -> PanelResult {
        if self.selected == Some(index) {
            return PanelResult::Used;
        }
        self.selected = Some(index);
        self.remembered = Some(index);
        self.list_draw();
        return PanelResult::Selected;
    }
}
