use {
    aargvark::{
        Aargvark,
        vark,
    },
    loga::{
        ResultContext,
        ea,
    },
    merman::{
        config,
        load_document,
    },
    merman_core::{
        back::{
            back_locate,
            back_reference,
        },
        cursor::Located,
        reference::{
            Reference,
            Segment,
        },
        serialize::serialize_atom,
    },
    std::{
        path::PathBuf,
        process::exit,
    },
};

#[derive(Aargvark)]
struct Args {
    command: Command,
}

#[derive(Aargvark)]
#[vark(break_help)]
enum Command {
    Get(Get),
}

#[derive(Aargvark)]
struct Get {
    file: PathBuf,
    reference: String,
}

fn main() {
    match (|| -> Result<(), loga::Error> {
        let args = vark::<Args>();
        match args.command {
            Command::Get(get) => {
                let cwd = std::env::current_dir().context("Error getting the working directory")?;
                let file =
                    std::fs::canonicalize(
                        &get.file,
                    ).context_with("Error resolving file", ea!(file = get.file.display()))?;
                let dir = file.parent().unwrap_or(&cwd).to_path_buf();
                let config = config::config_load(&dir, &cwd)?;
                let (_, syntax, _, document) = load_document(&config, &file)?;
                let reference =
                    Reference::reference_parse(
                        &get.reference,
                    ).map_err(|e| loga::err_with(e, ea!(reference = get.reference)))?;
                let target =
                    back_locate(
                        &syntax,
                        &document,
                        &reference,
                    ).context_with("Nothing is at this reference", ea!(reference = get.reference))?;
                let id = match &target.located {
                    Located::Atom(a) => document.document_atom(*a).unique_id,
                    Located::Field(_, _) => None,
                };
                let start = match reference.id {
                    Some(id) => document
                        .atoms
                        .iter()
                        .position(|a| a.unique_id == Some(id) || a.back_ids.contains(&id))
                        .unwrap(),
                    None => document.root,
                };
                let mut value = serialize_atom(&syntax, &document, start);
                for segment in &reference.path {
                    value = match segment {
                        Segment::Key(key) => value[key.as_str()].take(),
                        Segment::Index(index) => value[*index].take(),
                    };
                }
                if let Some((begin, end)) = reference.range {
                    value = match value {
                        serde_json::Value::Array(elements) => {
                            let end = end.min(elements.len());
                            serde_json::Value::Array(elements.into_iter().take(end).skip(begin.min(end)).collect())
                        },
                        serde_json::Value::String(text) => {
                            let end = end.min(text.chars().count());
                            serde_json::Value::String(text.chars().take(end).skip(begin.min(end)).collect())
                        },
                        other => other,
                    };
                }
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "reference": back_reference(&syntax, &document, &target.located, target.range).reference_format(),
                    "id": id,
                    "value": value
                })).unwrap());
                return Ok(());
            },
        }
    })() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        },
    }
}
