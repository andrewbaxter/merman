//! A matched document: a tree of atoms with named fields.
use crate::cursor::Located;
use {
    crate::syntax::TypeId,
    std::collections::HashMap,
};

#[derive(Clone)]
pub struct Atom {
    pub back_ids: Vec<i64>,
    pub fields: HashMap<String, Field>,
    /// None for the root.
    pub parent: Option<AtomParent>,
    pub path: String,
    pub type_: TypeId,
    pub unique_id: Option<i64>,
}

pub type AtomId = usize;

#[derive(Clone)]
pub struct AtomParent {
    pub atom: AtomId,
    pub field: String,
    /// Index in the array field; 0 for atom fields.
    pub index: usize,
}

#[derive(Clone)]
pub struct Document {
    pub atoms: Vec<Atom>,
    pub root: AtomId,
}

impl Document {
    pub fn document_array_splice(
        &mut self,
        atom: AtomId,
        field: &str,
        index: usize,
        remove: usize,
        add: Vec<AtomId>,
    ) -> Vec<AtomId> {
        let Some(Field::Array(elements)) = self.atoms[atom].fields.get_mut(field) else {
            panic!("field `{}` is not an array", field);
        };
        let removed: Vec<AtomId> = elements.splice(index .. index + remove, add).collect();
        let renumber = elements[index..].to_vec();
        for r in &removed {
            self.atoms[*r].parent = None;
        }
        for (i, a) in renumber.into_iter().enumerate() {
            self.atoms[a].parent = Some(AtomParent {
                atom: atom,
                field: field.to_string(),
                index: index + i,
            });
        }
        return removed;
    }

    pub fn document_atom(&self, id: AtomId) -> &Atom {
        return &self.atoms[id];
    }

    pub fn document_atom_set(&mut self, atom: AtomId, field: &str, value: AtomId) -> AtomId {
        let Some(Field::Atom(child)) = self.atoms[atom].fields.get_mut(field) else {
            panic!("field `{}` is not an atom", field);
        };
        let old = std::mem::replace(child, value);
        self.atoms[old].parent = None;
        self.atoms[value].parent = Some(AtomParent {
            atom: atom,
            field: field.to_string(),
            index: 0,
        });
        return old;
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

#[derive(Clone)]
pub enum Field {
    Array(Vec<AtomId>),
    Atom(AtomId),
    Primitive(String),
}

impl Document {
    pub fn document_primitive<'a>(&'a self, atom: AtomId, field: &str) -> &'a str {
        let Some(Field::Primitive(text)) = self.atoms[atom].fields.get(field) else {
            panic!("field `{}` is not a primitive", field);
        };
        return text;
    }

    pub fn document_primitive_splice(&mut self, atom: AtomId, field: &str, index: usize, remove: usize, add: &str) {
        let Some(Field::Primitive(text)) = self.atoms[atom].fields.get_mut(field) else {
            panic!("field `{}` is not a primitive", field);
        };
        text.replace_range(index .. index + remove, add);
    }
}
