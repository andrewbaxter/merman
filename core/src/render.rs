use crate::attachment::{
    DrawingKind,
    DrawingLayer,
};
use crate::context::{
    Context,
    Vector,
};
use crate::syntax::StyleId;
use crate::wall::BrickKind;

pub struct RowBrick {
    pub converse: f64,
    pub converse_span: f64,
    pub pad_before: f64,
    pub ascent: f64,
    pub descent: f64,
    pub text: String,
    pub style: StyleId,
}

pub struct Row {
    pub transverse: f64,
    pub ascent: f64,
    pub descent: f64,
    pub bricks: Vec<RowBrick>,
}

pub enum PathCommand {
    MoveTo(Vector),
    LineTo(Vector),
    ArcTo {
        corner: Vector,
        to: Vector,
        radius: f64,
    },
}

pub enum RenderDrawing {
    Obbox {
        layer: DrawingLayer,
        path: Vec<PathCommand>,
        line: bool,
        line_color: String,
        line_thickness: f64,
        fill: bool,
        fill_color: String,
    },
    Line {
        layer: DrawingLayer,
        from: Vector,
        to: Vector,
        thickness: f64,
        color: String,
        round_cap: bool,
    },
}

pub struct Snapshot {
    pub rows: Vec<Row>,
    pub drawings: Vec<RenderDrawing>,
    pub width: f64,
    pub transverse: (f64, f64),
}

fn simple_norm(v: f64) -> f64 {
    if (v * v) < 0.1 * 0.1 {
        return 0.;
    }
    if v < 0. {
        return -1.;
    }
    return 1.;
}

impl Context {
    pub fn render_snapshot(&self) -> Snapshot {
        let mut rows = vec![];
        let mut width: f64 = 0.;
        let mut t_min = f64::MAX;
        let mut t_max = f64::MIN;
        for c in &self.wall.children {
            let course = &self.courses[*c];
            t_min = t_min.min(course.transverse_start);
            t_max = t_max.max(self.course_transverse_edge(*c));
            let mut bricks = vec![];
            for b in &course.children {
                let brick = &self.bricks[*b];
                width = width.max(brick.converse + brick.converse_span);
                if let BrickKind::Text { text, style } = &brick.kind {
                    bricks.push(RowBrick {
                        converse: brick.converse,
                        converse_span: brick.converse_span,
                        pad_before: brick.pad_before,
                        ascent: brick.ascent,
                        descent: brick.descent,
                        text: text.clone(),
                        style: *style,
                    });
                }
            }
            rows.push(Row {
                transverse: course.transverse_start,
                ascent: course.ascent,
                descent: course.descent,
                bricks: bricks,
            });
        }
        if rows.is_empty() {
            t_min = 0.;
            t_max = 0.;
        }
        let mut drawings = vec![];
        for d in self.drawings.iter().flatten() {
            match &d.kind {
                None => { },
                Some(DrawingKind::Obbox { points, style }) => {
                    drawings.push(RenderDrawing::Obbox {
                        layer: d.layer,
                        path: {
                            let base_radius = style.round_radius;
                            let mut out = vec![];
                            let n = points.len();
                            for i in 0 .. n {
                                let (mid, round) = points[i];
                                if round {
                                    let (pre, _) = points[(i + n - 1) % n];
                                    let (post, _) = points[(i + 1) % n];
                                    let to_pre =
                                        Vector::new(pre.converse - mid.converse, pre.transverse - mid.transverse);
                                    let to_post =
                                        Vector::new(post.converse - mid.converse, post.transverse - mid.transverse);
                                    let mut radius = base_radius;
                                    radius =
                                        radius.min(f64::max(to_pre.converse.abs(), to_pre.transverse.abs()) / 2.);
                                    radius =
                                        radius.min(f64::max(to_post.converse.abs(), to_post.transverse.abs()) / 2.);
                                    if i == 0 {
                                        out.push(
                                            PathCommand::MoveTo(
                                                Vector::new(
                                                    mid.converse + simple_norm(to_pre.converse) * radius,
                                                    mid.transverse + simple_norm(to_pre.transverse) * radius,
                                                ),
                                            ),
                                        );
                                    }
                                    out.push(PathCommand::ArcTo {
                                        corner: mid,
                                        to: Vector::new(
                                            mid.converse + simple_norm(to_post.converse) * radius,
                                            mid.transverse + simple_norm(to_post.transverse) * radius,
                                        ),
                                        radius: radius,
                                    });
                                } else if i == 0 {
                                    out.push(PathCommand::MoveTo(mid));
                                } else {
                                    out.push(PathCommand::LineTo(mid));
                                }
                            }
                            out
                        },
                        line: style.line,
                        line_color: style.line_color.clone(),
                        line_thickness: style.line_thickness,
                        fill: style.fill,
                        fill_color: style.fill_color.clone(),
                    });
                },
                Some(DrawingKind::Line { from, to, thickness, color, round_cap }) => {
                    drawings.push(RenderDrawing::Line {
                        layer: d.layer,
                        from: *from,
                        to: *to,
                        thickness: *thickness,
                        color: color.clone(),
                        round_cap: *round_cap,
                    })
                },
            }
        }
        return Snapshot {
            rows: rows,
            drawings: drawings,
            width: width,
            transverse: (t_min, t_max),
        };
    }
}
