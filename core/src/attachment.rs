use crate::context::{
    BorderId,
    BrickId,
    CaretId,
    Context,
    DrawingId,
    TextBorderId,
    Vector,
};
use crate::spec::SpecObbox;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttachmentRef {
    BorderFirst(BorderId),
    BorderLast(BorderId),
    TextBorderFirst(TextBorderId),
    TextBorderLast(TextBorderId),
    Caret(CaretId),
    Cornerstone,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DrawingLayer {
    Background,
    Overlay,
}

pub enum DrawingKind {
    Obbox {
        points: Vec<(Vector, bool)>,
        style: SpecObbox,
    },
    Line {
        from: Vector,
        to: Vector,
        thickness: f64,
        color: String,
        round_cap: bool,
    },
}

pub struct Drawing {
    pub layer: DrawingLayer,
    pub kind: Option<DrawingKind>,
}

pub struct Border {
    pub first: Option<BrickId>,
    pub last: Option<BrickId>,
    start_converse: f64,
    start_transverse: f64,
    start_transverse_span: f64,
    end_transverse: f64,
    end_transverse_span: f64,
    style: SpecObbox,
    pub drawing: DrawingId,
}

pub struct TextBorder {
    pub first: Option<BrickId>,
    first_index: usize,
    pub last: Option<BrickId>,
    last_index: usize,
    start_converse: f64,
    start_transverse: f64,
    start_transverse_span: f64,
    end_converse: f64,
    end_transverse: f64,
    end_transverse_span: f64,
    block_redraw: bool,
    style: SpecObbox,
    pub drawing: DrawingId,
}

pub struct Caret {
    pub brick: Option<BrickId>,
    index: usize,
    start_converse: f64,
    start_transverse: f64,
    transverse_ascent: f64,
    style: SpecObbox,
    pub drawing: DrawingId,
    offset: Option<Vector>,
    size: Vector,
}

impl Context {
    fn drawing_new(&mut self, layer: DrawingLayer) -> DrawingId {
        let id = self.drawings.len();
        self.drawings.push(Some(Drawing {
            layer: layer,
            kind: None,
        }));
        return id;
    }

    fn drawing_remove(&mut self, id: DrawingId) {
        self.drawings[id] = None;
    }

    pub fn attachment_set_transverse(&mut self, a: AttachmentRef, transverse: f64) {
        match a {
            AttachmentRef::BorderFirst(b) => {
                self.borders[b].as_mut().unwrap().start_transverse = transverse;
                self.border_redraw(b);
            },
            AttachmentRef::BorderLast(b) => {
                self.borders[b].as_mut().unwrap().end_transverse = transverse;
                self.border_redraw(b);
            },
            AttachmentRef::TextBorderFirst(b) => {
                self.text_borders[b].as_mut().unwrap().start_transverse = transverse;
                self.text_border_redraw(b);
            },
            AttachmentRef::TextBorderLast(b) => {
                self.text_borders[b].as_mut().unwrap().end_transverse = transverse;
                self.text_border_redraw(b);
            },
            AttachmentRef::Caret(c) => {
                self.carets[c].as_mut().unwrap().start_transverse = transverse;
                self.caret_place(c);
            },
            AttachmentRef::Cornerstone => {
                let old = self.scroll_start;
                self.scroll_start = transverse;
                self.scroll_end += self.scroll_start - old;
                self.scroll_visible();
            },
        }
    }

    pub fn attachment_set_converse(&mut self, a: AttachmentRef, converse: f64) {
        match a {
            AttachmentRef::BorderFirst(b) => {
                self.borders[b].as_mut().unwrap().start_converse = converse;
                self.border_redraw(b);
            },
            AttachmentRef::BorderLast(b) => self.border_redraw(b),
            AttachmentRef::TextBorderFirst(b) => {
                self.text_borders[b].as_mut().unwrap().start_converse = converse;
                self.text_border_redraw(b);
            },
            AttachmentRef::TextBorderLast(b) => {
                self.text_borders[b].as_mut().unwrap().end_converse = converse;
                self.text_border_redraw(b);
            },
            AttachmentRef::Caret(c) => {
                self.carets[c].as_mut().unwrap().start_converse = converse;
                self.caret_place(c);
            },
            AttachmentRef::Cornerstone => { },
        }
    }

    pub fn attachment_set_transverse_span(&mut self, a: AttachmentRef, ascent: f64, descent: f64) {
        match a {
            AttachmentRef::BorderFirst(b) => {
                self.borders[b].as_mut().unwrap().start_transverse_span = ascent + descent;
                self.border_redraw(b);
            },
            AttachmentRef::BorderLast(b) => {
                self.borders[b].as_mut().unwrap().end_transverse_span = ascent + descent;
                self.border_redraw(b);
            },
            AttachmentRef::TextBorderFirst(b) => {
                self.text_borders[b].as_mut().unwrap().start_transverse_span = ascent + descent;
                self.text_border_redraw(b);
            },
            AttachmentRef::TextBorderLast(b) => {
                self.text_borders[b].as_mut().unwrap().end_transverse_span = ascent + descent;
                self.text_border_redraw(b);
            },
            AttachmentRef::Caret(c) => {
                self.carets[c].as_mut().unwrap().transverse_ascent = ascent;
                self.caret_place(c);
            },
            AttachmentRef::Cornerstone => {
                self.scroll_end = self.scroll_start + ascent + descent;
                self.scroll_visible();
            },
        }
    }

    pub fn attachment_set_baseline_transverse(&mut self, _a: AttachmentRef, _baseline: f64) { }

    pub fn attachment_destroy(&mut self, a: AttachmentRef) {
        match a {
            AttachmentRef::BorderFirst(b) => {
                if let Some(border) = self.borders[b].as_mut() {
                    border.first = None;
                }
            },
            AttachmentRef::BorderLast(_) => { },
            AttachmentRef::TextBorderFirst(b) => {
                if let Some(border) = self.text_borders[b].as_mut() {
                    border.first = None;
                }
            },
            AttachmentRef::TextBorderLast(b) => {
                if let Some(border) = self.text_borders[b].as_mut() {
                    border.last = None;
                }
            },
            AttachmentRef::Caret(c) => {
                if let Some(caret) = self.carets[c].as_mut() {
                    caret.brick = None;
                    caret.offset = None;
                }
            },
            AttachmentRef::Cornerstone => { },
        }
    }

    pub fn border_new(&mut self, style: SpecObbox) -> BorderId {
        let drawing = self.drawing_new(DrawingLayer::Background);
        let id = self.borders.len();
        self.borders.push(Some(Border {
            first: None,
            last: None,
            start_converse: 0.,
            start_transverse: 0.,
            start_transverse_span: 0.,
            end_transverse: 0.,
            end_transverse_span: 0.,
            style: style,
            drawing: drawing,
        }));
        return id;
    }

    pub fn border_set_first(&mut self, b: BorderId, first: Option<BrickId>) {
        if let Some(old) = self.borders[b].as_ref().unwrap().first {
            self.brick_remove_attachment(old, AttachmentRef::BorderFirst(b));
        }
        self.borders[b].as_mut().unwrap().first = first;
        if let Some(f) = first {
            self.brick_add_attachment(f, AttachmentRef::BorderFirst(b));
        }
    }

    pub fn border_set_last(&mut self, b: BorderId, last: Option<BrickId>) {
        if let Some(old) = self.borders[b].as_ref().unwrap().last {
            self.brick_remove_attachment(old, AttachmentRef::BorderLast(b));
        }
        self.borders[b].as_mut().unwrap().last = last;
        if let Some(l) = last {
            self.brick_add_attachment(l, AttachmentRef::BorderLast(b));
        }
    }

    pub fn border_destroy(&mut self, b: BorderId) {
        let Some(border) = self.borders[b].take() else {
            return;
        };
        if let Some(f) = border.first {
            self.brick_remove_attachment(f, AttachmentRef::BorderFirst(b));
        }
        if let Some(l) = border.last {
            self.brick_remove_attachment(l, AttachmentRef::BorderLast(b));
        }
        self.drawing_remove(border.drawing);
    }

    fn border_redraw(&mut self, b: BorderId) {
        let border = self.borders[b].as_ref().unwrap();
        let (Some(first), Some(last)) = (border.first, border.last) else {
            return;
        };
        let one_line = self.bricks[first].course.is_some() && self.bricks[first].course == self.bricks[last].course;
        let last_edge = self.brick_converse_edge(last);
        let points =
            obbox_points(
                &border.style,
                self.edge,
                one_line,
                border.start_converse,
                border.start_transverse,
                border.start_transverse + border.start_transverse_span,
                last_edge,
                border.end_transverse,
                border.end_transverse + border.end_transverse_span,
            );
        let style = border.style.clone();
        let drawing = border.drawing;
        self.drawings[drawing].as_mut().unwrap().kind = Some(DrawingKind::Obbox {
            points: points,
            style: style,
        });
    }

    pub fn text_border_new(&mut self, style: SpecObbox) -> TextBorderId {
        let drawing = self.drawing_new(DrawingLayer::Background);
        let id = self.text_borders.len();
        self.text_borders.push(Some(TextBorder {
            first: None,
            first_index: 0,
            last: None,
            last_index: 0,
            start_converse: 0.,
            start_transverse: 0.,
            start_transverse_span: 0.,
            end_converse: 0.,
            end_transverse: 0.,
            end_transverse_span: 0.,
            block_redraw: false,
            style: style,
            drawing: drawing,
        }));
        return id;
    }

    pub fn text_border_set_both(
        &mut self,
        b: TextBorderId,
        first: Option<BrickId>,
        first_index: usize,
        last: Option<BrickId>,
        last_index: usize,
    ) {
        let (old_first, old_last) = {
            let tb = self.text_borders[b].as_ref().unwrap();
            (tb.first, tb.last)
        };
        if let Some(of) = old_first {
            if old_first != first {
                self.brick_remove_attachment(of, AttachmentRef::TextBorderFirst(b));
            }
        }
        if let Some(ol) = old_last {
            if old_last != last {
                self.brick_remove_attachment(ol, AttachmentRef::TextBorderLast(b));
            }
        }
        {
            let tb = self.text_borders[b].as_mut().unwrap();
            tb.first = first;
            tb.last = last;
            tb.first_index = first_index;
            tb.last_index = last_index;
            tb.block_redraw = true;
        }
        if let Some(f) = first {
            self.brick_add_attachment(f, AttachmentRef::TextBorderFirst(b));
        }
        if let Some(l) = last {
            self.brick_add_attachment(l, AttachmentRef::TextBorderLast(b));
        }
        self.text_borders[b].as_mut().unwrap().block_redraw = false;
        self.text_border_redraw(b);
    }

    pub fn text_border_destroy(&mut self, b: TextBorderId) {
        let Some(tb) = self.text_borders[b].take() else {
            return;
        };
        if let Some(f) = tb.first {
            self.brick_remove_attachment(f, AttachmentRef::TextBorderFirst(b));
        }
        if let Some(l) = tb.last {
            self.brick_remove_attachment(l, AttachmentRef::TextBorderLast(b));
        }
        self.drawing_remove(tb.drawing);
    }

    fn text_border_redraw(&mut self, b: TextBorderId) {
        let (first, last, first_index, last_index, block) = {
            let tb = self.text_borders[b].as_ref().unwrap();
            (tb.first, tb.last, tb.first_index, tb.last_index, tb.block_redraw)
        };
        let (Some(first), Some(last)) = (first, last) else {
            return;
        };
        if block {
            return;
        }
        let first_offset = self.brick_text_get_converse_offset(first, first_index);
        let last_offset = self.brick_text_get_converse_offset(last, last_index);
        let one_line = self.bricks[first].course.is_some() && self.bricks[first].course == self.bricks[last].course;
        let tb = self.text_borders[b].as_ref().unwrap();
        let points =
            obbox_points(
                &tb.style,
                self.edge,
                one_line,
                tb.start_converse + first_offset,
                tb.start_transverse,
                tb.start_transverse + tb.start_transverse_span,
                tb.end_converse + last_offset,
                tb.end_transverse,
                tb.end_transverse + tb.end_transverse_span,
            );
        let style = tb.style.clone();
        let drawing = tb.drawing;
        self.drawings[drawing].as_mut().unwrap().kind = Some(DrawingKind::Obbox {
            points: points,
            style: style,
        });
    }

    pub fn caret_new(&mut self, style: SpecObbox) -> CaretId {
        let drawing = self.drawing_new(DrawingLayer::Overlay);
        let id = self.carets.len();
        self.carets.push(Some(Caret {
            brick: None,
            index: 0,
            start_converse: 0.,
            start_transverse: 0.,
            transverse_ascent: 0.,
            style: style,
            drawing: drawing,
            offset: None,
            size: Vector::default(),
        }));
        return id;
    }

    pub fn caret_set_position(&mut self, c: CaretId, brick: Option<BrickId>, index: usize) {
        let old = self.carets[c].as_ref().unwrap().brick;
        if old != brick {
            self.carets[c].as_mut().unwrap().offset = None;
            if let Some(o) = old {
                self.brick_remove_attachment(o, AttachmentRef::Caret(c));
            }
            self.carets[c].as_mut().unwrap().brick = brick;
            let Some(b) = brick else {
                return;
            };
            self.brick_add_attachment(b, AttachmentRef::Caret(c));
            {
                let brick = self.carets[c].as_ref().unwrap().brick.unwrap();
                let ascent = (self.bricks[brick].ascent * 1.8).floor();
                let descent = (self.bricks[brick].descent * 1.8).floor();
                let caret = self.carets[c].as_mut().unwrap();
                let half_buffer = (caret.style.line_thickness / 2. + 0.5).floor();
                let buffer = half_buffer * 2.;
                caret.size = Vector::new(buffer + 1., ascent + if caret.style.round_start {
                    buffer
                } else {
                    0.
                });
                caret.offset = Some(Vector::new(half_buffer, -ascent + descent - if caret.style.round_start {
                    half_buffer
                } else {
                    0.
                }));
            }
        }
        self.carets[c].as_mut().unwrap().index = index;
        self.caret_place(c);
    }

    fn caret_place(&mut self, c: CaretId) {
        let caret = self.carets[c].as_ref().unwrap();
        let (Some(offset), Some(brick)) = (caret.offset, caret.brick) else {
            return;
        };
        let (index, start_converse, start_transverse, ascent, size, style, drawing) =
            (
                caret.index,
                caret.start_converse,
                caret.start_transverse,
                caret.transverse_ascent,
                caret.size,
                caret.style.clone(),
                caret.drawing,
            );
        let converse_offset = self.brick_text_get_converse_offset(brick, index);
        let position =
            Vector::new(
                start_converse + converse_offset + offset.converse,
                start_transverse + ascent + offset.transverse,
            );
        let half_buffer = (style.line_thickness / 2. + 0.5).floor();
        let from = Vector::new(position.converse + half_buffer, position.transverse + half_buffer);
        let to =
            Vector::new(
                position.converse + size.converse - half_buffer - 1.,
                position.transverse + size.transverse - half_buffer - 1.,
            );
        self.drawings[drawing].as_mut().unwrap().kind = Some(DrawingKind::Line {
            from: from,
            to: to,
            thickness: style.line_thickness,
            color: style.line_color.clone(),
            round_cap: style.round_start,
        });
    }

    pub fn caret_destroy(&mut self, c: CaretId) {
        let Some(caret) = self.carets[c].take() else {
            return;
        };
        if let Some(b) = caret.brick {
            self.brick_remove_attachment(b, AttachmentRef::Caret(c));
        }
        self.drawing_remove(caret.drawing);
    }
}

fn aeq(a: f64, b: f64, t: f64) -> bool {
    return (a - b) * (a - b) < t * t;
}

pub fn obbox_points(
    style: &SpecObbox,
    edge: f64,
    one_line: bool,
    mut first_line_converse: f64,
    mut first_line_transverse: f64,
    mut first_line_transverse_end: f64,
    mut last_line_converse_end: f64,
    mut last_line_transverse: f64,
    mut last_line_transverse_end: f64,
) -> Vec<(Vector, bool)> {
    let pad = &style.padding;
    first_line_converse -= pad.converse_start;
    first_line_transverse -= pad.transverse_start;
    first_line_transverse_end = if one_line {
        first_line_transverse_end + pad.transverse_end
    } else {
        first_line_transverse_end - pad.transverse_start
    };
    last_line_converse_end += pad.converse_end;
    last_line_transverse += pad.transverse_end;
    last_line_transverse_end += pad.transverse_end;
    let converse_zero = -pad.converse_start;
    let converse_edge = edge + pad.converse_end;
    if one_line {
        return vec![
            (Vector::new(first_line_converse, first_line_transverse), style.round_start),
            (Vector::new(last_line_converse_end, first_line_transverse), style.round_outer_corners),
            (Vector::new(last_line_converse_end, first_line_transverse_end), style.round_end),
            (Vector::new(first_line_converse, first_line_transverse_end), style.round_outer_corners),
        ];
    }
    let mut points = vec![];
    points.push((Vector::new(first_line_converse, first_line_transverse), style.round_start));
    points.push((Vector::new(converse_edge, first_line_transverse), style.round_outer_corners));
    if !aeq(last_line_converse_end, converse_edge, 5.) {
        points.push((Vector::new(converse_edge, last_line_transverse), style.round_inner_corners));
        points.push((Vector::new(last_line_converse_end, last_line_transverse), style.round_concave));
    }
    points.push((Vector::new(last_line_converse_end, last_line_transverse_end), style.round_end));
    points.push((Vector::new(converse_zero, last_line_transverse_end), style.round_outer_corners));
    if !aeq(first_line_converse, converse_zero, 5.) {
        points.push((Vector::new(converse_zero, first_line_transverse_end), style.round_inner_corners));
        points.push((Vector::new(first_line_converse, first_line_transverse_end), style.round_concave));
    }
    return points;
}
