use crate::{
    context::{
        Context,
        VisualId,
    },
    cursor::{
        Cursor,
        CursorKind,
        DragSelect,
        Located,
        RangeLoc,
    },
    document::Field,
    keys::{
        Action,
        KeyResolve,
        KeyStroke,
    },
    syntax::Front,
    visual::VisualKind,
};

impl Context {
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

    pub fn input_flush(&mut self) {
        self.flush_iteration(100);
    }

    pub fn key_copy(&mut self) {
        if self.cursor.is_none() {
            return;
        }
        self.cursor_copy();
        self.input_flush();
        self.clear_hover();
    }

    pub fn mouse_button(&mut self, press: bool) -> bool {
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
            self.input_flush();
        }
        self.clear_hover();
        return handled;
    }
}

impl Context {
    pub fn key_action(&mut self, action: Action) -> bool {
        match action {
            Action::ClearWindow => {
                if !self.window {
                    return false;
                }
                self.window_clear();
                self.trigger_idle_lay_bricks_outward();
                return true;
            },
            Action::WindowTowardsRoot => {
                if !self.window {
                    return false;
                }
                let Some(parent) = &self.document.document_atom(self.window_atom).parent else {
                    return false;
                };
                let parent = parent.atom;
                self.window_exact(parent);
                self.trigger_idle_lay_bricks_outward();
                return true;
            },
            Action::WindowTowardsCursor => {
                if !self.window {
                    return false;
                }
                let Some(cursor) = self.cursor else {
                    return false;
                };
                let visual = match self.cursor_get(cursor) {
                    Cursor::Atom(c) => c.visual,
                    Cursor::Array(c) => c.visual,
                    Cursor::Primitive(c) => c.visual,
                };
                let Some(containing) = self.visual_containing_atom(visual) else {
                    return false;
                };
                let mut at = self.atom_id_of_visual(containing);
                let mut window_next = None;
                while at != self.window_atom {
                    let Some(parent) = &self.document.document_atom(at).parent else {
                        break;
                    };
                    window_next = Some(at);
                    at = parent.atom;
                }
                let Some(window_next) = window_next else {
                    return false;
                };
                self.window_exact(window_next);
                self.trigger_idle_lay_bricks_outward();
                return true;
            },
            Action::ScrollNext => {
                self.context_scroll_by(-self.config.scroll_factor * self.transverse_edge);
                return true;
            },
            Action::ScrollNextAlot => {
                self.context_scroll_by(-self.config.scroll_alot_factor * self.transverse_edge);
                return true;
            },
            Action::ScrollPrevious => {
                self.context_scroll_by(self.config.scroll_factor * self.transverse_edge);
                return true;
            },
            Action::ScrollPreviousAlot => {
                self.context_scroll_by(self.config.scroll_alot_factor * self.transverse_edge);
                return true;
            },
            Action::ScrollReset => {
                self.scroll_follow = true;
                if let Some(c) = self.cursor {
                    match self.cursor_get(c) {
                        Cursor::Atom(_) => self.cursor_atom_reset_cornerstone(c),
                        Cursor::Array(ca) => {
                            let (begin, end) = (ca.begin_index, ca.end_index);
                            self.cursor_array_set_range(c, begin, end);
                        },
                        Cursor::Primitive(cp) => {
                            let (visual, lead) = (cp.range.visual, cp.range.range_lead_index());
                            let line = self.primitive_find_containing(visual, lead);
                            self.range_set_cornerstone(RangeLoc::Cursor(c), line);
                        },
                    }
                }
                self.scroll_visible();
                return true;
            },
            Action::Delete |
            Action::Cut |
            Action::Suffix |
            Action::InsertBefore |
            Action::InsertAfter |
            Action::MoveBefore |
            Action::MoveAfter |
            Action::DeletePrevious |
            Action::DeleteNext |
            Action::SplitLines |
            Action::JoinLines |
            Action::ChoiceNext |
            Action::ChoicePrevious |
            Action::Choose => return self.edit_action(
                action,
            ),
            _ => { },
        }
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
                    Action::Exit => {
                        let atom = self.visual_atom(visual).atom;
                        return self.atom_parent_select_field(atom);
                    },
                    Action::NextElement => return select(self, (index + 1) % count),
                    Action::PreviousElement => return select(self, (index + count - 1) % count),
                    Action::Copy => {
                        self.cursor_copy();
                        return true;
                    },
                    Action::Window => {
                        let atom = self.visual_atom(visual).atom;
                        let field = self.visual_atom(visual).selectable[index].0.clone();
                        let child = match self.document.document_atom(atom).fields.get(&field) {
                            Some(Field::Atom(child)) => Some(*child),
                            _ => None,
                        };
                        let Some(child) = child else {
                            self.window_exact(atom);
                            self.trigger_idle_lay_bricks_outward();
                            return true;
                        };
                        let type_ = self.document.document_atom(child).type_;
                        let front = &self.syntax.syntax_type(type_).front;
                        if front.is_empty() || front.iter().any(|f| matches!(f, Front::Symbol(_))) {
                            return false;
                        }
                        self.window_exact(child);
                        self.trigger_idle_lay_bricks_outward();
                        let body = self.atom_visual[child].unwrap();
                        self.visual_select_into_any_child(body);
                        return true;
                    },
                    _ => return false,
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
                let set_begin = |ctx: &mut Context, new: usize| -> bool {
                    if new == begin {
                        return false;
                    }
                    ctx.cursor_array_set_begin(cursor, new);
                    return true;
                };
                let set_end = |ctx: &mut Context, new: usize| -> bool {
                    if new == end {
                        return false;
                    }
                    ctx.cursor_array_set_end(cursor, new);
                    return true;
                };
                match action {
                    Action::Enter => {
                        let element = self.array_element_visual(visual, begin);
                        return self.visual_select_into_any_child(element);
                    },
                    Action::Exit => {
                        let (atom, field) = self.array_field(visual);
                        return self.field_parent_select_parent(atom, &field);
                    },
                    Action::NextElement => return position(self, (end + 1) % count),
                    Action::PreviousElement => return position(self, (begin + count - 1) % count),
                    Action::FirstElement => return position(self, 0),
                    Action::LastElement => return position(self, count - 1),
                    Action::GatherNext => return set_end(self, (end + 1).min(count - 1)),
                    Action::GatherPrevious => return set_begin(self, begin.saturating_sub(1)),
                    Action::GatherFirst => return set_begin(self, 0),
                    Action::GatherLast => return set_end(self, count - 1),
                    Action::ReleaseAll => {
                        if begin == end {
                            return false;
                        }
                        if lead_first {
                            self.cursor_array_set_end(cursor, begin);
                        } else {
                            self.cursor_array_set_begin(cursor, end);
                        }
                        return true;
                    },
                    Action::ReleaseNext => return set_end(self, begin.max(end - 1)),
                    Action::ReleasePrevious => return set_begin(self, end.min(begin + 1)),
                    Action::SelectNext => {
                        if lead_first && begin != end {
                            return set_begin(self, end.min(begin + 1));
                        }
                        return set_end(self, (end + 1).min(count - 1));
                    },
                    Action::SelectPrevious => {
                        if !lead_first && begin != end {
                            return set_end(self, begin.max(end - 1));
                        }
                        return set_begin(self, begin.saturating_sub(1));
                    },
                    Action::Copy => {
                        self.cursor_copy();
                        return true;
                    },
                    Action::Window => {
                        let element = self.array_element_visual(visual, begin);
                        if !self.visual_select_into_any_child(element) {
                            return false;
                        }
                        let atom = self.array_elements(visual)[begin];
                        self.window_exact(atom);
                        self.trigger_idle_lay_bricks_outward();
                        return true;
                    },
                    _ => return false,
                }
            },
            Cursor::Primitive(c) => {
                let (visual, begin, end, lead_first) =
                    (c.visual, c.range.begin_offset, c.range.end_offset, c.range.lead_first);
                let loc = RangeLoc::Cursor(cursor);
                let text = self.visual_primitive(visual).value.clone();
                let glyphs = self.environment.environment_glyph_walker(&text);
                let words = self.environment.environment_word_walker(&text);
                let point = |ctx: &mut Context, new: usize| -> bool {
                    if new == begin && new == end {
                        return false;
                    }
                    ctx.range_set_offsets(loc, new, new);
                    return true;
                };
                let set_begin = |ctx: &mut Context, new: usize| -> bool {
                    if new == begin {
                        return false;
                    }
                    ctx.range_set_begin_offset(loc, new);
                    return true;
                };
                let set_end = |ctx: &mut Context, new: usize| -> bool {
                    if new == end {
                        return false;
                    }
                    ctx.range_set_end_offset(loc, new);
                    return true;
                };
                match action {
                    Action::NextGlyph => return point(self, glyphs.glyph_after(end)),
                    Action::PreviousGlyph => return point(self, glyphs.glyph_before(begin)),
                    Action::NextWord => return point(self, words.word_start_after(end)),
                    Action::PreviousWord => return point(self, words.word_start_before(begin)),
                    Action::FirstGlyph => return point(self, 0),
                    Action::LastGlyph => return point(self, text.len()),
                    Action::LineBegin => {
                        let (start, _) = self.primitive_line_bounds(visual, begin);
                        return point(self, start);
                    },
                    Action::LineEnd => {
                        let (_, stop) = self.primitive_line_bounds(visual, end);
                        return point(self, stop);
                    },
                    Action::NextLine => {
                        let new = self.primitive_line_shift(visual, end, true);
                        return point(self, new);
                    },
                    Action::PreviousLine => {
                        let new = self.primitive_line_shift(visual, begin, false);
                        return point(self, new);
                    },
                    Action::Exit => {
                        if self.config.editable && self.gap_cursor().is_some() {
                            return self.gap_exit();
                        }
                        let (atom, field) = self.primitive_field(visual);
                        return self.field_parent_select_parent(atom, &field);
                    },
                    Action::GatherFirst => return set_begin(self, 0),
                    Action::GatherLast => return set_end(self, text.len()),
                    Action::GatherNextGlyph => return set_end(self, glyphs.glyph_after(end)),
                    Action::GatherPreviousGlyph => return set_begin(self, glyphs.glyph_before(begin)),
                    Action::GatherNextWord => return set_end(self, words.word_end_after(end)),
                    Action::GatherPreviousWord => return set_begin(self, words.word_start_before(begin)),
                    Action::GatherNextLine => {
                        let new = self.primitive_line_shift(visual, end, true);
                        return set_end(self, new);
                    },
                    Action::GatherPreviousLine => {
                        let new = self.primitive_line_shift(visual, begin, false);
                        return set_begin(self, new);
                    },
                    Action::GatherNextLineEnd => {
                        let (_, stop) = self.primitive_line_bounds(visual, end);
                        return set_end(self, stop);
                    },
                    Action::GatherPreviousLineStart => {
                        let (start, _) = self.primitive_line_bounds(visual, begin);
                        return set_begin(self, start);
                    },
                    Action::ReleaseAll => {
                        if begin == end {
                            return false;
                        }
                        if lead_first {
                            self.range_set_offsets(loc, begin, begin);
                        } else {
                            self.range_set_offsets(loc, end, end);
                        }
                        return true;
                    },
                    Action::ReleaseNextGlyph => return set_end(self, begin.max(glyphs.glyph_before(end))),
                    Action::ReleasePreviousGlyph => return set_begin(self, end.min(glyphs.glyph_after(begin))),
                    Action::ReleaseNextWord => return set_end(self, begin.max(words.word_start_before(end))),
                    Action::ReleasePreviousWord => return set_begin(self, end.min(words.word_start_after(begin))),
                    Action::SelectNextGlyph => {
                        if lead_first && begin != end {
                            return set_begin(self, end.min(glyphs.glyph_after(begin)));
                        }
                        return set_end(self, glyphs.glyph_after(end));
                    },
                    Action::SelectPreviousGlyph => {
                        if !lead_first && begin != end {
                            return set_end(self, begin.max(glyphs.glyph_before(end)));
                        }
                        return set_begin(self, glyphs.glyph_before(begin));
                    },
                    Action::SelectNextWord => {
                        if lead_first && begin != end {
                            return set_begin(self, end.min(words.word_start_after(begin)));
                        }
                        return set_end(self, words.word_end_after(end));
                    },
                    Action::SelectPreviousWord => {
                        if !lead_first && begin != end {
                            return set_end(self, begin.max(words.word_start_before(end)));
                        }
                        return set_begin(self, words.word_start_before(begin));
                    },
                    Action::ReleaseNextLine => {
                        let new = self.primitive_line_shift(visual, end, false);
                        return set_end(self, begin.max(new));
                    },
                    Action::ReleasePreviousLine => {
                        let new = self.primitive_line_shift(visual, begin, true);
                        return set_begin(self, end.min(new));
                    },
                    Action::ReleaseNextLineEnd => {
                        let (start, _) = self.primitive_line_bounds(visual, end);
                        return set_end(self, begin.max(start));
                    },
                    Action::ReleasePreviousLineStart => {
                        let (_, stop) = self.primitive_line_bounds(visual, begin);
                        return set_begin(self, end.min(stop));
                    },
                    Action::Copy => {
                        self.cursor_copy();
                        return true;
                    },
                    _ => return false,
                }
            },
        }
    }

    pub fn key_resolve(&mut self, stroke: KeyStroke) -> KeyResolve {
        let mut pending = std::mem::take(&mut self.key_pending);
        let mut cursor = self.cursor.map(|c| self.cursor_get(c).cursor_kind());
        if cursor == Some(CursorKind::Primitive) && self.gap_cursor().is_some() {
            cursor = Some(CursorKind::Gap);
        }
        let typing = self.config.editable && matches!(cursor, Some(CursorKind::Primitive | CursorKind::Gap));
        let resolved = self.config.keys.keymap_read(&mut pending, stroke, cursor, typing);
        self.key_pending = pending;
        return resolved;
    }

    fn primitive_line_at(&self, visual: VisualId, offset: usize) -> Option<usize> {
        let count = self.visual_primitive(visual).lines.len();
        if count == 0 {
            return None;
        }
        return Some(self.primitive_find_containing(visual, offset).min(count - 1));
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
}
