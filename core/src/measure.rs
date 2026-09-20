//! Text measurement abstraction. The web display implements this with a canvas;
//! tests use a fixed-width fake.
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq)]
pub struct FontSpec {
    pub family: String,
    pub size: f64,
}

impl FontSpec {
    /// CSS shorthand as used by canvas and the renderer.
    pub fn font_css(&self) -> String {
        return format!("{}px {}", self.size, self.family);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    pub ascent: f64,
    pub descent: f64,
}

pub trait Measure {
    fn measure_width(&mut self, font: &FontSpec, text: &str) -> f64;
    fn measure_metrics(&mut self, font: &FontSpec) -> FontMetrics;
}

pub fn measure_index_at_converse(measure: &mut dyn Measure, font: &FontSpec, text: &str, converse: f64) -> usize {
    let mut previous_width = 0.;
    let mut previous_index = 0;
    for (i, g) in text.grapheme_indices(true) {
        let end = i + g.len();
        let width = measure.measure_width(font, &text[..end]);
        if (converse - previous_width) / (width - previous_width) < 0.5 {
            break;
        }
        previous_width = width;
        previous_index = end;
    }
    return previous_index;
}

/// Line break position for a split near `offset`: the last word boundary at or
/// before it, extended over any following whitespace so the next line starts at
/// text (merman's line walker: a line keeps as much trailing whitespace as
/// possible; hanging whitespace is invisible).
pub fn measure_line_before_or_at(text: &str, offset: usize) -> usize {
    let mut out = 0;
    for (i, _) in text.split_word_bound_indices() {
        if i > offset {
            break;
        }
        out = i;
    }
    while out < text.len() {
        match text[out..].chars().next() {
            Some(c) if c.is_whitespace() => out += c.len_utf8(),
            _ => break,
        }
    }
    return out;
}

/// Fixed width fake for tests: every grapheme is `size * 0.6` wide.
pub struct MeasureFixed;

impl Measure for MeasureFixed {
    fn measure_width(&mut self, font: &FontSpec, text: &str) -> f64 {
        return text.graphemes(true).count() as f64 * font.size * 0.6;
    }

    fn measure_metrics(&mut self, font: &FontSpec) -> FontMetrics {
        return FontMetrics {
            ascent: font.size * 0.8,
            descent: font.size * 0.2,
        };
    }
}
