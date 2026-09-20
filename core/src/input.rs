use crate::context::{
    Context,
    VisualId,
};
use crate::cursor::{
    Cursor,
    DragSelect,
    Located,
    RangeLoc,
};
use crate::document::Field;
use crate::keys::{
    Action,
    KeyMatch,
    KeyStroke,
};
use crate::visual::VisualKind;
use unicode_segmentation::UnicodeSegmentation;

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

fn text_next_glyph(text: &str, offset: usize) -> usize {
    for (i, g) in text.grapheme_indices(true) {
        if i + g.len() > offset {
            return i + g.len();
        }
    }
    return text.len();
}

fn text_previous_glyph(text: &str, offset: usize) -> usize {
    let mut out = 0;
    for (i, _) in text.grapheme_indices(true) {
        if i >= offset {
            break;
        }
        out = i;
    }
    return out;
}

impl Context {
    pub fn key_press(&mut self, stroke: KeyStroke, now_ms: &mut dyn FnMut() -> f64) -> bool {
        let mut sequence = std::mem::take(&mut self.key_pending);
        sequence.push(stroke);
        let action = match self.config.keys.keymap_match(&sequence) {
            KeyMatch::None => return false,
            KeyMatch::Prefix => {
                self.key_pending = sequence;
                return true;
            },
            KeyMatch::Action(a) => a,
        };
        let handled = self.key_action(action);
        if handled {
            self.flush_iteration(100, now_ms);
        }
        return handled;
    }

    pub fn key_action(&mut self, action: Action) -> bool {
        let Some(cursor) = self.cursor else {
            let root = self.root_visual;
            return self.visual_select_into_any_child(root);
        };
        match self.cursor_get(cursor) {
            Cursor::Atom(c) => {
                let (visual, index) = (c.visual, c.index);
                let count = self.visual_atom(visual).selectable.len();
                let select = |ctx: &mut Context, new: usize| -> bool {
                    if new == index {
                        return false;
                    }
                    ctx.atom_select(visual, new);
                    return true;
                };
                match action {
                    Action::Enter => {
                        let child = self.visual_atom(visual).selectable[index].1;
                        let into = match &self.visuals[child].kind {
                            VisualKind::FieldAtom(fa) => fa.body,
                            _ => child,
                        };
                        return self.visual_select_into_any_child(into);
                    },
                    Action::Exit | Action::Escape => {
                        let atom = self.visual_atom(visual).atom;
                        return self.atom_parent_select_field(atom);
                    },
                    Action::Next | Action::SelectNext => return select(self, (index + 1) % count),
                    Action::Previous | Action::SelectPrevious => return select(self, (index + count - 1) % count),
                    Action::First => return select(self, 0),
                    Action::Last => return select(self, count - 1),
                    Action::NextWord | Action::PreviousWord => return false,
                    Action::Copy => {
                        self.cursor_copy();
                        return true;
                    },
                }
            },
            Cursor::Array(c) => {
                let (visual, begin, end, lead_first) = (c.visual, c.begin_index, c.end_index, c.lead_first);
                let count = self.array_elements(visual).len();
                if count == 0 {
                    return false;
                }
                let position = |ctx: &mut Context, new: usize| -> bool {
                    if new == begin && new == end {
                        return false;
                    }
                    ctx.cursor_array_set_position(cursor, new);
                    return true;
                };
                match action {
                    Action::Enter => {
                        let element = self.array_element_visual(visual, begin);
                        return self.visual_select_into_any_child(element);
                    },
                    Action::Exit | Action::Escape => {
                        let (atom, field) = self.array_field(visual);
                        return self.field_parent_select_parent(atom, &field);
                    },
                    Action::Next => return position(self, (end + 1) % count),
                    Action::Previous => return position(self, (begin + count - 1) % count),
                    Action::First => return position(self, 0),
                    Action::Last => return position(self, count - 1),
                    Action::SelectNext => {
                        if lead_first && begin != end {
                            self.cursor_array_set_begin(cursor, begin + 1);
                            return true;
                        }
                        let new = (end + 1).min(count - 1);
                        if new == end {
                            return false;
                        }
                        self.cursor_array_set_end(cursor, new);
                        return true;
                    },
                    Action::SelectPrevious => {
                        if !lead_first && begin != end {
                            self.cursor_array_set_end(cursor, end - 1);
                            return true;
                        }
                        if begin == 0 {
                            return false;
                        }
                        self.cursor_array_set_begin(cursor, begin - 1);
                        return true;
                    },
                    Action::NextWord | Action::PreviousWord => return false,
                    Action::Copy => {
                        self.cursor_copy();
                        return true;
                    },
                }
            },
            Cursor::Primitive(c) => {
                let (visual, begin, end, lead_first) =
                    (c.visual, c.range.begin_offset, c.range.end_offset, c.range.lead_first);
                let loc = RangeLoc::Cursor(cursor);
                let text = self.visual_primitive(visual).value.clone();
                let point = |ctx: &mut Context, new: usize| -> bool {
                    if new == begin && new == end {
                        return false;
                    }
                    ctx.range_set_offsets(loc, new, new);
                    return true;
                };
                match action {
                    Action::Enter => return point(self, text_next_glyph(&text, end)),
                    Action::Exit => return point(self, text_previous_glyph(&text, begin)),
                    Action::NextWord => {
                        let mut new = text.len();
                        for (i, word) in text.split_word_bound_indices() {
                            if i > end && !word.chars().all(char::is_whitespace) {
                                new = i;
                                break;
                            }
                        }
                        return point(self, new);
                    },
                    Action::PreviousWord => {
                        let mut new = 0;
                        for (i, word) in text.split_word_bound_indices() {
                            if i >= begin {
                                break;
                            }
                            if !word.chars().all(char::is_whitespace) {
                                new = i;
                            }
                        }
                        return point(self, new);
                    },
                    Action::Next => {
                        let new = self.primitive_line_shift(visual, end, true);
                        return point(self, new);
                    },
                    Action::Previous => {
                        let new = self.primitive_line_shift(visual, begin, false);
                        return point(self, new);
                    },
                    Action::First => {
                        let (start, _) = self.primitive_line_bounds(visual, begin);
                        return point(self, start);
                    },
                    Action::Last => {
                        let (_, stop) = self.primitive_line_bounds(visual, end);
                        return point(self, stop);
                    },
                    Action::Escape => {
                        let (atom, field) = self.primitive_field(visual);
                        return self.field_parent_select_parent(atom, &field);
                    },
                    Action::SelectNext => {
                        if lead_first && begin != end {
                            let new = text_next_glyph(&text, begin).min(end);
                            if new == begin {
                                return false;
                            }
                            self.range_set_begin_offset(loc, new);
                            return true;
                        }
                        let new = text_next_glyph(&text, end);
                        if new == end {
                            return false;
                        }
                        self.range_set_end_offset(loc, new);
                        return true;
                    },
                    Action::SelectPrevious => {
                        if !lead_first && begin != end {
                            let new = text_previous_glyph(&text, end).max(begin);
                            if new == end {
                                return false;
                            }
                            self.range_set_end_offset(loc, new);
                            return true;
                        }
                        let new = text_previous_glyph(&text, begin);
                        if new == begin {
                            return false;
                        }
                        self.range_set_begin_offset(loc, new);
                        return true;
                    },
                    Action::Copy => {
                        self.cursor_copy();
                        return true;
                    },
                }
            },
        }
    }

    fn primitive_line_bounds(&self, visual: VisualId, offset: usize) -> (usize, usize) {
        let Some(index) = self.primitive_line_at(visual, offset) else {
            return (0, 0);
        };
        let line = &self.visual_primitive(visual).lines[index];
        return (line.offset, line.offset + line.text.len());
    }

    fn primitive_line_shift(&self, visual: VisualId, offset: usize, next: bool) -> usize {
        let Some(index) = self.primitive_line_at(visual, offset) else {
            return 0;
        };
        let lines = &self.visual_primitive(visual).lines;
        let line = &lines[index];
        let column = offset - line.offset;
        if next {
            let Some(to) = lines.get(index + 1) else {
                return line.offset + line.text.len();
            };
            return to.offset + column.min(to.text.len());
        }
        if index == 0 {
            return line.offset;
        }
        let to = &lines[index - 1];
        return to.offset + column.min(to.text.len());
    }

    fn primitive_line_at(&self, visual: VisualId, offset: usize) -> Option<usize> {
        let count = self.visual_primitive(visual).lines.len();
        if count == 0 {
            return None;
        }
        return Some(self.primitive_find_containing(visual, offset).min(count - 1));
    }
}
