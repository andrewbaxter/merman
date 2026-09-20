use crate::document::{
    AtomId,
    Document,
    Field,
};
use crate::spec::SpecBack;
use crate::syntax::Syntax;
use serde_json::{
    Map,
    Number,
    Value,
};

pub fn serialize_atom(syntax: &Syntax, document: &Document, atom: AtomId) -> Value {
    let a = document.document_atom(atom);
    return write_back(syntax, document, atom, &syntax.syntax_type(a.type_).back);
}

pub fn serialize_pair(syntax: &Syntax, document: &Document, atom: AtomId) -> (String, Value) {
    let a = document.document_atom(atom);
    let SpecBack::Pair(pair) = &syntax.syntax_type(a.type_).back else {
        panic!("serializing a non-pair atom as a record entry");
    };
    let Value::String(key) = write_back(syntax, document, atom, &pair.key) else {
        panic!("pair key back did not produce a string; syntax validation should have caught this");
    };
    return (key, write_back(syntax, document, atom, &pair.value));
}

fn literal(text: &str) -> Value {
    match text {
        "null" => return Value::Null,
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => match text.parse::<Number>() {
            Ok(n) => return Value::Number(n),
            Err(_) => panic!("literal field `{}` is not null, a boolean or a number", text),
        },
    }
}

fn write_back(syntax: &Syntax, document: &Document, atom: AtomId, back: &SpecBack) -> Value {
    let a = document.document_atom(atom);
    let field = |id: &str| {
        a.fields.get(id).unwrap_or_else(|| panic!("atom is missing field `{}`", id))
    };
    match back {
        SpecBack::FixedString(s) => return Value::String(s.clone()),
        SpecBack::FixedLiteral(s) => return literal(s),
        SpecBack::String(f) => {
            let Field::Primitive(s) = field(&f.id) else {
                panic!("field `{}` is not a primitive", f.id);
            };
            return Value::String(s.clone());
        },
        SpecBack::Number(f) | SpecBack::Literal(f) => {
            let Field::Primitive(s) = field(&f.id) else {
                panic!("field `{}` is not a primitive", f.id);
            };
            return literal(s);
        },
        SpecBack::Atom(f) => {
            let Field::Atom(child) = field(&f.id) else {
                panic!("field `{}` is not an atom", f.id);
            };
            return serialize_atom(syntax, document, *child);
        },
        SpecBack::Array(f) => {
            let Field::Array(children) = field(&f.id) else {
                panic!("field `{}` is not an array", f.id);
            };
            return Value::Array(children.iter().map(|c| serialize_atom(syntax, document, *c)).collect());
        },
        SpecBack::Optional(f) => {
            let Field::Array(children) = field(&f.id) else {
                panic!("field `{}` is not an array", f.id);
            };
            let mut out = Map::new();
            match children.first() {
                Some(c) => {
                    out.insert(f.some_key.clone(), serialize_atom(syntax, document, *c));
                },
                None => {
                    out.insert(f.none_key.clone(), Value::Null);
                },
            }
            return Value::Object(out);
        },
        SpecBack::Record(f) => {
            let Field::Array(children) = field(&f.id) else {
                panic!("field `{}` is not an array", f.id);
            };
            let mut out = Map::new();
            for c in children {
                let (k, v) = serialize_pair(syntax, document, *c);
                out.insert(k, v);
            }
            return Value::Object(out);
        },
        SpecBack::Pair(_) => panic!("pair back outside a record"),
        SpecBack::FixedArray(elems) => {
            return Value::Array(elems.iter().map(|e| write_back(syntax, document, atom, e)).collect());
        },
        SpecBack::FixedRecord(entries) => {
            let mut out = Map::new();
            for e in entries {
                out.insert(e.key.clone(), write_back(syntax, document, atom, &e.value));
            }
            return Value::Object(out);
        },
        SpecBack::Discard(_) => return Value::Null,
    }
}
