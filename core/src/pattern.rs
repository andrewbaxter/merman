use {
    crate::spec::SpecPattern,
    std::collections::HashSet,
};

enum Node {
    Accept,
    Split(Vec<usize>),
    Test(Test, usize),
}

pub struct PatternMatcher {
    nodes: Vec<Node>,
    start: usize,
}

impl PatternMatcher {
    fn pattern_compile(&mut self, pattern: &SpecPattern, next: usize) -> usize {
        let one = |s: &str| SpecPattern::String(s.to_string());
        let digit = || SpecPattern::CharacterClass(vec![("0".to_string(), "9".to_string())]);
        let no_leading_zero =
            || SpecPattern::Union(
                vec![
                    one("0"),
                    SpecPattern::Sequence(
                        vec![
                            SpecPattern::CharacterClass(vec![("1".to_string(), "9".to_string())]),
                            SpecPattern::Repeat0(Box::new(digit())),
                        ],
                    ),
                ],
            );
        let test = |m: &mut PatternMatcher, test: Test| {
            m.nodes.push(Node::Test(test, next));
            return m.nodes.len() - 1;
        };
        match pattern {
            SpecPattern::Any => return test(self, Test::Any),
            SpecPattern::Digits => return test(self, Test::Digit),
            SpecPattern::Letters => return test(self, Test::Letter),
            SpecPattern::SymbolCharacter => return test(self, Test::Symbol),
            SpecPattern::CharacterClass(ranges) => {
                let branches =
                    ranges
                        .iter()
                        .map(|(low, high)| test(self, Test::Range(low.clone(), high.clone())))
                        .collect();
                self.nodes.push(Node::Split(branches));
                return self.nodes.len() - 1;
            },
            SpecPattern::String(text) => {
                let mut at = next;
                for glyph in unicode_segmentation::UnicodeSegmentation::graphemes(text.as_str(), true)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev() {
                    self.nodes.push(Node::Test(Test::Glyph(glyph.to_string()), at));
                    at = self.nodes.len() - 1;
                }
                return at;
            },
            SpecPattern::Integer => {
                return self.pattern_compile(
                    &SpecPattern::Sequence(
                        vec![SpecPattern::Maybe(Box::new(one("-"))), SpecPattern::Repeat1(Box::new(digit()))],
                    ),
                    next,
                );
            },
            SpecPattern::JsonDecimal => {
                return self.pattern_compile(
                    &SpecPattern::Sequence(
                        vec![
                            SpecPattern::Maybe(Box::new(one("-"))),
                            no_leading_zero(),
                            SpecPattern::Maybe(
                                Box::new(
                                    SpecPattern::Sequence(vec![one("."), SpecPattern::Repeat0(Box::new(digit()))]),
                                ),
                            ),
                            SpecPattern::Maybe(
                                Box::new(
                                    SpecPattern::Sequence(
                                        vec![
                                            SpecPattern::Union(vec![one("e"), one("E")]),
                                            SpecPattern::Maybe(
                                                Box::new(SpecPattern::Union(vec![one("-"), one("+")])),
                                            ),
                                            no_leading_zero(),
                                        ],
                                    ),
                                ),
                            ),
                        ],
                    ),
                    next,
                );
            },
            SpecPattern::Maybe(inner) => {
                let body = self.pattern_compile(inner, next);
                self.nodes.push(Node::Split(vec![body, next]));
                return self.nodes.len() - 1;
            },
            SpecPattern::Repeat0(inner) => {
                self.nodes.push(Node::Split(vec![]));
                let split = self.nodes.len() - 1;
                let body = self.pattern_compile(inner, split);
                self.nodes[split] = Node::Split(vec![body, next]);
                return split;
            },
            SpecPattern::Repeat1(inner) => {
                let rest = self.pattern_compile(&SpecPattern::Repeat0(inner.clone()), next);
                return self.pattern_compile(inner, rest);
            },
            SpecPattern::Sequence(children) => {
                let mut at = next;
                for child in children.iter().rev() {
                    at = self.pattern_compile(child, at);
                }
                return at;
            },
            SpecPattern::Union(children) => {
                let branches = children.iter().map(|c| self.pattern_compile(c, next)).collect();
                self.nodes.push(Node::Split(branches));
                return self.nodes.len() - 1;
            },
        }
    }

    pub fn pattern_matches(&self, glyphs: &[String], prefix: bool) -> bool {
        let mut states = self.pattern_close(vec![self.start]);
        for glyph in glyphs {
            let mut next = vec![];
            let mut chars = glyph.chars();
            let single = match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            };
            for s in &states {
                if let Node::Test(test, to) = &self.nodes[*s] {
                    let passes = match test {
                        Test::Any => true,
                        Test::Digit => single.is_some_and(|c| c.is_ascii_digit()),
                        Test::Glyph(g) => g == glyph,
                        Test::Letter => single.is_some_and(|c| c.is_ascii_alphabetic()),
                        Test::Range(low, high) => low.as_str() <= glyph.as_str() && glyph.as_str() <= high.as_str(),
                        Test::Symbol => !glyph.is_empty() && glyph.chars().all(|c| c.is_alphanumeric()),
                    };
                    if passes {
                        next.push(*to);
                    }
                }
            }
            states = self.pattern_close(next);
            if states.is_empty() {
                return false;
            }
        }
        return prefix || states.iter().any(|s| matches!(self.nodes[*s], Node::Accept));
    }

    pub fn pattern_new(pattern: &SpecPattern) -> PatternMatcher {
        let mut out = PatternMatcher {
            nodes: vec![Node::Accept],
            start: 0,
        };
        out.start = out.pattern_compile(pattern, 0);
        return out;
    }

    fn pattern_close(&self, states: Vec<usize>) -> Vec<usize> {
        let mut seen = HashSet::new();
        let mut out = vec![];
        let mut stack = states;
        while let Some(s) = stack.pop() {
            if !seen.insert(s) {
                continue;
            }
            match &self.nodes[s] {
                Node::Split(branches) => stack.extend(branches.iter().copied()),
                Node::Test(..) | Node::Accept => out.push(s),
            }
        }
        return out;
    }
}

enum Test {
    Any,
    Digit,
    Glyph(String),
    Letter,
    Range(String, String),
    Symbol,
}
