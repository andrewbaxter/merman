//! Turns a document into "bricks" (the atomic laid out pieces: a run of text or
//! a space) following each atom type's front specification.
use crate::document::{AtomId, Document, Field};
use crate::measure::Measure;
use crate::spec::{SpecAlignment, SpecCondition, SpecSplit};
use crate::syntax::{Front, StyleId, Symbol, SymbolKind, Syntax, TypeId};
use std::collections::HashSet;
use std::rc::Rc;

pub type VisualAtomId = usize;
pub type AlignId = usize;
pub type BrickId = usize;
pub type PrimId = usize;

pub struct VisualAtom {
    pub atom: AtomId,
    pub type_: TypeId,
    pub parent: Option<VisualAtomParent>,
    pub depth_score: i64,
    pub precedence: i64,
    /// Local alignments, in declaration order.
    pub alignments: Vec<(String, AlignId)>,
    /// Bricks created directly for this atom (not soft wrapped continuation
    /// lines, which are added during layout).
    pub bricks: Vec<BrickId>,
    /// First and last brick of this atom including nested atoms, in document
    /// order; None if the subtree has no bricks.
    pub subtree_bricks: Option<(BrickId, BrickId)>,
}

pub struct VisualAtomParent {
    pub atom: VisualAtomId,
    /// Which of the parent's local alignments this atom may see.
    pub forward: Rc<HashSet<String>>,
}

pub enum AlignKind {
    Relative {
        base: Option<AlignId>,
        offset: f64,
        collapse: bool,
    },
    Concensus,
}

pub struct AlignDef {
    pub kind: AlignKind,
}

#[derive(Clone)]
pub struct Brick {
    pub owner: VisualAtomId,
    /// None for a space.
    pub text: Option<String>,
    pub style: StyleId,
    pub split: SpecSplit,
    pub align: Option<AlignId>,
    pub split_align: Option<AlignId>,
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    /// Offset of the text within the brick.
    pub pad_before: f64,
    pub primitive: Option<BrickPrimitive>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BrickPrimitive {
    pub prim: PrimId,
    /// Which newline separated line of the primitive this brick shows (part of).
    pub hard_line: usize,
}

pub struct Primitive {
    pub owner: VisualAtomId,
    pub style: StyleId,
    pub soft_split_align: Option<AlignId>,
    /// Number of newline separated lines.
    pub hard_lines: usize,
}

pub struct Visual {
    pub atoms: Vec<VisualAtom>,
    pub aligns: Vec<AlignDef>,
    /// In document order.
    pub bricks: Vec<Brick>,
    pub prims: Vec<Primitive>,
}

pub struct TextMetrics {
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    pub pad_before: f64,
}

/// Size of a text brick including style padding and overrides.
pub fn visual_text_metrics(
    syntax: &Syntax,
    measure: &mut dyn Measure,
    style: StyleId,
    text: &str,
) -> TextMetrics {
    let style = syntax.syntax_style(style);
    let font = measure.measure_metrics(&style.font);
    let width = measure.measure_width(&style.font, text);
    return TextMetrics {
        width: width + style.padding.converse_start + style.padding.converse_end,
        ascent: style.ascent.unwrap_or(font.ascent) + style.padding.transverse_start,
        descent: style.descent.unwrap_or(font.descent) + style.padding.transverse_end,
        pad_before: style.padding.converse_start,
    };
}

impl Visual {
    pub fn visual_build(syntax: &Syntax, document: &Document, measure: &mut dyn Measure) -> Visual {
        let mut b = Builder {
            syntax,
            document,
            measure,
            out: Visual {
                atoms: vec![],
                aligns: vec![],
                bricks: vec![],
                prims: vec![],
            },
        };
        b.build_atom(document.root, None);
        return b.out;
    }

    /// Look up an alignment by name from an atom: first its own (if allowed),
    /// then its ancestors' (restricted by what each field forwards).
    pub fn visual_find_alignment(
        &self,
        atom: VisualAtomId,
        name: &str,
        allow: Option<&HashSet<String>>,
    ) -> Option<AlignId> {
        let va = &self.atoms[atom];
        if allow.map_or(true, |a| a.contains(name)) {
            if let Some((_, id)) = va.alignments.iter().find(|(n, _)| n == name) {
                return Some(*id);
            }
        }
        match &va.parent {
            Some(p) => return self.visual_find_alignment(p.atom, name, Some(&p.forward)),
            None => return None,
        }
    }
}

struct Builder<'a> {
    syntax: &'a Syntax,
    document: &'a Document,
    measure: &'a mut dyn Measure,
    out: Visual,
}

impl<'a> Builder<'a> {
    fn build_atom(&mut self, atom_id: AtomId, parent: Option<VisualAtomParent>) -> VisualAtomId {
        let atom = self.document.document_atom(atom_id);
        let type_ = self.syntax.syntax_type(atom.type_);
        let depth_score = match &parent {
            Some(p) => self.out.atoms[p.atom].depth_score + type_.depth_score,
            None => 0,
        };
        let vid = self.out.atoms.len();
        let first_brick = self.out.bricks.len();
        self.out.atoms.push(VisualAtom {
            atom: atom_id,
            type_: atom.type_,
            parent,
            depth_score,
            precedence: type_.precedence,
            alignments: vec![],
            bricks: vec![],
            subtree_bricks: None,
        });
        let to_pixels = self.syntax.spec_root.to_pixels;
        for (name, spec) in &type_.alignments {
            let kind = match spec {
                SpecAlignment::Relative(r) => {
                    let base = match &self.out.atoms[vid].parent {
                        Some(p) => self.out.visual_find_alignment(p.atom, &r.base, Some(&p.forward)),
                        None => None,
                    };
                    AlignKind::Relative {
                        base,
                        offset: r.offset * to_pixels,
                        collapse: r.collapse,
                    }
                }
                SpecAlignment::Concensus(_) => AlignKind::Concensus,
            };
            let aid = self.out.aligns.len();
            self.out.aligns.push(AlignDef { kind });
            self.out.atoms[vid].alignments.push((name.clone(), aid));
        }
        for front in &type_.front {
            match front {
                Front::Symbol(s) => self.build_symbol(vid, atom_id, s),
                Front::Primitive(p) => {
                    let Some(Field::Primitive(text)) = atom.fields.get(&p.field) else {
                        panic!("front primitive field `{}` missing; syntax validation should have caught this", p.field);
                    };
                    let prim = self.out.prims.len();
                    self.out.prims.push(Primitive {
                        owner: vid,
                        style: p.style,
                        soft_split_align: self.resolve_alignment(vid, &p.soft_split_alignment),
                        hard_lines: text.split('\n').count(),
                    });
                    for (i, line) in text.split('\n').enumerate() {
                        let (split, align, split_align) = if i == 0 {
                            (
                                p.split,
                                self.resolve_alignment(vid, &p.first_alignment),
                                self.resolve_alignment(vid, &p.first_split_alignment),
                            )
                        } else {
                            (
                                SpecSplit::Always,
                                None,
                                self.resolve_alignment(vid, &p.hard_split_alignment),
                            )
                        };
                        let shown = self.printable(line);
                        self.push_text_brick(
                            vid,
                            shown,
                            p.style,
                            split,
                            align,
                            split_align,
                            Some(BrickPrimitive { prim, hard_line: i }),
                        );
                    }
                }
                Front::Atom(f) => {
                    let Some(Field::Atom(child)) = atom.fields.get(&f.field) else {
                        panic!("front atom field `{}` missing; syntax validation should have caught this", f.field);
                    };
                    self.build_atom(
                        *child,
                        Some(VisualAtomParent {
                            atom: vid,
                            forward: f.forward_alignments.clone(),
                        }),
                    );
                }
                Front::Array(f) => {
                    let Some(Field::Array(children)) = atom.fields.get(&f.field) else {
                        panic!("front array field `{}` missing; syntax validation should have caught this", f.field);
                    };
                    if children.is_empty() {
                        if let Some(e) = &f.empty {
                            self.build_symbol(vid, atom_id, e);
                        }
                    }
                    for (i, child) in children.iter().enumerate() {
                        if i > 0 {
                            for s in &f.separator {
                                self.build_symbol(vid, atom_id, s);
                            }
                        }
                        for s in &f.prefix {
                            self.build_symbol(vid, atom_id, s);
                        }
                        self.build_atom(
                            *child,
                            Some(VisualAtomParent {
                                atom: vid,
                                forward: f.forward_alignments.clone(),
                            }),
                        );
                        for s in &f.suffix {
                            self.build_symbol(vid, atom_id, s);
                        }
                    }
                }
            }
        }
        let end = self.out.bricks.len();
        if end > first_brick {
            self.out.atoms[vid].subtree_bricks = Some((first_brick, end - 1));
        }
        return vid;
    }

    fn printable(&self, text: &str) -> String {
        let unprintable = &self.syntax.spec_root.unprintable;
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if c.is_control() {
                out.push_str(unprintable);
            } else {
                out.push(c);
            }
        }
        return out;
    }

    fn resolve_alignment(&self, vid: VisualAtomId, name: &Option<String>) -> Option<AlignId> {
        match name {
            Some(n) => return self.out.visual_find_alignment(vid, n, None),
            None => return None,
        }
    }

    fn build_symbol(&mut self, vid: VisualAtomId, atom_id: AtomId, s: &Symbol) {
        if let Some(c) = &s.condition {
            if !self.condition_show(c, atom_id) {
                return;
            }
        }
        let align = self.resolve_alignment(vid, &s.alignment);
        let split_align = self.resolve_alignment(vid, &s.split_alignment);
        match &s.kind {
            SymbolKind::Text { text, style } => {
                self.push_text_brick(vid, text.clone(), *style, s.split, align, split_align, None);
            }
            SymbolKind::Space {
                width,
                ascent,
                descent,
            } => {
                let to_pixels = self.syntax.spec_root.to_pixels;
                let id = self.out.bricks.len();
                self.out.bricks.push(Brick {
                    owner: vid,
                    text: None,
                    style: 0,
                    split: s.split,
                    align,
                    split_align,
                    width: *width * to_pixels,
                    ascent: *ascent * to_pixels,
                    descent: *descent * to_pixels,
                    pad_before: 0.,
                    primitive: None,
                });
                self.out.atoms[vid].bricks.push(id);
            }
        }
    }

    fn push_text_brick(
        &mut self,
        vid: VisualAtomId,
        text: String,
        style: StyleId,
        split: SpecSplit,
        align: Option<AlignId>,
        split_align: Option<AlignId>,
        primitive: Option<BrickPrimitive>,
    ) {
        let m = visual_text_metrics(self.syntax, self.measure, style, &text);
        let id = self.out.bricks.len();
        self.out.bricks.push(Brick {
            owner: vid,
            text: Some(text),
            style,
            split,
            align,
            split_align,
            width: m.width,
            ascent: m.ascent,
            descent: m.descent,
            pad_before: m.pad_before,
            primitive,
        });
        self.out.atoms[vid].bricks.push(id);
    }

    fn condition_show(&self, condition: &SpecCondition, atom_id: AtomId) -> bool {
        let atom = self.document.document_atom(atom_id);
        match condition {
            SpecCondition::Empty(c) => {
                let empty = match atom.fields.get(&c.field) {
                    Some(Field::Primitive(s)) => s.is_empty(),
                    Some(Field::Array(a)) => a.is_empty(),
                    _ => panic!("condition field `{}` is not a primitive or array; syntax validation should have caught this", c.field),
                };
                return empty != c.invert;
            }
            SpecCondition::Precedent(c) => {
                return self.is_precedent(atom_id) != c.invert;
            }
        }
    }

    /// Whether the atom binds at least as tightly as its parent, so it needs
    /// no parentheses.
    fn is_precedent(&self, atom_id: AtomId) -> bool {
        let atom = self.document.document_atom(atom_id);
        let Some(parent_ref) = &atom.parent else {
            return true;
        };
        let parent = self.document.document_atom(parent_ref.atom);
        let parent_type = self.syntax.syntax_type(parent.type_);
        let own_type = self.syntax.syntax_type(atom.type_);
        let mut fore_child = true;
        let mut back_child = true;
        let mut found = false;
        for front in &parent_type.front {
            match front {
                Front::Symbol(s) => {
                    if s.symbol_delimits() {
                        if !found {
                            back_child = false;
                        } else {
                            fore_child = false;
                        }
                    }
                }
                Front::Primitive(_) => {
                    if !found {
                        back_child = false;
                    } else {
                        fore_child = false;
                    }
                }
                Front::Array(a) => {
                    if !found {
                        for p in &a.prefix {
                            if p.symbol_delimits() {
                                back_child = false;
                            }
                        }
                    }
                    if a.field == parent_ref.field {
                        let Some(Field::Array(siblings)) = parent.fields.get(&a.field) else {
                            panic!("front array field `{}` missing; syntax validation should have caught this", a.field);
                        };
                        if parent_ref.index > 0 {
                            back_child = false;
                        }
                        found = true;
                        if parent_ref.index + 1 < siblings.len() {
                            fore_child = false;
                        }
                    }
                    if found {
                        for s in &a.suffix {
                            if s.symbol_delimits() {
                                fore_child = false;
                            }
                        }
                    }
                }
                Front::Atom(f) => {
                    if f.field == parent_ref.field {
                        found = true;
                    }
                }
            }
        }
        if !back_child && !fore_child {
            return true;
        }
        if parent_type.precedence < own_type.precedence {
            return true;
        }
        if parent_type.precedence == own_type.precedence
            && fore_child == parent_type.associate_forward
        {
            return true;
        }
        return false;
    }
}
