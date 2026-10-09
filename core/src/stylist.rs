use {
    crate::{
        spec::{
            SpecMark,
            SpecObbox,
        },
        syntax::{
            Style,
            StyleId,
            Syntax,
        },
    },
    std::rc::Rc,
};

pub trait Stylist {
    fn style_empty(&self, style: StyleId) -> Style;
    fn style_mark(&self) -> SpecMark;
    fn style_obbox(&self, type_: ObboxType) -> SpecObbox;
    fn style_text(&self, style: StyleId) -> Style;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObboxType {
    Cursor,
    Hover,
}

pub struct StylistDirect {
    pub syntax: Rc<Syntax>,
}

impl Stylist for StylistDirect {
    fn style_empty(&self, style: StyleId) -> Style {
        return self.syntax.syntax_style(style).clone();
    }

    fn style_mark(&self) -> SpecMark {
        return self.syntax.spec_root.error_mark.clone();
    }

    fn style_obbox(&self, type_: ObboxType) -> SpecObbox {
        match type_ {
            ObboxType::Hover => return self.syntax.spec_root.hover.clone(),
            ObboxType::Cursor => return self.syntax.spec_root.cursor.clone(),
        }
    }

    fn style_text(&self, style: StyleId) -> Style {
        return self.syntax.syntax_style(style).clone();
    }
}
