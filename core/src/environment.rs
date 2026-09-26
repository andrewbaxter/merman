use unicode_segmentation::UnicodeSegmentation;

pub trait Environment {
    fn environment_clipboard_set(&mut self, text: &str);

    fn environment_glyph_walker(&self, text: &str) -> GlyphWalker {
        return GlyphWalker { bounds: bounds_of(text.grapheme_indices(true).map(|(i, _)| i), text.len()) };
    }
    fn environment_now_ms(&mut self) -> f64;

    fn environment_split_glyphs(&self, text: &str) -> Vec<String> {
        return text.graphemes(true).map(|g| g.to_string()).collect();
    }

    fn environment_word_walker(&self, text: &str) -> WordWalker {
        return WordWalker {
            bounds: bounds_of(text.split_word_bound_indices().map(|(i, _)| i), text.len()),
            whitespace: text.char_indices().filter(|(_, c)| c.is_whitespace()).map(|(i, _)| i).collect(),
            length: text.len(),
        };
    }
}

fn bounds_of(starts: impl Iterator<Item = usize>, length: usize) -> Vec<usize> {
    let mut out: Vec<usize> = starts.collect();
    out.push(length);
    return out;
}

#[derive(Clone, Default)]
pub struct EnvironmentTest(pub std::rc::Rc<std::cell::RefCell<EnvironmentTestState>>);

impl Environment for EnvironmentTest {
    fn environment_clipboard_set(&mut self, text: &str) {
        self.0.borrow_mut().clipboard = Some(text.to_string());
    }

    fn environment_now_ms(&mut self) -> f64 {
        let mut s = self.0.borrow_mut();
        s.now += 1.;
        return s.now;
    }
}

#[derive(Default)]
pub struct EnvironmentTestState {
    pub clipboard: Option<String>,
    pub now: f64,
}

pub struct GlyphWalker {
    bounds: Vec<usize>,
}

impl GlyphWalker {
    pub fn glyph_after(&self, offset: usize) -> usize {
        for b in &self.bounds {
            if *b > offset {
                return *b;
            }
        }
        return *self.bounds.last().unwrap();
    }

    pub fn glyph_before(&self, offset: usize) -> usize {
        let mut out = 0;
        for b in &self.bounds {
            if *b >= offset {
                break;
            }
            out = *b;
        }
        return out;
    }
}

pub struct WordWalker {
    bounds: Vec<usize>,
    length: usize,
    whitespace: Vec<usize>,
}

impl WordWalker {
    fn any_at_or_after(&self, offset: usize) -> usize {
        for b in &self.bounds {
            if *b >= offset {
                return *b;
            }
        }
        return self.length;
    }

    fn any_before_or_at(&self, offset: usize) -> usize {
        let mut out = 0;
        for b in &self.bounds {
            if *b > offset {
                break;
            }
            out = *b;
        }
        return out;
    }

    fn is_whitespace(&self, offset: usize) -> bool {
        if offset >= self.length {
            return true;
        }
        return self.whitespace.binary_search(&offset).is_ok();
    }

    pub fn word_end_after(&self, offset: usize) -> usize {
        if offset == self.length {
            return self.length;
        }
        let out = self.any_at_or_after(offset + 1);
        if self.is_whitespace(out) {
            return out;
        }
        return self.any_at_or_after(out + 1);
    }

    pub fn word_start_after(&self, offset: usize) -> usize {
        if offset == self.length {
            return self.length;
        }
        let out = self.any_at_or_after(offset + 1);
        if !self.is_whitespace(out) || out == self.length {
            return out;
        }
        return self.any_at_or_after(out + 1);
    }

    pub fn word_start_before(&self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }
        let at = offset - 1;
        let out = self.any_before_or_at(at);
        if !self.is_whitespace(out) || out == 0 {
            return out;
        }
        return self.any_before_or_at(out - 1);
    }
}
