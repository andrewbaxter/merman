use aargvark::{
    vark,
    Aargvark,
};
use htwrap::htserve::handler::{
    Handler,
    HandlerArgs,
};
use htwrap::htserve::responses::{
    body_full,
    response_200_html,
    response_400,
    response_404,
    Body,
};
use futures::{
    SinkExt,
    StreamExt,
};
use http::header::CONTENT_TYPE;
use http::Response;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::Request;
use hyper_tungstenite::tungstenite::Message;
use hyper_util::rt::TokioIo;
use loga::{
    ea,
    ResultContext,
};
use merman3_api::api::{
    Req,
    ServerReq,
    ServerResp,
};
use merman3_api::{
    AiMessage,
    AiRole,
    AiStatus,
    ListEntry,
    RespAiClear,
    RespAiHistory,
    RespAiSend,
    RespList,
    RespOpen,
    RespStart,
    WsClient,
    WsServer,
    API_PATH,
    WS_PATH,
};
use merman3_core::cursor::Located;
use merman3_core::document::Document;
use merman3_core::keys::Keymap;
use merman3_core::matcher::match_document;
use merman3_core::serialize::serialize_atom;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use notify::{
    EventKind,
    RecursiveMode,
    Watcher,
};
use std::path::{
    Path,
    PathBuf,
};
use std::process::exit;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tao::event::{
    Event,
    WindowEvent,
};
use tao::event_loop::{
    ControlFlow,
    EventLoop,
};
use tao::window::WindowBuilder;
use tokio::io::{
    AsyncBufReadExt,
    AsyncReadExt,
    AsyncWriteExt,
    BufReader,
};
use tokio::process::Command;
use tokio::sync::broadcast;
use wry::WebViewBuilder;

mod ai;
mod config;
mod events;

#[derive(Aargvark)]
struct Locate {
    file: PathBuf,
    reference: String,
}

/// View a JSON source file using a merman3 syntax definition.
#[derive(Aargvark)]
struct Args {
    /// Path to the source JSON to view
    source: Option<PathBuf>,
    browser: Option<()>,
    /// Port to listen on; a free port is chosen if unspecified
    port: Option<u16>,
    /// Don't open the page in a browser
    no_open: Option<()>,
    locate: Option<Locate>,
}

struct HandlerRoot {
    html: Vec<u8>,
    config: config::Config,
    dir: PathBuf,
    file: Option<PathBuf>,
    keys: String,
    events: Arc<events::Events>,
    ai: Arc<ai::Ai>,
}

impl HandlerRoot {
    fn dir_contains(&self, path: &Path) -> Result<PathBuf, loga::Error> {
        let path = std::fs::canonicalize(path).context_with("Error resolving path", ea!(path = path.display()))?;
        if !path.starts_with(&self.dir) {
            return Err(
                loga::err_with(
                    "The path is outside the directory the editor was started in",
                    ea!(path = path.display(), dir = self.dir.display()),
                ),
            );
        }
        return Ok(path);
    }
}

fn response_bytes(data: &'static [u8], content_type: &str) -> Response<Body> {
    return Response::builder()
        .status(200)
        .header(CONTENT_TYPE, content_type)
        .body(body_full(data.to_vec()))
        .unwrap();
}

fn display(path: &Path) -> String {
    return path.to_string_lossy().into_owned();
}

#[htwrap::htserve::handler::async_trait::async_trait]
impl Handler<Body> for HandlerRoot {
    async fn handle(&self, args: HandlerArgs<'_>) -> Response<Body> {
        if args.subpath == API_PATH {
            let body = match args.body.collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => return response_400(format!("Error reading request body: {}", e)),
            };
            let req = match serde_json::from_slice::<Req>(&body) {
                Ok(r) => r,
                Err(e) => {
                    return response_400(
                        format!("Error parsing request: {}\nBody: {}", e, String::from_utf8_lossy(&body)),
                    );
                },
            };
            let resp = match req.to_server_req() {
                ServerReq::Start(respond, _) => respond(RespStart {
                    dir: display(&self.dir),
                    file: self.file.as_deref().map(display),
                    keys: self.keys.clone(),
                }),
                ServerReq::List(respond, req) => match (|| -> Result<RespList, loga::Error> {
                    let dir = self.dir_contains(Path::new(&req.dir))?;
                    let mut dirs = vec![];
                    let mut files = vec![];
                    for entry in std::fs::read_dir(
                        &dir,
                    ).context_with("Error reading directory", ea!(dir = dir.display()))? {
                        let Ok(entry) = entry else {
                            continue;
                        };
                        let Ok(kind) = entry.file_type() else {
                            continue;
                        };
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let is_dir = if kind.is_symlink() {
                            match std::fs::metadata(&path) {
                                Ok(m) => m.is_dir(),
                                Err(_) => continue,
                            }
                        } else {
                            kind.is_dir()
                        };
                        if is_dir {
                            dirs.push(ListEntry {
                                name: name,
                                path: display(&path),
                                dir: true,
                            });
                        } else if self.config.config_mapping(&path).is_some() {
                            files.push(ListEntry {
                                name: name,
                                path: display(&path),
                                dir: false,
                            });
                        }
                    }
                    dirs.sort_by(|a, b| a.name.cmp(&b.name));
                    files.sort_by(|a, b| a.name.cmp(&b.name));
                    dirs.append(&mut files);
                    return Ok(RespList {
                        parent: if dir == self.dir {
                            None
                        } else {
                            dir.parent().map(display)
                        },
                        dir: display(&dir),
                        entries: dirs,
                    });
                })() {
                    Ok(v) => respond(v),
                    Err(e) => ServerResp::err(e.to_string()),
                },
                ServerReq::AiSend(respond, req) => {
                    let result: Result<(), loga::Error> = async {
                        let ai = &self.ai;
                        let mut state = ai.state.lock().await;
                        if req.text.trim() == "/cancel" {
                            let Some(session) = state.session.as_mut() else {
                                return Err(loga::err("There is no session to cancel"));
                            };
                            let request = serde_json::json!({
                                "type": "control_request",
                                "request_id": format !("cancel-{}", ai::now_ms()),
                                "request": {
                                    "subtype": "interrupt"
                                }
                            });
                            session
                                .stdin
                                .write_all(format!("{}\n", request).as_bytes())
                                .await
                                .context("Error sending the interrupt to claude")?;
                            ai::ai_message(&session.log, &ai.events, AiRole::System, "Cancel requested".to_string())?;
                            return Ok(());
                        }
                        if state.session.is_none() {
                            std::fs::create_dir_all(
                                &ai.logs,
                            ).context_with("Error creating the transcript directory", ea!(path = ai.logs.display()))?;
                            let log = ai.logs.join(format!("{}.jsonl", ai::now_ms()));
                            let home =
                                directories::BaseDirs::new()
                                    .context("Error finding the home directory")?
                                    .home_dir()
                                    .to_path_buf();
                            let mut cmd = Command::new("bwrap");
                            cmd.args(
                                [
                                    "--unshare-all",
                                    "--share-net",
                                    "--die-with-parent",
                                    "--proc",
                                    "/proc",
                                    "--dev",
                                    "/dev",
                                    "--tmpfs",
                                    "/tmp",
                                ],
                            );
                            let mut bound: Vec<PathBuf> = vec![];
                            let mut bind_ro = |cmd: &mut Command, path: &Path| {
                                if bound.iter().any(|b| path.starts_with(b)) || !path.exists() {
                                    return;
                                }
                                cmd.arg("--ro-bind").arg(path).arg(path);
                                bound.push(path.to_path_buf());
                            };
                            for root in [
                                "/nix",
                                "/usr",
                                "/lib",
                                "/lib64",
                                "/bin",
                                "/sbin",
                                "/etc",
                                "/opt",
                                "/run/current-system",
                            ] {
                                bind_ro(&mut cmd, Path::new(root));
                            }
                            let exe_dir =
                                std::env::current_exe()
                                    .context("Error finding the editor's own binary")?
                                    .parent()
                                    .context("The editor's own binary has no directory")?
                                    .to_path_buf();
                            let mut path_dirs = vec![exe_dir];
                            path_dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
                            for dir in &path_dirs {
                                if let Ok(dir) = std::fs::canonicalize(dir) {
                                    bind_ro(&mut cmd, &dir);
                                }
                            }
                            cmd.arg("--dir").arg(&home);
                            bind_ro(&mut cmd, &home.join(".claude"));
                            bind_ro(&mut cmd, &home.join(".claude.json"));
                            cmd.arg("--bind").arg(&ai.dir).arg(&ai.dir).arg("--chdir").arg(&ai.dir);
                            cmd.arg("--setenv").arg("HOME").arg(&home);
                            cmd
                                .arg("--setenv")
                                .arg("PATH")
                                .arg(std::env::join_paths(&path_dirs).context("Error building the sandbox PATH")?);
                            cmd.arg("--setenv").arg("TERM").arg("dumb");
                            cmd.args(
                                [
                                    "--",
                                    "claude",
                                    "-p",
                                    "--input-format",
                                    "stream-json",
                                    "--output-format",
                                    "stream-json",
                                    "--verbose",
                                    "--permission-mode",
                                    "bypassPermissions",
                                    "--no-session-persistence",
                                    "--append-system-prompt",
                                    "The user is editing this project's syntax tree files in the merman3 editor. They may \
                                    refer to locations as FILE#ID or FILE#ID/PATH (PATH is a syntax path like \
                                    named/field/2 below the element with that id) or FILE#/PATH from the root. Run \
                                    `merman3 --locate FILE REF` (REF is the part from `#` on, or a JSON pointer like \
                                    /v1/expr) to print the element's JSON pointer, id and value. Every syntax element \
                                    carries its id at `id.value` in the file.",
                                ],
                            );
                            cmd
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .stderr(Stdio::piped())
                                .kill_on_drop(true);
                            let mut child = cmd.spawn().context("Error starting claude in the sandbox")?;
                            let stdin = child.stdin.take().unwrap();
                            let stdout = child.stdout.take().unwrap();
                            let mut stderr = child.stderr.take().unwrap();
                            let reader = tokio::spawn({
                                let ai = ai.clone();
                                let log = log.clone();
                                async move {
                                    let stderr_task = tokio::spawn(async move {
                                        let mut out = String::new();
                                        _ = stderr.read_to_string(&mut out).await;
                                        return out;
                                    });
                                    let mut lines = BufReader::new(stdout).lines();
                                    while let Ok(Some(line)) = lines.next_line().await {
                                        let value = match serde_json::from_str::<serde_json::Value>(&line) {
                                            Ok(v) => v,
                                            Err(e) => {
                                                _ =
                                                    ai::ai_message(
                                                        &log,
                                                        &ai.events,
                                                        AiRole::System,
                                                        format!("Unreadable output from claude ({}): {}", e, line),
                                                    );
                                                continue;
                                            },
                                        };
                                        match value["type"].as_str() {
                                            Some("assistant") => {
                                                let Some(blocks) = value["message"]["content"].as_array() else {
                                                    continue;
                                                };
                                                for block in blocks {
                                                    match block["type"].as_str() {
                                                        Some("text") => {
                                                            let text = block["text"].as_str().unwrap_or_default();
                                                            if text.is_empty() {
                                                                continue;
                                                            }
                                                            _ =
                                                                ai::ai_message(
                                                                    &log,
                                                                    &ai.events,
                                                                    AiRole::Assistant,
                                                                    text.to_string(),
                                                                );
                                                        },
                                                        Some("tool_use") => {
                                                            _ =
                                                                ai::ai_message(
                                                                    &log,
                                                                    &ai.events,
                                                                    AiRole::Tool,
                                                                    format!(
                                                                        "{} {}",
                                                                        block["name"].as_str().unwrap_or_default(),
                                                                        serde_json::to_string(&block["input"]).unwrap()
                                                                    ),
                                                                );
                                                        },
                                                        _ => { },
                                                    }
                                                }
                                            },
                                            Some("control_response") => {
                                                let response = &value["response"];
                                                if response["subtype"].as_str() == Some("error") {
                                                    _ =
                                                        ai::ai_message(
                                                            &log,
                                                            &ai.events,
                                                            AiRole::System,
                                                            format!(
                                                                "Claude rejected the request: {}",
                                                                response["error"].as_str().unwrap_or_default()
                                                            ),
                                                        );
                                                }
                                            },
                                            Some("result") => {
                                                if value["is_error"].as_bool() == Some(true) {
                                                    _ =
                                                        ai::ai_message(
                                                            &log,
                                                            &ai.events,
                                                            AiRole::System,
                                                            value["result"]
                                                                .as_str()
                                                                .unwrap_or("Claude reported an error")
                                                                .to_string(),
                                                        );
                                                }
                                                let mut state = ai.state.lock().await;
                                                ai::ai_status_set(&mut state, &ai.events, AiStatus::Waiting);
                                            },
                                            _ => { },
                                        }
                                    }
                                    let stderr = stderr_task.await.unwrap_or_default();
                                    let mut state = ai.state.lock().await;
                                    let Some(mut session) = state.session.take() else {
                                        return;
                                    };
                                    let status =
                                        match tokio::time::timeout(
                                            Duration::from_secs(2),
                                            session.child.wait(),
                                        ).await {
                                            Ok(Ok(status)) => status.to_string(),
                                            _ => {
                                                _ = session.child.kill().await;
                                                "killed".to_string()
                                            },
                                        };
                                    let stderr = stderr.trim();
                                    _ = ai::ai_message(&log, &ai.events, AiRole::System, if stderr.is_empty() {
                                        format!("Claude exited ({})", status)
                                    } else {
                                        format!("Claude exited ({}):\n{}", status, stderr)
                                    });
                                    ai::ai_status_set(&mut state, &ai.events, AiStatus::Off);
                                }
                            });
                            state.session = Some(ai::AiSession {
                                child: child,
                                stdin: stdin,
                                log: log,
                                reader: reader,
                            });
                        }
                        let session = state.session.as_mut().unwrap();
                        ai::ai_message(&session.log, &ai.events, AiRole::User, req.text.clone())?;
                        let line = serde_json::json!({
                            "type": "user",
                            "message": {
                                "role": "user",
                                "content": req.text
                            }
                        });
                        session
                            .stdin
                            .write_all(format!("{}\n", line).as_bytes())
                            .await
                            .context("Error sending the message to claude")?;
                        ai::ai_status_set(&mut state, &ai.events, AiStatus::Thinking);
                        return Ok(());
                    }.await;
                    match result {
                        Ok(()) => respond(RespAiSend {}),
                        Err(e) => ServerResp::err(e.to_string()),
                    }
                },
                ServerReq::AiClear(respond, _) => {
                    let mut state = self.ai.state.lock().await;
                    if let Some(mut session) = state.session.take() {
                        session.reader.abort();
                        _ = session.child.kill().await;
                    }
                    ai::ai_status_set(&mut state, &self.ai.events, AiStatus::Off);
                    respond(RespAiClear {})
                },
                ServerReq::AiHistory(respond, _) => {
                    let result: Result<RespAiHistory, loga::Error> = async {
                        let state = self.ai.state.lock().await;
                        let mut messages = vec![];
                        if let Some(session) = &state.session {
                            let text =
                                std::fs::read_to_string(
                                    &session.log,
                                ).context_with("Error reading the transcript", ea!(path = session.log.display()))?;
                            for line in text.lines() {
                                messages.push(
                                    serde_json::from_str::<AiMessage>(
                                        line,
                                    ).context_with("Error parsing the transcript", ea!(path = session.log.display()))?,
                                );
                            }
                        }
                        return Ok(RespAiHistory {
                            status: state.status,
                            messages: messages,
                        });
                    }.await;
                    match result {
                        Ok(v) => respond(v),
                        Err(e) => ServerResp::err(e.to_string()),
                    }
                },
                ServerReq::Open(respond, req) => match (|| -> Result<RespOpen, loga::Error> {
                    let path = self.dir_contains(Path::new(&req.path))?;
                    let Some(mapping) = self.config.config_mapping(&path) else {
                        let mut known = self.config.extensions.keys().cloned().collect::<Vec<_>>();
                        known.sort();
                        return Err(
                            loga::err_with(
                                "No syntax is configured for this file's extension",
                                ea!(
                                    source = path.display(),
                                    configured_extensions = known.join(", "),
                                    config_files = self.config.config_files()
                                ),
                            ),
                        );
                    };
                    let syntax =
                        std::fs::read_to_string(
                            &mapping.syntax,
                        ).context_with(
                            "Error reading the syntax configured for this file",
                            ea!(syntax = mapping.syntax.display(), config = mapping.config.display()),
                        )?;
                    let source =
                        std::fs::read_to_string(
                            &path,
                        ).context_with("Error reading file", ea!(path = path.display()))?;
                    return Ok(RespOpen {
                        syntax: syntax,
                        source: source,
                    });
                })() {
                    Ok(v) => respond(v),
                    Err(e) => ServerResp::err(e.to_string()),
                },
            };
            return Response::builder()
                .status(200)
                .header(CONTENT_TYPE, "application/json")
                .body(body_full(resp.0))
                .unwrap();
        }
        return match args.subpath {
            "" => response_200_html(self.html.clone()),
            "/merman3.css" => response_bytes(include_bytes!("../static/merman3.css"), "text/css"),
            "/merman3_web.js" => response_bytes(include_bytes!("../static/merman3_web.js"), "text/javascript"),
            "/merman3_web_bg.wasm" => response_bytes(
                include_bytes!("../static/merman3_web_bg.wasm"),
                "application/wasm",
            ),
            "/MaterialIcons-Regular.ttf" => response_bytes(
                include_bytes!("../static/MaterialIcons-Regular.ttf"),
                "font/ttf",
            ),
            _ => response_404(),
        };
    }
}

/// Make JSON safe to embed in a `<script>` element.
fn embed_json(json: &str) -> String {
    return json.replace("</", "<\\/").replace("<!--", "<\\u0021--");
}

fn load_document(config: &config::Config, source: &Path) -> Result<(String, Syntax, String, Document), loga::Error> {
    let Some(mapping) = config.config_mapping(source) else {
        let mut known = config.extensions.keys().cloned().collect::<Vec<_>>();
        known.sort();
        return Err(
            loga::err_with(
                "No syntax is configured for this file's extension",
                ea!(
                    source = source.display(),
                    configured_extensions = known.join(", "),
                    config_files = config.config_files()
                ),
            ),
        );
    };
    let syntax_path = &mapping.syntax;
    let syntax_text =
        std::fs::read_to_string(
            syntax_path,
        ).context_with(
            "Error reading the syntax configured for this file",
            ea!(syntax = syntax_path.display(), config = mapping.config.display()),
        )?;
    let spec =
        serde_json::from_str::<SpecSyntax>(
            &syntax_text,
        ).context_with("Error parsing syntax", ea!(syntax = syntax_path.display()))?;
    let syntax = match Syntax::syntax_resolve(spec) {
        Ok(s) => s,
        Err(errors) => {
            return Err(
                loga::agg_err_with(
                    "Errors in syntax",
                    errors.0.into_iter().map(|e| loga::err(e.to_string())).collect(),
                    ea!(syntax = syntax_path.display()),
                ),
            );
        },
    };
    let source_text =
        std::fs::read_to_string(source).context_with("Error reading file", ea!(path = source.display()))?;
    let value =
        serde_json::from_str::<serde_json::Value>(
            &source_text,
        ).context_with("Error parsing source", ea!(source = source.display()))?;
    let document = match match_document(&syntax, &value) {
        Ok(d) => d,
        Err(e) => {
            return Err(
                loga::err_with(
                    format!("Source doesn't match syntax:\n{}", e.mismatch_format()),
                    ea!(source = source.display(), syntax = syntax_path.display()),
                ),
            );
        },
    };
    return Ok((syntax_text, syntax, source_text, document));
}

fn main() {
    match (|| -> Result<(), loga::Error> {
        let args = vark::<Args>();
        let cwd = std::env::current_dir().context("Error getting the working directory")?;
        let source =
            args.source.or_else(|| args.locate.as_ref().map(|l| l.file.clone())).unwrap_or_else(|| cwd.clone());
        let source_abs = std::fs::canonicalize(&source).unwrap_or_else(|_| source.clone());
        let file = if source_abs.is_dir() {
            None
        } else {
            Some(source_abs.clone())
        };
        let dir = match &file {
            Some(file) => file.parent().unwrap_or(&cwd).to_path_buf(),
            None => source_abs.clone(),
        };
        let config = {
            let mut extensions = std::collections::HashMap::new();
            let mut keys = config::SpecKeys::default();
            let mut sources = vec![];
            let dirs = {
                let mut out = vec![];
                let mut add = |dir: PathBuf| {
                    if !out.contains(&dir) {
                        out.push(dir);
                    }
                };
                for ancestor in dir.ancestors() {
                    add(ancestor.to_path_buf());
                }
                for ancestor in cwd.ancestors() {
                    add(ancestor.to_path_buf());
                }
                if let Some(dirs) = directories::BaseDirs::new() {
                    let config_dir = dirs.config_dir();
                    add(config_dir.join("merman"));
                    add(config_dir.to_path_buf());
                }
                out
            };
            for dir in dirs {
                for name in ["merman.json", ".merman.json"] {
                    let path = dir.join(name);
                    if !path.is_file() {
                        continue;
                    }
                    let spec = (|| -> Result<config::SpecConfig, loga::Error> {
                        let text = std::fs::read_to_string(&path).context("Error reading config file")?;
                        return Ok(serde_json::from_str(&text).context("Error parsing config file")?);
                    })().context_with("Error loading config", ea!(path = path.display()))?;
                    for (ext, syntax) in spec.extensions {
                        extensions.entry(config::normalize_ext(&ext)).or_insert_with(|| config::Mapping {
                            syntax: dir.join(syntax),
                            config: path.clone(),
                        });
                    }
                    for (
                        into,
                        from,
                    ) in [
                        (&mut keys.common, spec.keys.common),
                        (&mut keys.atom, spec.keys.atom),
                        (&mut keys.array, spec.keys.array),
                        (&mut keys.primitive, spec.keys.primitive),
                    ] {
                        for (action, bindings) in from {
                            into.entry(action).or_insert(bindings);
                        }
                    }
                    sources.push(path);
                }
            }
            config::Config {
                extensions: extensions,
                keys: keys,
                sources: sources,
            }
        };
        if let Err(errors) = Keymap::keymap_resolve(&config.keys) {
            return Err(
                loga::agg_err_with(
                    "Errors in key bindings",
                    errors.0.into_iter().map(|e| loga::err(e.to_string())).collect(),
                    ea!(config_files = config.config_files()),
                ),
            );
        }
        if let Some(locate) = &args.locate {
            let (_, syntax, _, document) = load_document(&config, &source_abs)?;
            let reference = &locate.reference;
            let located = if let Some(rest) = reference.strip_prefix('#') {
                let (id, path) = match rest.find('/') {
                    Some(i) => (&rest[..i], &rest[i + 1..]),
                    None => (rest, ""),
                };
                let from = if id.is_empty() {
                    document.root
                } else {
                    let id = id.parse::<i64>().context_with("The id after # must be an integer", ea!(id = id))?;
                    document
                        .atoms
                        .iter()
                        .position(|a| a.unique_id == Some(id))
                        .context_with("No element has this id", ea!(id = id))?
                };
                let path = if path.is_empty() {
                    vec![]
                } else {
                    path.split('/').map(str::to_string).collect::<Vec<_>>()
                };
                document
                    .document_locate(from, &path)
                    .context_with("The path doesn't resolve", ea!(reference = reference))?
            } else if reference.starts_with('/') || reference.is_empty() {
                let (atom, _) =
                    document
                        .atoms
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| *reference == a.path || reference.starts_with(&format!("{}/", a.path)))
                        .max_by_key(|(_, a)| a.path.len())
                        .context_with("No element is at this pointer", ea!(reference = reference))?;
                Located::Atom(atom)
            } else {
                return Err(
                    loga::err_with(
                        "A reference must start with # (an element id) or / (a JSON pointer)",
                        ea!(reference = reference),
                    ),
                );
            };
            let (atom, field) = match located {
                Located::Atom(a) => (a, None),
                Located::Field(a, f) => (a, Some(f)),
            };
            let a = document.document_atom(atom);
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "pointer": a.path,
                "id": a.unique_id,
                "field": field,
                "value": serialize_atom(&syntax, &document, atom)
            })).unwrap());
            return Ok(());
        }
        let keys_text = serde_json::to_string(&config.keys).unwrap();
        let demo = args.browser.is_some();
        let html = if demo {
            let Some(source) = &file else {
                return Err(
                    loga::err_with(
                        "The demo shows one file, so it needs a file to show",
                        ea!(source = source_abs.display()),
                    ),
                );
            };
            let (syntax_text, _, source_text, document) = load_document(&config, source)?;
            eprintln!("Matched {} atoms", document.atoms.len());
            include_str!("../static/demo.html")
                .replace("__MERMAN_SYNTAX__", &embed_json(&syntax_text))
                .replace("__MERMAN_SOURCE__", &embed_json(&source_text))
                .replace("__MERMAN_KEYS__", &embed_json(&keys_text))
        } else {
            include_str!("../static/editor.html").to_string()
        };
        let events = Arc::new(events::Events {
            buffer: std::sync::Mutex::new((ai::now_ms(), std::collections::VecDeque::new())),
            tx: broadcast::channel(1024).0,
        });
        let mut watcher = notify::recommended_watcher({
            let events = events.clone();
            move |res: Result<notify::Event, notify::Error>| {
                let event = match res {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("Error watching files: {}", e);
                        return;
                    },
                };
                if let EventKind::Access(_) = event.kind {
                    return;
                }
                for path in event.paths {
                    events.events_publish(merman3_api::Event::FileChanged { path: display(&path) });
                }
            }
        }).context("Error creating file watcher")?;
        watcher
            .watch(&dir, RecursiveMode::Recursive)
            .context_with("Error watching directory", ea!(dir = dir.display()))?;
        let logs =
            directories::BaseDirs::new()
                .context("Error finding the home directory")?
                .data_local_dir()
                .join("merman3")
                .join("ai")
                .join(dir.to_string_lossy().chars().map(|c| {
                    if c.is_ascii_alphanumeric() || "._-".contains(c) {
                        return c.to_string();
                    }
                    return format!("%{:02X}", c as u32);
                }).collect::<String>());
        let ai = Arc::new(ai::Ai {
            dir: dir.clone(),
            logs: logs,
            events: events.clone(),
            state: tokio::sync::Mutex::new(ai::AiState {
                status: AiStatus::Off,
                session: None,
            }),
        });
        let handler = Arc::new(HandlerRoot {
            html: html.into_bytes(),
            config: config,
            dir: dir,
            file: file,
            keys: keys_text,
            events: events,
            ai: ai,
        });
        let log = loga::Log::new_root(loga::INFO);
        let rt =
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("Error starting async runtime")?;
        let listener =
            rt
                .block_on(tokio::net::TcpListener::bind(("127.0.0.1", args.port.unwrap_or(0))))
                .context("Error binding port")?;
        let url = format!("http://{}/", listener.local_addr().context("Error getting listen address")?);
        eprintln!("Serving at {} (ctrl-c to stop)", url);
        let serve = async move {
            loop {
                let (stream, peer_addr) = match listener.accept().await {
                    Ok(x) => x,
                    Err(e) => {
                        log.log(loga::DEBUG, format!("Error accepting connection: {}", e));
                        continue;
                    },
                };
                let handler = handler.clone();
                let log = log.clone();
                tokio::spawn(async move {
                    let service = service_fn(|mut req: Request<Incoming>| {
                        let handler = handler.clone();
                        return async move {
                            if req.uri().path() == WS_PATH && hyper_tungstenite::is_upgrade_request(&req) {
                                let (response, websocket) = match hyper_tungstenite::upgrade(&mut req, None) {
                                    Ok(x) => x,
                                    Err(e) => return Ok(response_400(format!("Bad websocket request: {}", e))),
                                };
                                let events = handler.events.clone();
                                tokio::spawn(async move {
                                    let Ok(mut ws) = websocket.await else {
                                        return;
                                    };
                                    let Some(Ok(Message::Text(first))) = ws.next().await else {
                                        return;
                                    };
                                    let Ok(WsClient { since }) = serde_json::from_str::<WsClient>(&first) else {
                                        return;
                                    };
                                    let mut rx = events.tx.subscribe();
                                    let mut last = since.unwrap_or(0);
                                    let mut send = vec![];
                                    {
                                        let buffer = events.buffer.lock().unwrap();
                                        match since {
                                            Some(since) => {
                                                let first =
                                                    buffer.1.front().map(|(seq, _)| *seq).unwrap_or(buffer.0 + 1);
                                                if first > since + 1 {
                                                    send.push(WsServer::Gap);
                                                }
                                                for (
                                                    seq,
                                                    event,
                                                ) in buffer.1.iter().filter(|(seq, _)| *seq > since) {
                                                    send.push(WsServer::Event {
                                                        seq: *seq,
                                                        event: event.clone(),
                                                    });
                                                    last = *seq;
                                                }
                                            },
                                            None => {
                                                last = buffer.0;
                                            },
                                        }
                                    }
                                    loop {
                                        for message in send.drain(..) {
                                            if ws
                                                .send(Message::text(serde_json::to_string(&message).unwrap()))
                                                .await
                                                .is_err() {
                                                return;
                                            }
                                        }
                                        tokio::select!{
                                            received = rx.recv() => match received {
                                                Ok((seq, event)) => {
                                                    if seq <= last {
                                                        continue;
                                                    }
                                                    last = seq;
                                                    send.push(WsServer::Event {
                                                        seq: seq,
                                                        event: event,
                                                    });
                                                },
                                                Err(broadcast::error::RecvError::Lagged(_)) => {
                                                    let buffer = events.buffer.lock().unwrap();
                                                    let from = last;
                                                    let first =
                                                        buffer
                                                            .1
                                                            .front()
                                                            .map(|(seq, _)| *seq)
                                                            .unwrap_or(buffer.0 + 1);
                                                    if first > from + 1 {
                                                        send.push(WsServer::Gap);
                                                    }
                                                    for (
                                                        seq,
                                                        event,
                                                    ) in buffer.1.iter().filter(|(seq, _)| *seq > from) {
                                                        send.push(WsServer::Event {
                                                            seq: *seq,
                                                            event: event.clone(),
                                                        });
                                                        last = *seq;
                                                    }
                                                },
                                                Err(broadcast::error::RecvError::Closed) => return,
                                            },
                                            incoming = ws.next() => match incoming {
                                                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                                                Some(Ok(_)) => {
                                                },
                                            },
                                        }
                                    }
                                });
                                return Ok(response.map(|body| body.map_err(|e| match e { }).boxed()));
                            }
                            let (head, body) = req.into_parts();
                            let (path, query) = match head.uri.path_and_query() {
                                Some(pq) => (pq.path().trim_end_matches('/'), pq.query().unwrap_or_default()),
                                None => ("", ""),
                            };
                            return Ok(handler.handle(HandlerArgs {
                                peer_addr: peer_addr,
                                subpath: path,
                                query: query,
                                url: head.uri.clone(),
                                head: &head,
                                body: body,
                            }).await) as Result<_, std::convert::Infallible>;
                        };
                    });
                    if let Err(e) =
                        hyper::server::conn::http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), service)
                            .with_upgrades()
                            .await {
                        log.log(loga::DEBUG, format!("Error serving connection: {}", e));
                    }
                });
            }
        };
        if args.no_open.is_some() || demo {
            if args.no_open.is_none() {
                if let Err(e) = open::that_detached(&url) {
                    eprintln!("Error opening browser: {}", e);
                }
            }
            rt.block_on(serve);
            return Ok(());
        }
        rt.spawn(serve);
        let title =
            format!(
                "{} - merman3",
                source_abs
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| source_abs.display().to_string())
            );
        {
            let event_loop = {
                let hook = std::panic::take_hook();
                std::panic::set_hook(Box::new(|_| { }));
                let res = std::panic::catch_unwind(|| EventLoop::new());
                std::panic::set_hook(hook);
                res.map_err(
                    |_| loga::err(
                        "Error opening a window (is a display available?) - try viewing in a browser with --browser",
                    ),
                )
            }?;
            let window =
                WindowBuilder::new().with_title(&title).build(&event_loop).context("Error creating window")?;
            let builder = WebViewBuilder::new().with_url(&url);
            #[cfg(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android"))]
            let _webview = builder.build(&window).context("Error creating webview")?;
            #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android")))]
            let _webview = {
                use tao::platform::unix::WindowExtUnix;
                use wry::WebViewBuilderExtUnix;

                let vbox = window.default_vbox().context("Window has no gtk container to put the webview in")?;
                builder.build_gtk(vbox).context("Error creating webview")?
            };
            event_loop.run(move |event, _, control_flow| {
                *control_flow = ControlFlow::Wait;
                if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
                    *control_flow = ControlFlow::Exit;
                }
            })
        }
    })() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        },
    }
}
