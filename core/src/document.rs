//! A matched document: a tree of atoms with named fields.
use crate::syntax::TypeId;
use std::collections::HashMap;

pub type AtomId = usize;

pub struct Document {
    pub atoms: Vec<Atom>,
    pub root: AtomId,
}

pub struct Atom {
    pub type_: TypeId,
    pub fields: HashMap<String, Field>,
    /// None for the root.
    pub parent: Option<AtomParent>,
}

pub struct AtomParent {
    pub atom: AtomId,
    pub field: String,
    /// Index in the array field; 0 for atom fields.
    pub index: usize,
}

pub enum Field {
    Primitive(String),
    Atom(AtomId),
    Array(Vec<AtomId>),
}

impl Document {
    pub fn document_atom(&self, id: AtomId) -> &Atom {
        return &self.atoms[id];
    }
}
