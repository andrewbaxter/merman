//! Conversion between layout coordinates (converse along a line, transverse across
//! lines) and page coordinates, for every direction pair (ported from merman's
//! `Display`).
use crate::spec::SpecDirection;

#[derive(Debug, Clone, Copy)]
pub struct DirectionConvert {
    pub converse: SpecDirection,
    pub transverse: SpecDirection,
}

/// A page position along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnconvertAxis {
    /// True if the amount is an x coordinate, false for y.
    pub x: bool,
    pub amount: f64,
}

impl DirectionConvert {
    pub fn new(converse: SpecDirection, transverse: SpecDirection) -> DirectionConvert {
        return DirectionConvert {
            converse: converse,
            transverse: transverse,
        };
    }

    pub fn direction_converse_vertical(&self) -> bool {
        return matches!(self.converse, SpecDirection::Up | SpecDirection::Down);
    }

    /// Page top-left of a box at (converse, transverse) whose page size is (x_span,
    /// y_span).
    pub fn direction_unconvert(&self, converse: f64, transverse: f64, x_span: f64, y_span: f64) -> (f64, f64) {
        use SpecDirection::*;

        match (self.converse, self.transverse) {
            (Up, Left) => return (-transverse - x_span, -converse - y_span),
            (Up, Right) => return (transverse, -converse - y_span),
            (Down, Left) => return (-transverse - x_span, converse),
            (Down, Right) => return (transverse, converse),
            (Left, Up) => return (-converse - x_span, -transverse - y_span),
            (Left, Down) => return (-converse - x_span, transverse),
            (Right, Up) => return (converse, -transverse - y_span),
            (Right, Down) => return (converse, transverse),
            _ => panic!("directions are not perpendicular; syntax validation should have caught this"),
        }
    }

    pub fn direction_convert_point(&self, x: f64, y: f64) -> (f64, f64) {
        use SpecDirection::*;

        match (self.converse, self.transverse) {
            (Up, Left) => return (-y, -x),
            (Up, Right) => return (-y, x),
            (Down, Left) => return (y, -x),
            (Down, Right) => return (y, x),
            (Left, Up) => return (-x, -y),
            (Left, Down) => return (-x, y),
            (Right, Up) => return (x, -y),
            (Right, Down) => return (x, y),
            _ => panic!("directions are not perpendicular; syntax validation should have caught this"),
        }
    }

    pub fn direction_convert_transverse(&self, amount: f64, span: f64) -> f64 {
        use SpecDirection::*;

        match self.transverse {
            Left | Up => return -amount - span,
            Right | Down => return amount,
        }
    }

    /// Page coordinate of the transverse start of a box of the given size.
    pub fn direction_unconvert_transverse(&self, transverse: f64, x_span: f64, y_span: f64) -> UnconvertAxis {
        use SpecDirection::*;

        match self.transverse {
            Left => return UnconvertAxis {
                x: true,
                amount: -transverse - x_span,
            },
            Right => return UnconvertAxis {
                x: true,
                amount: transverse,
            },
            Up => return UnconvertAxis {
                x: false,
                amount: -transverse - y_span,
            },
            Down => return UnconvertAxis {
                x: false,
                amount: transverse,
            },
        }
    }

    /// (x_span, y_span) of a box spanning (converse_span, transverse_span).
    pub fn direction_unconvert_span(&self, converse_span: f64, transverse_span: f64) -> (f64, f64) {
        if self.direction_converse_vertical() {
            return (transverse_span, converse_span);
        } else {
            return (converse_span, transverse_span);
        }
    }

    /// (converse_span, transverse_span) of a page box of (width, height).
    pub fn direction_convert_span(&self, width: f64, height: f64) -> (f64, f64) {
        if self.direction_converse_vertical() {
            return (height, width);
        } else {
            return (width, height);
        }
    }
}
