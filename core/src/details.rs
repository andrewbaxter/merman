use crate::{
    attachment::DrawingLayer,
    context::Context,
    wall::Bedding,
};

impl Context {
    pub fn details_close(&mut self) {
        let Some((node, bedding, _)) = self.details.take() else {
            return;
        };
        let overlay = self.overlay_layer;
        self.display.group_remove_node(overlay, node);
        self.display.display_destroy(node);
        self.wall_remove_bedding(bedding);
    }

    pub fn details_open(&mut self) {
        if self.details.is_some() {
            return;
        }
        let (node, transverse_span) = self.display.display_details();
        let at = self.drawings.iter().flatten().filter(|d| d.layer == DrawingLayer::Overlay).count();
        let overlay = self.overlay_layer;
        self.display.group_add(overlay, at, node);
        let bedding = self.wall_add_bedding(Bedding {
            before: 0.,
            after: transverse_span + self.syntax.spec_root.course_transverse_gap,
        });
        self.details = Some((node, bedding, transverse_span));
        self.details_place();
    }

    pub fn details_place(&mut self) {
        let Some((node, _, transverse_span)) = self.details else {
            return;
        };
        let pad = &self.syntax.spec_root.pad;
        let (pad_start, pad_end) = (pad.converse_start.round(), pad.converse_end.round());
        self.display.node_set_span(node, self.edge + pad_start + pad_end, transverse_span, 0.);
        self
            .display
            .node_set_position(
                node,
                -pad_start,
                (self.scroll_end + self.syntax.spec_root.course_transverse_gap).round(),
                false,
            );
    }
}
