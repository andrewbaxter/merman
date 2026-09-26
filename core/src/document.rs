//! A matched document: a tree of atoms with named fields.
use crate::cursor::Located;
use crate::syntax::TypeId;
use std::collections::HashMap;

pub type AtomId = usize;

pub struct Document {
    pub atoms: Vec<Atom>,
    pub root: AtomId,
}

pub struct Atom {
    pub type_: TypeId,
    pub back_ids: Vec<i64>,
    pub unique_id: Option<i64>,
    pub path: String,
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

    pub fn document_locate(&self, from: AtomId, path: &[String]) -> Option<Located> {
        let mut at = Located::Atom(from);
        let mut i = 0;
        while i < path.len() {
            match at {
                Located::Atom(a) => {
                    if path[i] != "named" || i + 1 >= path.len() {
                        return None;
                    }
                    let field = path[i + 1].clone();
                    if !self.document_atom(a).fields.contains_key(&field) {
                        return None;
                    }
                    i += 2;
                    at = Located::Field(a, field);
                },
                Located::Field(a, ref field) => match self.document_atom(a).fields.get(field).unwrap() {
                    Field::Array(elements) => {
                        let Ok(index) = path[i].parse::<usize>() else {
                            return None;
                        };
                        if index >= elements.len() {
                            return None;
                        }
                        i += 1;
                        at = Located::Atom(elements[index]);
                    },
                    Field::Atom(child) => {
                        at = Located::Atom(*child);
                    },
                    Field::Primitive(_) => {
                        return Some(at);
                    },
                },
            }
        }
        return Some(at);
    }
}
