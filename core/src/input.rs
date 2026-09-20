use crate::context::Context;
use crate::cursor::{
    DragSelect,
    Located,
};
use crate::document::Field;

impl Context {
    pub fn mouse_button(&mut self, press: bool, now_ms: &mut dyn FnMut() -> f64) -> bool {
        let handled = if !press {
            if self.drag_select.is_some() {
                self.drag_select = None;
                true
            } else {
                false
            }
        } else if let Some(h) = self.hover {
            let path = self.hoverable_syntax_path(h);
            self.hoverable_select(h);
            self.drag_select = Some(DragSelect {
                start: path,
                end: None,
            });
            true
        } else if let Some(c) = self.cursor {
            let path = self.cursor_syntax_path(c);
            self.drag_select = Some(DragSelect {
                start: path,
                end: None,
            });
            true
        } else {
            false
        };
        if handled {
            self.flush_iteration(100, now_ms);
        }
        self.clear_hover();
        return handled;
    }

    pub fn key_copy(&mut self, now_ms: &mut dyn FnMut() -> f64) {
        if self.cursor.is_none() {
            return;
        }
        self.cursor_copy();
        self.flush_iteration(100, now_ms);
        self.clear_hover();
    }

    pub fn hover_changed(&mut self) {
        let Some(h) = self.hover else {
            return;
        };
        let Some(drag) = &self.drag_select else {
            return;
        };
        let end_path = self.hoverable_syntax_path(h);
        if drag.end.as_ref() == Some(&end_path) {
            return;
        }
        let start_path = drag.start.clone();
        self.drag_select.as_mut().unwrap().end = Some(end_path.clone());
        let mut longest_match = 0;
        while longest_match < start_path.len() && longest_match < end_path.len() &&
            start_path[longest_match] == end_path[longest_match] {
            longest_match += 1;
        }
        let Some(base) = self.syntax_locate(&end_path[..longest_match]) else {
            return;
        };
        match base {
            Located::Field(atom, field) => {
                let kind = match self.document.document_atom(atom).fields.get(&field).unwrap() {
                    Field::Array(_) => "array",
                    Field::Primitive(_) => "primitive",
                    Field::Atom(_) => "atom",
                };
                let atom_visual = self.atom_visual[atom].unwrap();
                let field_visual =
                    self
                        .visual_atom(atom_visual)
                        .selectable
                        .iter()
                        .find(|(f, _)| *f == field)
                        .map(|(_, v)| *v);
                let Some(field_visual) = field_visual else {
                    return;
                };
                match kind {
                    "array" => {
                        let (Some(start_index), Some(end_index)) =
                            (
                                start_path.get(longest_match).and_then(|s| s.parse::<usize>().ok()),
                                end_path.get(longest_match).and_then(|s| s.parse::<usize>().ok()),
                            ) else {
                                return;
                            };
                        if end_index < start_index {
                            self.field_array_select_into(field_visual, true, end_index, start_index);
                        } else {
                            self.field_array_select_into(field_visual, false, start_index, end_index);
                        }
                    },
                    "primitive" => {
                        let mut lm = longest_match;
                        if lm == start_path.len() {
                            lm -= 1;
                        }
                        let (Some(start_index), Some(end_index)) =
                            (
                                start_path.get(lm).and_then(|s| s.parse::<usize>().ok()),
                                end_path.get(lm).and_then(|s| s.parse::<usize>().ok()),
                            ) else {
                                return;
                            };
                        if end_index < start_index {
                            self.primitive_select(field_visual, true, end_index, start_index);
                        } else {
                            self.primitive_select(field_visual, false, start_index, end_index);
                        }
                    },
                    _ => {
                        let child = match self.document.document_atom(atom).fields.get(&field).unwrap() {
                            Field::Atom(c) => *c,
                            _ => unreachable!(),
                        };
                        self.atom_parent_select_field(child);
                    },
                }
            },
            Located::Atom(atom) => {
                self.atom_parent_select_field(atom);
            },
        }
    }
}
