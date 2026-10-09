use crate::{
    context::{
        BorderId,
        BrickId,
        CaretId,
        Context,
        DrawingId,
        MarkId,
        TextBorderId,
        Vector,
    },
    display::{
        DisplayNodeId,
        DrawCommand,
        obbox_commands,
    },
    document::AtomId,
    spec::{
        SpecMark,
        SpecObbox,
    },
};

fn aeq(a: f64, b: f64, t: f64) -> bool {
    return (a - b) * (a - b) < t * t;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttachmentRef {
    BorderFirst(BorderId),
    BorderLast(BorderId),
    Caret(CaretId),
    Cornerstone,
    Mark(MarkId),
    TextBorderFirst(TextBorderId),
    TextBorderLast(TextBorderId),
}

pub struct Border {
    pub drawing: DrawingId,
    end_transverse: f64,
    end_transverse_span: f64,
    pub first: Option<BrickId>,
    pub last: Option<BrickId>,
    start_converse: f64,
    start_transverse: f64,
    start_transverse_span: f64,
    style: SpecObbox,
}

pub struct Caret {
    pub brick: Option<BrickId>,
    pub drawing: DrawingId,
    index: usize,
    offset: Option<Vector>,
    size: Vector,
    start_converse: f64,
    start_transverse: f64,
    style: SpecObbox,
    transverse_ascent: f64,
}

pub struct Drawing {
    pub layer: DrawingLayer,
    pub node: DisplayNodeId,
}

pub struct Mark {
    pub atom: AtomId,
    pub brick: Option<BrickId>,
    pub drawing: DrawingId,
    start_converse: f64,
    start_transverse: f64,
    style: SpecMark,
    transverse_span: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum DrawingLayer {
    Background,
    Hover,
    Overlay,
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

pub struct TextBorder {
    block_redraw: bool,
    pub drawing: DrawingId,
    end_converse: f64,
    end_transverse: f64,
    end_transverse_span: f64,
    pub first: Option<BrickId>,
    first_index: usize,
    pub last: Option<BrickId>,
    last_index: usize,
    start_converse: f64,
    start_transverse: f64,
    start_transverse_span: f64,
    style: SpecObbox,
}

impl Context {
    pub fn attachment_destroy(&mut self, a: AttachmentRef) {
        match a {
            AttachmentRef::BorderFirst(b) => {
                if let Some(border) = self.borders[b].as_mut() {
                    border.first = None;
                }
            },
            AttachmentRef::BorderLast(b) => {
                if let Some(border) = self.borders[b].as_mut() {
                    border.last = None;
                }
            },
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
            AttachmentRef::Mark(m) => {
                if let Some(mark) = self.marks[m].as_mut() {
                    mark.brick = None;
                }
                self.mark_place(m);
            },
            AttachmentRef::Cornerstone => { },
        }
    }

    pub fn attachment_set_baseline_transverse(&mut self, _a: AttachmentRef, _baseline: f64) { }

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
            AttachmentRef::Mark(m) => {
                self.marks[m].as_mut().unwrap().start_converse = converse;
                self.mark_place(m);
            },
            AttachmentRef::Cornerstone => { },
        }
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
            AttachmentRef::Mark(m) => {
                self.marks[m].as_mut().unwrap().start_transverse = transverse;
                self.mark_place(m);
            },
            AttachmentRef::Cornerstone => {
                let old = self.scroll_start;
                self.scroll_start = transverse;
                self.scroll_end += self.scroll_start - old;
                self.details_place();
                self.scroll_visible();
            },
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
            AttachmentRef::Mark(m) => {
                self.marks[m].as_mut().unwrap().transverse_span = ascent + descent;
                self.mark_place(m);
            },
            AttachmentRef::Cornerstone => {
                self.scroll_end = self.scroll_start + ascent + descent;
                self.details_place();
                self.scroll_visible();
            },
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

    pub fn border_new(&mut self, style: SpecObbox, layer: DrawingLayer) -> BorderId {
        let drawing = self.drawing_new(layer);
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
        self.obbox_place(drawing, &points, &style);
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

    pub fn caret_destroy(&mut self, c: CaretId) {
        let Some(caret) = self.carets[c].take() else {
            return;
        };
        if let Some(b) = caret.brick {
            self.brick_remove_attachment(b, AttachmentRef::Caret(c));
        }
        self.drawing_remove(caret.drawing);
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
                (start_converse + converse_offset + offset.converse).round(),
                (start_transverse + ascent + offset.transverse).round(),
            );
        let half_buffer = (style.line_thickness_px / 2. + 0.5).floor();
        let node = self.drawings[drawing].as_ref().unwrap().node;
        self.display.drawing_clear(node);
        self.display.drawing_resize(node, size);
        self.display.node_set_position(node, position.converse, position.transverse, false);
        let mut commands =
            vec![
                DrawCommand::SetLineThickness(style.line_thickness_px),
                if style.round_start {
                    DrawCommand::SetLineCapRound
                } else {
                    DrawCommand::SetLineCapFlat
                },
                DrawCommand::SetLineColor(style.line_color.clone()),
                DrawCommand::BeginStrokePath,
                DrawCommand::MoveTo(Vector::new(half_buffer, half_buffer)),
            ];
        commands.push(
            DrawCommand::LineTo(Vector::new(size.converse - half_buffer - 1., size.transverse - half_buffer - 1.)),
        );
        commands.push(DrawCommand::ClosePath);
        self.display.drawing_draw(node, &commands);
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
                let half_buffer = (caret.style.line_thickness_px / 2. + 0.5).floor();
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

    pub fn mark_atom_brick_created(&mut self, atom: AtomId, brick: BrickId) {
        let Some(marks) = self.atom_marks.get(&atom) else {
            return;
        };
        for m in marks.clone() {
            self.mark_set_brick(m, Some(brick));
        }
    }

    pub fn mark_atom(&self, m: MarkId) -> AtomId {
        return self.marks[m].as_ref().unwrap().atom;
    }

    pub fn mark_destroy(&mut self, m: MarkId) {
        let Some(mark) = self.marks[m].take() else {
            return;
        };
        if let Some(marks) = self.atom_marks.get_mut(&mark.atom) {
            marks.retain(|other| *other != m);
            if marks.is_empty() {
                self.atom_marks.remove(&mark.atom);
            }
        }
        if let Some(b) = mark.brick {
            self.brick_remove_attachment(b, AttachmentRef::Mark(m));
        }
        self.drawing_remove(mark.drawing);
    }

    pub fn mark_new(&mut self, atom: AtomId, style: SpecMark) -> MarkId {
        let drawing = self.drawing_new(DrawingLayer::Overlay);
        let id = self.marks.len();
        self.marks.push(Some(Mark {
            atom: atom,
            brick: None,
            drawing: drawing,
            start_converse: 0.,
            start_transverse: 0.,
            style: style,
            transverse_span: 0.,
        }));
        self.atom_marks.entry(atom).or_default().push(id);
        let brick = self.atom_visual[atom].and_then(|v| self.visual_get_first_brick(v));
        self.mark_set_brick(id, brick);
        return id;
    }

    fn mark_place(&mut self, m: MarkId) {
        let mark = self.marks[m].as_ref().unwrap();
        let node = self.drawings[mark.drawing].as_ref().unwrap().node;
        self.display.drawing_clear(node);
        if mark.brick.is_none() {
            return;
        }
        let (start_converse, start_transverse, transverse_span, style) =
            (mark.start_converse, mark.start_transverse, mark.transverse_span, mark.style.clone());
        let size = Vector::new(style.length, style.thickness);
        let position =
            Vector::new(start_converse.round(), (start_transverse + transverse_span - style.thickness).round());
        self.display.drawing_resize(node, size);
        self.display.node_set_position(node, position.converse, position.transverse, false);
        let middle = style.thickness / 2.;
        self
            .display
            .drawing_draw(
                node,
                &[
                    DrawCommand::SetLineThickness(style.thickness),
                    DrawCommand::SetLineCapFlat,
                    DrawCommand::SetLineColor(style.color),
                    DrawCommand::BeginStrokePath,
                    DrawCommand::MoveTo(Vector::new(0., middle)),
                    DrawCommand::LineTo(Vector::new(style.length, middle)),
                    DrawCommand::ClosePath,
                ],
            );
    }

    fn mark_set_brick(&mut self, m: MarkId, brick: Option<BrickId>) {
        let old = self.marks[m].as_ref().unwrap().brick;
        if old == brick {
            return;
        }
        if let Some(o) = old {
            self.brick_remove_attachment(o, AttachmentRef::Mark(m));
        }
        self.marks[m].as_mut().unwrap().brick = brick;
        match brick {
            Some(b) => self.brick_add_attachment(b, AttachmentRef::Mark(m)),
            None => self.mark_place(m),
        }
    }

    fn drawing_layer_group(&self, layer: DrawingLayer) -> DisplayNodeId {
        match layer {
            DrawingLayer::Background | DrawingLayer::Hover => return self.background_layer,
            DrawingLayer::Overlay => return self.overlay_layer,
        }
    }

    fn drawing_new(&mut self, layer: DrawingLayer) -> DrawingId {
        let id = self.drawings.len();
        let node = self.display.display_drawing();
        let group = self.drawing_layer_group(layer);
        let at =
            self
                .drawings
                .iter()
                .flatten()
                .filter(|d| self.drawing_layer_group(d.layer) == group && d.layer <= layer)
                .count();
        self.display.group_add(group, at, node);
        self.drawings.push(Some(Drawing {
            layer: layer,
            node: node,
        }));
        return id;
    }

    fn drawing_remove(&mut self, id: DrawingId) {
        let Some(drawing) = self.drawings[id].take() else {
            return;
        };
        let group = self.drawing_layer_group(drawing.layer);
        self.display.group_remove_node(group, drawing.node);
        self.display.display_destroy(drawing.node);
    }

    fn obbox_place(&mut self, drawing: DrawingId, points: &[(Vector, bool)], style: &SpecObbox) {
        let node = self.drawings[drawing].as_ref().unwrap().node;
        self.display.drawing_clear(node);
        if points.is_empty() {
            return;
        }
        let buffer = (style.line_thickness_px + 1.).floor();
        let points =
            points
                .iter()
                .map(|(point, round)| (Vector::new(point.converse.round(), point.transverse.round()), *round))
                .collect::<Vec<_>>();
        let mut min = points[0].0;
        let mut max = points[0].0;
        for (point, _) in &points {
            min.converse = min.converse.min(point.converse);
            min.transverse = min.transverse.min(point.transverse);
            max.converse = max.converse.max(point.converse);
            max.transverse = max.transverse.max(point.transverse);
        }
        let size =
            Vector::new(max.converse - min.converse + buffer * 2., max.transverse - min.transverse + buffer * 2.);
        self.display.drawing_resize(node, size);
        self.display.node_set_position(node, min.converse - buffer, min.transverse - buffer, false);
        let mut commands =
            vec![DrawCommand::Translate(Vector::new(buffer - min.converse, buffer - min.transverse))];
        commands.extend(obbox_commands(&points, style));
        self.display.drawing_draw(node, &commands);
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
        self.obbox_place(drawing, &points, &style);
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
}
