use {
    aargvark::{
        Aargvark,
        vark,
    },
    loga::{
        ResultContext,
        ea,
    },
    merman_api::api::ReqTrait,
    merman_core::spec::SpecCompression,
    merman::{
        compress::{
            encode_document,
            format_document,
        },
        config,
        load_document,
    },
    merman_core::{
        back::{
            back_atom_segments,
            back_locate,
        },
        cursor::Located,
        document::Document,
        matcher::{
            match_document,
            source_parse,
        },
        reference::{
            Reference,
            Segment,
        },
        serialize::serialize_atom,
        syntax::Syntax,
    },
    serde_json::Value,
    std::{
        collections::HashSet,
        io::Read,
        path::{
            Path,
            PathBuf,
        },
    },
};

#[derive(Aargvark)]
struct Args {
    command: Command,
}

fn location_ref(location: &merman_langserver::Location) -> String {
    let source = match &location.source {
        merman_langserver::Source::Local { path } => path.clone(),
        merman_langserver::Source::Remote { spec } => spec.clone(),
    };
    return match location.expr {
        Some(expr) => format!("{}#{}", source, expr),
        None => source,
    };
}

#[derive(Aargvark)]
#[vark(break_help)]
enum Command {
    Get(Get),
    Find(Find),
    Set(Set),
    Delete(Delete),
    Compile(Compile),
}

#[derive(Aargvark)]
struct Compile {}

#[derive(Aargvark)]
struct Get {
    file: PathBuf,
    reference: String,
    up: Option<usize>,
    depth: Option<usize>,
}

#[derive(Aargvark)]
struct Find {
    file: PathBuf,
    pattern: String,
    within: Option<String>,
    regex: Option<()>,
    depth: Option<usize>,
}

#[derive(Aargvark)]
struct Set {
    file: PathBuf,
    reference: String,
}

#[derive(Aargvark)]
struct Delete {
    file: PathBuf,
    reference: String,
}

struct Loaded {
    compression: SpecCompression,
    document: Document,
    file: PathBuf,
    syntax: Syntax,
}

fn load(file: &Path) -> Result<Loaded, loga::Error> {
    let cwd = std::env::current_dir().context("Error getting the working directory")?;
    let file = std::fs::canonicalize(file).context_with("Error resolving file", ea!(file = file.display()))?;
    let dir = file.parent().unwrap_or(&cwd).to_path_buf();
    let config = config::config_load(&dir, &cwd)?;
    let loaded = load_document(&config, &file)?;
    return Ok(Loaded {
        compression: loaded.compression,
        document: loaded.document,
        file: file,
        syntax: loaded.syntax,
    });
}

fn root_serialize(loaded: &Loaded) -> Value {
    return serialize_atom(&loaded.syntax, &loaded.document, loaded.document.root);
}

fn reference_parse(text: &str) -> Result<Reference, loga::Error> {
    return Reference::reference_parse(text).map_err(|e| loga::err_with(e, ea!(reference = text)));
}

fn reference_path(
    loaded: &Loaded,
    reference: &Reference,
) -> Result<(Vec<Segment>, Option<(usize, usize)>), loga::Error> {
    back_locate(
        &loaded.syntax,
        &loaded.document,
        reference,
    ).context_with("Nothing is at this reference", ea!(reference = reference.reference_format()))?;
    let mut path = match reference.id {
        Some(id) => back_atom_segments(
            &loaded.syntax,
            &loaded.document,
            loaded
                .document
                .atoms
                .iter()
                .position(|a| a.unique_id == Some(id) || a.back_ids.contains(&id))
                .unwrap(),
        ),
        None => vec![],
    };
    path.extend(reference.path.iter().cloned());
    return Ok((path, reference.range));
}

fn value_walk<'a>(value: &'a mut Value, path: &[Segment]) -> &'a mut Value {
    let mut at = value;
    for segment in path {
        at = match segment {
            Segment::Key(key) => &mut at[key.as_str()],
            Segment::Index(index) => &mut at[*index],
        };
    }
    return at;
}

fn locate_json(
    syntax: &Syntax,
    document: &Document,
    path: &[Segment],
    range: Option<(usize, usize)>,
) -> (String, Option<i64>) {
    let mut reference = Reference {
        id: None,
        path: path.to_vec(),
        range: range,
    };
    let Some(target) = back_locate(syntax, document, &reference) else {
        return (reference.reference_format(), None);
    };
    let mut at = Some(match &target.located {
        Located::Atom(a) | Located::Field(a, _) => *a,
    });
    while let Some(a) = at {
        let atom = document.document_atom(a);
        if let Some(id) = atom.unique_id {
            reference.id = Some(id);
            reference.path.drain(..back_atom_segments(syntax, document, a).len());
            break;
        }
        at = atom.parent.as_ref().map(|p| p.atom);
    }
    if !reference.path.is_empty() || reference.range.is_some() {
        return (reference.reference_format(), None);
    }
    return (reference.reference_format(), reference.id);
}

fn pattern_match(pattern: &Value, value: &Value, regex: bool) -> bool {
    match (pattern, value) {
        (Value::Object(p), Value::Object(v)) => {
            return p.iter().all(|(k, pv)| v.get(k).is_some_and(|vv| pattern_match(pv, vv, regex)));
        },
        (Value::Array(p), Value::Array(v)) => {
            return p.len() == v.len() && p.iter().zip(v).all(|(pv, vv)| pattern_match(pv, vv, regex));
        },
        (Value::String(p), Value::String(v)) => {
            if regex {
                return regex::Regex::new(p).is_ok_and(|r| r.is_match(v));
            }
            return p == v;
        },
        _ => return pattern == value,
    }
}

fn value_truncate(value: &Value, depth: usize) -> Value {
    match value {
        Value::Object(o) => {
            if depth == 0 && !o.is_empty() {
                return Value::String("…".to_string());
            }
            return Value::Object(o.iter().map(|(k, v)| (k.clone(), value_truncate(v, depth - 1))).collect());
        },
        Value::Array(a) => {
            if depth == 0 && !a.is_empty() {
                return Value::String("…".to_string());
            }
            return Value::Array(a.iter().map(|v| value_truncate(v, depth - 1)).collect());
        },
        other => return other.clone(),
    }
}

fn find_walk(
    loaded: &Loaded,
    pattern: &Value,
    regex: bool,
    depth: usize,
    path: &mut Vec<Segment>,
    value: &Value,
    out: &mut Vec<Value>,
) {
    if pattern_match(pattern, value, regex) {
        let (reference, id) = locate_json(&loaded.syntax, &loaded.document, path, None);
        out.push(serde_json::json!({
            "reference": reference,
            "id": id,
            "value": value_truncate(value, depth)
        }));
    }
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                path.push(Segment::Key(k.clone()));
                find_walk(loaded, pattern, regex, depth, path, v, out);
                path.pop();
            }
        },
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                path.push(Segment::Index(i));
                find_walk(loaded, pattern, regex, depth, path, v, out);
                path.pop();
            }
        },
        _ => { },
    }
}

fn write(
    loaded: &Loaded,
    root: &Value,
    path: &[Segment],
    slice: Option<(usize, usize)>,
) -> Result<(), loga::Error> {
    let mut document = match match_document(&loaded.syntax, root) {
        Ok(d) => d,
        Err(e) => {
            return Err(
                loga::err_with(
                    format!("The edited file doesn't match the syntax, so it wasn't written:\n{}", e.mismatch_format()),
                    ea!(file = loaded.file.display()),
                ),
            );
        },
    };
    let mut inside = vec![];
    let mut seen = HashSet::new();
    let mut next = 1;
    for (index, atom) in document.atoms.iter().enumerate() {
        let atom_path = back_atom_segments(&loaded.syntax, &document, index);
        let in_region = atom_path.starts_with(path) && match slice {
            None => true,
            Some((begin, count)) => match atom_path.get(path.len()) {
                Some(Segment::Index(i)) => (begin .. begin + count).contains(i),
                _ => false,
            },
        };
        if in_region {
            inside.push(index);
        } else {
            seen.extend(atom.back_ids.iter().copied());
        }
        for id in &atom.back_ids {
            next = next.max(id + 1);
        }
    }
    for index in inside {
        let atom = &mut document.atoms[index];
        for i in 0 .. atom.back_ids.len() {
            let id = atom.back_ids[i];
            if id >= 0 && seen.insert(id) {
                continue;
            }
            atom.back_ids[i] = next;
            if atom.unique_id == Some(id) {
                atom.unique_id = Some(next);
            }
            next += 1;
        }
    }
    let text = format_document(&serialize_atom(&loaded.syntax, &document, document.root));
    let bytes = encode_document(&text, loaded.compression)?;
    let temp = loaded.file.with_extension("merman_tool_tmp");
    std::fs::write(&temp, bytes).context_with("Error writing the edited file", ea!(path = temp.display()))?;
    std::fs::rename(
        &temp,
        &loaded.file,
    ).context_with("Error replacing the file", ea!(path = loaded.file.display()))?;
    let (reference, id) = locate_json(&loaded.syntax, &document, path, None);
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "reference": reference,
        "id": id
    })).unwrap());
    return Ok(());
}

fn main() {
    match (|| -> Result<(), loga::Error> {
        let args = vark::<Args>();
        match args.command {
            Command::Get(get) => {
                let loaded = load(&get.file)?;
                let reference = reference_parse(&get.reference)?;
                let (mut path, mut range) = reference_path(&loaded, &reference)?;
                if let Some(up) = get.up.filter(|up| *up > 0) {
                    path.truncate(path.len().saturating_sub(up - range.is_some() as usize));
                    range = None;
                }
                let mut root = root_serialize(&loaded);
                let mut value = value_walk(&mut root, &path).take();
                if let Some((begin, end)) = range {
                    value = match value {
                        Value::Array(elements) => {
                            let end = end.min(elements.len());
                            Value::Array(elements.into_iter().take(end).skip(begin.min(end)).collect())
                        },
                        Value::String(text) => {
                            let end = end.min(text.chars().count());
                            Value::String(text.chars().take(end).skip(begin.min(end)).collect())
                        },
                        other => other,
                    };
                }
                if let Some(depth) = get.depth {
                    value = value_truncate(&value, depth);
                }
                let (reference, id) = locate_json(&loaded.syntax, &loaded.document, &path, range);
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "reference": reference,
                    "id": id,
                    "value": value
                })).unwrap());
                return Ok(());
            },
            Command::Find(find) => {
                let loaded = load(&find.file)?;
                let pattern =
                    serde_json::from_str::<Value>(
                        &find.pattern,
                    ).context_with("The pattern isn't valid JSON", ea!(pattern = find.pattern))?;
                if find.regex.is_some() {
                    let mut stack = vec![&pattern];
                    while let Some(v) = stack.pop() {
                        match v {
                            Value::String(p) => {
                                regex::Regex::new(
                                    p,
                                ).context_with("Bad regular expression in the pattern", ea!(pattern = p))?;
                            },
                            Value::Object(o) => stack.extend(o.values()),
                            Value::Array(a) => stack.extend(a),
                            _ => { },
                        }
                    }
                }
                let mut path = match &find.within {
                    Some(within) => reference_path(&loaded, &reference_parse(within)?)?.0,
                    None => vec![],
                };
                let mut root = root_serialize(&loaded);
                let value = value_walk(&mut root, &path).take();
                let mut out = vec![];
                find_walk(
                    &loaded,
                    &pattern,
                    find.regex.is_some(),
                    find.depth.unwrap_or(4),
                    &mut path,
                    &value,
                    &mut out,
                );
                println!("{}", serde_json::to_string_pretty(&Value::Array(out)).unwrap());
                return Ok(());
            },
            Command::Set(set) => {
                let loaded = load(&set.file)?;
                let reference = reference_parse(&set.reference)?;
                let (path, range) = reference_path(&loaded, &reference)?;
                let mut text = String::new();
                std::io::stdin().read_to_string(&mut text).context("Error reading the new value from stdin")?;
                let new = source_parse(&text).context("The new value on stdin isn't valid JSON")?;
                let mut root = root_serialize(&loaded);
                let at = value_walk(&mut root, &path);
                let mut slice = None;
                match range {
                    None => *at = new,
                    Some((begin, end)) => match (at, new) {
                        (Value::Array(elements), Value::Array(new)) => {
                            let end = end.min(elements.len());
                            let begin = begin.min(end);
                            slice = Some((begin, new.len()));
                            elements.splice(begin .. end, new);
                        },
                        (Value::String(text), Value::String(new)) => {
                            let end = end.min(text.chars().count());
                            let begin = begin.min(end);
                            let byte =
                                |i: usize| text.char_indices().nth(i).map(|(b, _)| b).unwrap_or(text.len());
                            text.replace_range(byte(begin) .. byte(end), &new);
                        },
                        (at, new) => {
                            return Err(
                                loga::err_with(
                                    "A slice can only be replaced by an array in an array or a string in a string",
                                    ea!(at = at, new = new),
                                ),
                            );
                        },
                    },
                }
                return write(&loaded, &root, &path, slice);
            },
            Command::Delete(delete) => {
                let loaded = load(&delete.file)?;
                let reference = reference_parse(&delete.reference)?;
                let (mut path, range) = reference_path(&loaded, &reference)?;
                let mut root = root_serialize(&loaded);
                if let Some((begin, end)) = range {
                    match value_walk(&mut root, &path) {
                        Value::Array(elements) => {
                            let end = end.min(elements.len());
                            elements.drain(begin.min(end) .. end);
                        },
                        Value::String(text) => {
                            let end = end.min(text.chars().count());
                            let begin = begin.min(end);
                            let byte =
                                |i: usize| text.char_indices().nth(i).map(|(b, _)| b).unwrap_or(text.len());
                            text.replace_range(byte(begin) .. byte(end), "");
                        },
                        other => {
                            return Err(
                                loga::err_with(
                                    "Only array elements and string characters can be sliced out",
                                    ea!(at = other),
                                ),
                            );
                        },
                    }
                } else {
                    let Some(last) = path.pop() else {
                        return Err(loga::err("The root of the file can't be deleted"));
                    };
                    match (value_walk(&mut root, &path), &last) {
                        (Value::Array(elements), Segment::Index(index)) => {
                            elements.remove(*index);
                        },
                        (Value::Object(entries), Segment::Key(key)) => {
                            entries.shift_remove(key);
                        },
                        _ => unreachable!(),
                    }
                }
                return write(&loaded, &root, &path, Some((0, 0)));
            },
            Command::Compile(_) => {
                let api_url =
                    std::env::var(
                        "MERMAN_API_URL",
                    ).context("MERMAN_API_URL isn't set; is this running in the editor's ai session?")?;
                let url: http::Uri =
                    format!("{}{}", api_url.trim_end_matches('/'), merman_api::API_PATH)
                        .parse()
                        .context_with("MERMAN_API_URL isn't a url", ea!(url = api_url))?;
                let rt =
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .context("Error starting async runtime")?;
                let resp: glove::Resp<merman_api::RespLangFlush> = rt.block_on(async {
                    let limits =
                        htwrap::htreq::Limits::default().with_read_body_time(std::time::Duration::from_secs(3600));
                    let mut conn = htwrap::htreq::connect(limits, &url).await?;
                    return htwrap::htreq::post_json(
                        &loga::Log::new_root(loga::WARN),
                        limits,
                        &mut conn,
                        &url,
                        &Default::default(),
                        merman_api::ReqLangFlush {}.to_enum(),
                    ).await;
                })?;
                let resp = match resp {
                    glove::Resp::Ok(r) => r,
                    glove::Resp::Err(e) => return Err(
                        loga::err_with("The editor refused the request", ea!(err = e)),
                    ),
                };
                let mut out = serde_json::Map::new();
                let mut failed = false;
                for server in resp.servers {
                    let compiled = match server.result {
                        Ok(compiled) => compiled,
                        Err(e) => {
                            failed = true;
                            out.insert(server.server, serde_json::json!({
                                "error": e
                            }));
                            continue;
                        },
                    };
                    let mut units = vec![];
                    for unit in compiled {
                        let mut errors = vec![];
                        for error in unit.errors {
                            failed = true;
                            errors.push(serde_json::json!({
                                "location": location_ref(&error.location),
                                "message": error.message,
                                "related": error.related.iter().map(|r| serde_json::json!({
                                    "location": location_ref(&r.location),
                                    "description": r.description
                                })).collect::< Vec < _ >>()
                            }));
                        }
                        units.push(serde_json::json!({
                            "source": location_ref(&merman_langserver::Location {
                                source: unit.source,
                                expr: None,
                            }),
                            "import_state": unit.import_state,
                            "errors": errors
                        }));
                    }
                    out.insert(server.server, Value::Array(units));
                }
                println!("{}", serde_json::to_string_pretty(&Value::Object(out)).unwrap());
                if failed {
                    std::process::exit(1);
                }
                return Ok(());
            },
        }
    })() {
        Ok(_) => (),
        Err(e) => loga::fatal(e),
    }
}
