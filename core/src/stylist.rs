use crate::spec::SpecObbox;
use crate::syntax::{
    Style,
    StyleId,
    Syntax,
};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObboxType {
    Hover,
    Cursor,
}

pub trait Stylist {
    fn style_text(&self, style: StyleId) -> Style;
    fn style_empty(&self, style: StyleId) -> Style;
    fn style_obbox(&self, type_: ObboxType) -> SpecObbox;
}

pub struct StylistDirect {
    pub syntax: Rc<Syntax>,
}

impl Stylist for StylistDirect {
    fn style_text(&self, style: StyleId) -> Style {
        return self.syntax.syntax_style(style).clone();
    }

    fn style_empty(&self, style: StyleId) -> Style {
        return self.syntax.syntax_style(style).clone();
    }

    fn style_obbox(&self, type_: ObboxType) -> SpecObbox {
        match type_ {
            ObboxType::Hover => return self.syntax.spec_root.hover.clone(),
            ObboxType::Cursor => return self.syntax.spec_root.cursor.clone(),
        }
    }
}
