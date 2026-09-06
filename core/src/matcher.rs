//! Structural matching of a JSON value against the syntax, producing a
//! document. Group members are tried in order; the first type whose back
//! matches wins.
use crate::document::{Atom, AtomId, AtomParent, Document, Field};
use crate::spec::SpecBack;
use crate::syntax::{Syntax, TypeId, TYPE_ROOT};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Write;

pub struct Mismatch {
    /// JSON pointer style path of the value that failed.
    pub path: String,
    pub message: String,
    /// For group matches: each candidate type and why it failed.
    pub alternatives: Vec<(String, Mismatch)>,
}

impl Mismatch {
    fn leaf(path: &str, message: String) -> Mismatch {
        return Mismatch {
            path: path.to_string(),
            message,
            alternatives: vec![],
        };
    }

    /// Multi-line explanation, most specific failures nested under the group
    /// they were tried for.
    pub fn mismatch_format(&self) -> String {
        let mut out = String::new();
        let mut lines = 0;
        self.format_into(&mut out, 0, &mut lines);
        return out;
    }

    fn format_into(&self, out: &mut String, depth: usize, lines: &mut usize) {
        const MAX_LINES: usize = 400;
        const MAX_DEPTH: usize = 8;
        if *lines >= MAX_LINES {
            return;
        }
        *lines += 1;
        let indent = "  ".repeat(depth);
        writeln!(out, "{}{}: {}", indent, self.path, self.message).unwrap();
        if depth >= MAX_DEPTH && !self.alternatives.is_empty() {
            writeln!(out, "{}  ...", indent).unwrap();
            return;
        }
        for (t, m) in &self.alternatives {
            if *lines >= MAX_LINES {
                writeln!(out, "{}  ...", indent).unwrap();
                return;
            }
            *lines += 1;
            writeln!(out, "{}  as `{}`:", indent, t).unwrap();
            m.format_into(out, depth + 2, lines);
        }
    }
}

pub fn match_document(syntax: &Syntax, value: &Value) -> Result<Document, Mismatch> {
    let mut m = Matcher {
        syntax,
        atoms: vec![],
    };
    let root = m.match_type(TYPE_ROOT, value, "")?;
    return Ok(Document {
        atoms: m.atoms,
        root,
    });
}

struct Matcher<'a> {
    syntax: &'a Syntax,
    atoms: Vec<Atom>,
}

fn describe(value: &Value) -> String {
    match value {
        Value::Null => return "null".to_string(),
        Value::Bool(b) => return format!("{}", b),
        Value::Number(n) => return format!("number {}", n),
        Value::String(s) => return format!("string {:?}", s),
        Value::Array(a) => return format!("array of {}", a.len()),
        Value::Object(o) => {
            return format!(
                "object with keys [{}]",
                o.keys().map(|k| k.as_str()).collect::<Vec<_>>().join(", ")
            )
        }
    }
}

fn literal_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => return Some("null".to_string()),
        Value::Bool(b) => return Some(b.to_string()),
        Value::Number(n) => return Some(n.to_string()),
        _ => return None,
    }
}

impl<'a> Matcher<'a> {
    fn match_type(&mut self, type_id: TypeId, value: &Value, path: &str) -> Result<AtomId, Mismatch> {
        let mark = self.atoms.len();
        let id = mark;
        self.atoms.push(Atom {
            type_: type_id,
            fields: HashMap::new(),
            parent: None,
        });
        let mut fields = HashMap::new();
        let back = &self.syntax.syntax_type(type_id).back;
        match self.match_back(back, value, path, id, &mut fields) {
            Ok(()) => {
                self.atoms[id].fields = fields;
                return Ok(id);
            }
            Err(e) => {
                self.atoms.truncate(mark);
                return Err(e);
            }
        }
    }

    /// Match a record entry against a type whose back is a `pair`.
    fn match_type_pair(
        &mut self,
        type_id: TypeId,
        key: &str,
        value: &Value,
        path: &str,
    ) -> Result<AtomId, Mismatch> {
        let mark = self.atoms.len();
        let id = mark;
        self.atoms.push(Atom {
            type_: type_id,
            fields: HashMap::new(),
            parent: None,
        });
        let mut fields = HashMap::new();
        let SpecBack::Pair(pair) = &self.syntax.syntax_type(type_id).back else {
            panic!("record element type is not a pair; syntax validation should have caught this");
        };
        let key_value = Value::String(key.to_string());
        let res = self
            .match_back(&pair.key, &key_value, &format!("{}(key)", path), id, &mut fields)
            .and_then(|_| self.match_back(&pair.value, value, path, id, &mut fields));
        match res {
            Ok(()) => {
                self.atoms[id].fields = fields;
                return Ok(id);
            }
            Err(e) => {
                self.atoms.truncate(mark);
                return Err(e);
            }
        }
    }

    fn match_group(&mut self, group: &str, value: &Value, path: &str) -> Result<AtomId, Mismatch> {
        let candidates = self
            .syntax
            .groups
            .get(group)
            .unwrap_or_else(|| panic!("unknown group `{}`; syntax validation should have caught this", group))
            .clone();
        let mut alternatives = vec![];
        for t in candidates {
            match self.match_type(t, value, path) {
                Ok(a) => return Ok(a),
                Err(e) => alternatives.push((self.syntax.syntax_type(t).id.clone(), e)),
            }
        }
        return Err(Mismatch {
            path: path.to_string(),
            message: format!("{} matched no type in `{}`", describe(value), group),
            alternatives,
        });
    }

    fn match_group_pair(
        &mut self,
        group: &str,
        key: &str,
        value: &Value,
        path: &str,
    ) -> Result<AtomId, Mismatch> {
        let candidates = self
            .syntax
            .groups
            .get(group)
            .unwrap_or_else(|| panic!("unknown group `{}`; syntax validation should have caught this", group))
            .clone();
        let mut alternatives = vec![];
        for t in candidates {
            match self.match_type_pair(t, key, value, path) {
                Ok(a) => return Ok(a),
                Err(e) => alternatives.push((self.syntax.syntax_type(t).id.clone(), e)),
            }
        }
        return Err(Mismatch {
            path: path.to_string(),
            message: format!("entry `{}` matched no type in `{}`", key, group),
            alternatives,
        });
    }

    fn match_back(
        &mut self,
        back: &SpecBack,
        value: &Value,
        path: &str,
        owner: AtomId,
        fields: &mut HashMap<String, Field>,
    ) -> Result<(), Mismatch> {
        match back {
            SpecBack::FixedString(want) => match value {
                Value::String(s) if s == want => return Ok(()),
                _ => {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected string {:?}, got {}", want, describe(value)),
                    ))
                }
            },
            SpecBack::FixedLiteral(want) => match literal_text(value) {
                Some(t) if &t == want => return Ok(()),
                _ => {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected literal {}, got {}", want, describe(value)),
                    ))
                }
            },
            SpecBack::String(f) => match value {
                Value::String(s) => {
                    fields.insert(f.id.clone(), Field::Primitive(s.clone()));
                    return Ok(());
                }
                _ => {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected a string, got {}", describe(value)),
                    ))
                }
            },
            SpecBack::Number(f) => match value {
                Value::Number(n) => {
                    fields.insert(f.id.clone(), Field::Primitive(n.to_string()));
                    return Ok(());
                }
                _ => {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected a number, got {}", describe(value)),
                    ))
                }
            },
            SpecBack::Literal(f) => match literal_text(value) {
                Some(t) => {
                    fields.insert(f.id.clone(), Field::Primitive(t));
                    return Ok(());
                }
                None => {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected null, a boolean or a number, got {}", describe(value)),
                    ))
                }
            },
            SpecBack::Atom(a) => {
                let child = self.match_group(&a.type_, value, path)?;
                self.atoms[child].parent = Some(AtomParent {
                    atom: owner,
                    field: a.id.clone(),
                    index: 0,
                });
                fields.insert(a.id.clone(), Field::Atom(child));
                return Ok(());
            }
            SpecBack::Array(a) => {
                let Value::Array(elems) = value else {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected an array, got {}", describe(value)),
                    ));
                };
                let mut out = vec![];
                for (i, e) in elems.iter().enumerate() {
                    let child = self.match_group(&a.element, e, &format!("{}/{}", path, i))?;
                    self.atoms[child].parent = Some(AtomParent {
                        atom: owner,
                        field: a.id.clone(),
                        index: i,
                    });
                    out.push(child);
                }
                fields.insert(a.id.clone(), Field::Array(out));
                return Ok(());
            }
            SpecBack::Optional(a) => {
                let Value::Object(o) = value else {
                    return Err(Mismatch::leaf(
                        path,
                        format!(
                            "expected object {{{}: ...}} or {{{}: null}}, got {}",
                            a.some_key,
                            a.none_key,
                            describe(value)
                        ),
                    ));
                };
                if o.len() != 1 {
                    return Err(Mismatch::leaf(
                        path,
                        format!(
                            "expected object with single key {} or {}, got {}",
                            a.some_key,
                            a.none_key,
                            describe(value)
                        ),
                    ));
                }
                let (k, v) = o.iter().next().unwrap();
                if k == &a.none_key {
                    if !v.is_null() {
                        return Err(Mismatch::leaf(
                            &format!("{}/{}", path, k),
                            format!("expected null, got {}", describe(v)),
                        ));
                    }
                    fields.insert(a.id.clone(), Field::Array(vec![]));
                    return Ok(());
                }
                if k == &a.some_key {
                    let child = self.match_group(&a.element, v, &format!("{}/{}", path, k))?;
                    self.atoms[child].parent = Some(AtomParent {
                        atom: owner,
                        field: a.id.clone(),
                        index: 0,
                    });
                    fields.insert(a.id.clone(), Field::Array(vec![child]));
                    return Ok(());
                }
                return Err(Mismatch::leaf(
                    path,
                    format!(
                        "expected key {} or {}, got key {}",
                        a.some_key, a.none_key, k
                    ),
                ));
            }
            SpecBack::Record(a) => {
                let Value::Object(o) = value else {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected an object, got {}", describe(value)),
                    ));
                };
                let mut out = vec![];
                for (i, (k, v)) in o.iter().enumerate() {
                    let child = self.match_group_pair(&a.element, k, v, &format!("{}/{}", path, k))?;
                    self.atoms[child].parent = Some(AtomParent {
                        atom: owner,
                        field: a.id.clone(),
                        index: i,
                    });
                    out.push(child);
                }
                fields.insert(a.id.clone(), Field::Array(out));
                return Ok(());
            }
            SpecBack::Pair(_) => {
                panic!("pair back outside record element; syntax validation should have caught this")
            }
            SpecBack::FixedArray(specs) => {
                let Value::Array(elems) = value else {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected an array, got {}", describe(value)),
                    ));
                };
                if elems.len() != specs.len() {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected an array of {}, got {}", specs.len(), describe(value)),
                    ));
                }
                for (i, (s, e)) in specs.iter().zip(elems.iter()).enumerate() {
                    self.match_back(s, e, &format!("{}/{}", path, i), owner, fields)?;
                }
                return Ok(());
            }
            SpecBack::FixedRecord(entries) => {
                let Value::Object(o) = value else {
                    return Err(Mismatch::leaf(
                        path,
                        format!("expected an object, got {}", describe(value)),
                    ));
                };
                let mut ok = o.len() == entries.len();
                if ok {
                    for e in entries {
                        if !o.contains_key(&e.key) {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    return Err(Mismatch::leaf(
                        path,
                        format!(
                            "expected object with keys [{}], got {}",
                            entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>().join(", "),
                            describe(value)
                        ),
                    ));
                }
                for e in entries {
                    self.match_back(
                        &e.value,
                        o.get(&e.key).unwrap(),
                        &format!("{}/{}", path, e.key),
                        owner,
                        fields,
                    )?;
                }
                return Ok(());
            }
            SpecBack::Discard(_) => return Ok(()),
        }
    }
}
