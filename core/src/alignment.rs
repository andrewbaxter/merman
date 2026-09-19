//! Alignments (merman `visual/alignment`): named converse positions that
//! bricks starting a course snap to.
use crate::context::{AlignId, BrickId, Context, VisualId};
use crate::iteration::TaskKind;
use crate::spec::SpecAlignment;

pub enum AlignmentKind {
    Relative {
        base_key: String,
        /// In px.
        offset: f64,
        /// If true, doesn't contribute offset to derived alignments if this
        /// alignment doesn't have any bricks.
        collapse: bool,
        base: Option<AlignId>,
    },
    Concensus {
        iteration_align: Option<crate::context::TaskId>,
    },
}

pub struct Alignment {
    pub kind: AlignmentKind,
    pub converse: f64,
    /// Bricks (one per course) currently aligned by this alignment.
    pub bricks: Vec<BrickId>,
    pub derived: Vec<AlignId>,
}

impl Context {
    pub fn alignment_create(&mut self, spec: &SpecAlignment) -> AlignId {
        let to_pixels = self.syntax.spec_root.to_pixels;
        let (kind, converse) = match spec {
            SpecAlignment::Relative(r) => (
                AlignmentKind::Relative {
                    base_key: r.base.clone(),
                    offset: r.offset * to_pixels,
                    collapse: r.collapse,
                    base: None,
                },
                r.offset * to_pixels,
            ),
            SpecAlignment::Concensus(_) => (
                AlignmentKind::Concensus {
                    iteration_align: None,
                },
                0.,
            ),
        };
        let id = self.aligns.len();
        self.aligns.push(Alignment {
            kind,
            converse,
            bricks: vec![],
            derived: vec![],
        });
        return id;
    }

    /// Placed in the tree under `atom`: resolve the base alignment.
    pub fn alignment_root(&mut self, a: AlignId, atom: VisualId) {
        let base_key = match &self.aligns[a].kind {
            AlignmentKind::Relative { base_key, .. } => base_key.clone(),
            AlignmentKind::Concensus { .. } => return,
        };
        let base = self.parent_find_alignment(atom, &base_key);
        if base == Some(a) {
            panic!("alignment parented to self");
        }
        if let AlignmentKind::Relative { base: b, .. } = &mut self.aligns[a].kind {
            *b = base;
        }
        if let Some(b) = base {
            self.aligns[b].derived.push(a);
        }
        self.alignment_changed(a);
    }

    /// A course placed its aligned brick at `converse` before alignment.
    pub fn alignment_feedback(&mut self, a: AlignId, converse: f64) {
        if let AlignmentKind::Concensus { .. } = &self.aligns[a].kind {
            if converse > self.aligns[a].converse {
                self.alignment_iteration_align(a);
            }
        }
    }

    fn alignment_iteration_align(&mut self, a: AlignId) {
        let AlignmentKind::Concensus { iteration_align } = &self.aligns[a].kind else {
            return;
        };
        if iteration_align.is_some() {
            return;
        }
        let task = self.add_iteration(TaskKind::ConcensusAlign { align: a }, 0.);
        if let AlignmentKind::Concensus { iteration_align } = &mut self.aligns[a].kind {
            *iteration_align = Some(task);
        }
    }

    /// Recompute a relative alignment's position and re-lay everything that
    /// depends on it.
    pub fn alignment_changed(&mut self, a: AlignId) {
        if let AlignmentKind::Relative {
            offset,
            collapse,
            base,
            ..
        } = &self.aligns[a].kind
        {
            let base_converse = base.map(|b| self.aligns[b].converse).unwrap_or(0.);
            let offset = if *collapse && self.aligns[a].bricks.is_empty() {
                0.
            } else {
                *offset
            };
            self.aligns[a].converse = base_converse + offset;
        }
        for b in self.aligns[a].bricks.clone() {
            self.brick_layout_properties_changed(b);
        }
        for d in self.aligns[a].derived.clone() {
            self.alignment_changed(d);
        }
    }

    pub fn alignment_add_brick(&mut self, a: AlignId, b: BrickId) {
        self.aligns[a].bricks.push(b);
        if let AlignmentKind::Relative { collapse: true, .. } = &self.aligns[a].kind {
            if self.aligns[a].bricks.len() == 1 {
                self.alignment_changed(a);
            }
        }
    }

    pub fn alignment_remove_brick(&mut self, a: AlignId, b: BrickId) {
        self.aligns[a].bricks.retain(|x| *x != b);
        match &self.aligns[a].kind {
            AlignmentKind::Relative { collapse: true, .. } => {
                if self.aligns[a].bricks.is_empty() {
                    self.alignment_changed(a);
                }
            }
            AlignmentKind::Concensus { .. } => {
                if self.bricks[b].pre_align_converse == self.aligns[a].converse {
                    self.alignment_iteration_align(a);
                }
            }
            _ => {}
        }
    }

    /// Concensus alignment task: move to the largest natural position of its
    /// bricks.
    pub fn run_concensus_align(&mut self, a: AlignId) -> bool {
        let old = self.aligns[a].converse;
        let mut max: f64 = 0.;
        for b in &self.aligns[a].bricks {
            max = max.max(self.bricks[*b].pre_align_converse);
        }
        self.aligns[a].converse = max;
        if old != max {
            self.alignment_changed(a);
        }
        return false;
    }

    pub fn concensus_align_destroyed(&mut self, a: AlignId) {
        if let AlignmentKind::Concensus { iteration_align } = &mut self.aligns[a].kind {
            *iteration_align = None;
        }
    }
}
