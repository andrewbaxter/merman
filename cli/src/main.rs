mod ai;
mod events;

use {
    aargvark::{
        Aargvark,
        vark,
    },
    foyer::DeviceBuilder,
    futures::{
        SinkExt,
        StreamExt,
    },
    http::{
        Response,
        header::CONTENT_TYPE,
    },
    http_body_util::BodyExt,
    htwrap::htserve::{
        handler::{
            Handler,
            HandlerArgs,
        },
        responses::{
            Body,
            body_full,
            response_200_html,
            response_400,
            response_404,
        },
    },
    hyper::{
        Request,
        body::Incoming,
        service::service_fn,
    },
    hyper_tungstenite::tungstenite::Message,
    hyper_util::rt::TokioIo,
    loga::{
        ResultContext,
        ea,
    },
    merman::{
        config,
        load_document,
    },
    merman_api::{
        API_PATH,
        AiMessage,
        AiRole,
        AiSessionInfo,
        AiStatus,
        ListEntry,
        RespAiClear,
        RespAiHistory,
        RespAiResume,
        RespAiSend,
        RespAiSessions,
        RespList,
        RespLocationSet,
        RespOpen,
        RespStart,
        WS_PATH,
        WsClient,
        WsServer,
        api::{
            Req,
            ServerReq,
            ServerResp,
        },
    },
    merman_core::keys::Keymap,
    notify::{
        EventKind,
        RecursiveMode,
        Watcher,
    },
    std::{
        path::{
            Path,
            PathBuf,
        },
        sync::Arc,
    },
    tao::{
        event::{
            Event,
            WindowEvent,
        },
        event_loop::{
            ControlFlow,
            EventLoop,
        },
        window::WindowBuilder,
    },
    tokio::{
        io::AsyncWriteExt,
        sync::broadcast,
    },
    wry::WebViewBuilder,
};

/// View a JSON source file using a merman3 syntax definition.
#[derive(Aargvark)]
struct Args {
    browser: Option<()>,
    /// Don't open the page in a browser
    no_open: Option<()>,
    port: Option<u16>,
    source: Option<PathBuf>,
}

fn display(path: &Path) -> String {
    return path.to_string_lossy().into_owned();
}

/// Make JSON safe to embed in a `<script>` element.
fn embed_json(json: &str) -> String {
    return json.replace("</", "<\\/").replace("<!--", "<\\u0021--");
}

struct HandlerRoot {
    ai: Arc<ai::Ai>,
    config: config::Config,
    dir: PathBuf,
    events: Arc<events::Events>,
    file: Option<PathBuf>,
    html: Vec<u8>,
    keys: String,
    locations: foyer::HybridCache<String, String>,
    theme: String,
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
                    theme: self.theme.clone(),
                }),
                ServerReq::List(respond, req) => match (|| -> Result<RespList, loga::Error> {
                    let dir = self.dir_contains(Path::new(&req.dir))?;
                    self.events.events_stamp(&dir);
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
                        location: None,
                    });
                })() {
                    Ok(mut v) => match self.locations.get(&v.dir).await {
                        Ok(cached) => {
                            v.location = cached.map(|e| e.value().clone());
                            respond(v)
                        },
                        Err(e) => ServerResp::err(format!("Error reading the location cache: {}", e)),
                    },
                    Err(e) => ServerResp::err(serde_json::to_string_pretty(&e).unwrap()),
                },
                ServerReq::AiSend(respond, req) => {
                    let result: Result<(), loga::Error> = async {
                        let ai = &self.ai;
                        let mut state = ai.state.lock().await;
                        if req.text.trim() == "/cancel" {
                            let Some(session) = state.session.as_mut() else {
                                return Err(loga::err("There is no session to cancel"));
                            };
                            ai::ai_control(session, serde_json::json!({
                                "subtype": "interrupt"
                            })).await?;
                            ai::ai_message(&session.log, &ai.events, AiRole::System, "Cancel requested".to_string())?;
                            return Ok(());
                        }
                        if state.session.is_none() {
                            state.session = Some(ai::ai_spawn(ai, uuid::Uuid::new_v4().to_string(), false)?);
                        }
                        let session = state.session.as_mut().unwrap();
                        if let Some(model) =
                            req.text.trim().strip_prefix("/model").filter(|m| m.is_empty() || m.starts_with(' ')) {
                            let model = model.trim();
                            ai::ai_control(session, serde_json::json!({
                                "subtype": "set_model",
                                "model": if model.is_empty() {
                                    None
                                }
                                else {
                                    Some(model)
                                }
                            })).await?;
                            ai::ai_message(&session.log, &ai.events, AiRole::System, if model.is_empty() {
                                "Switching to the default model".to_string()
                            } else {
                                format!("Switching to model {}", model)
                            })?;
                            return Ok(());
                        }
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
                        Err(e) => ServerResp::err(serde_json::to_string_pretty(&e).unwrap()),
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
                        Err(e) => ServerResp::err(serde_json::to_string_pretty(&e).unwrap()),
                    }
                },
                ServerReq::AiSessions(respond, _) => match (|| -> Result<RespAiSessions, loga::Error> {
                    let mut sessions = vec![];
                    let entries = match std::fs::read_dir(&self.ai.logs) {
                        Ok(entries) => entries,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(RespAiSessions { sessions: sessions });
                        },
                        Err(e) => {
                            return Err(
                                loga::err_with(
                                    format!("Error reading the transcript directory: {}", e),
                                    ea!(path = self.ai.logs.display()),
                                ),
                            );
                        },
                    };
                    for entry in entries {
                        let entry = entry.context("Error reading the transcript directory")?;
                        let path = entry.path();
                        let Some(id) =
                            path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .and_then(|n| n.strip_suffix(".jsonl")) else {
                                continue;
                            };
                        let text =
                            std::fs::read_to_string(
                                &path,
                            ).context_with("Error reading a transcript", ea!(path = path.display()))?;
                        let mut first = None;
                        let mut last_time = 0;
                        let mut count = 0;
                        for line in text.lines() {
                            let message =
                                serde_json::from_str::<AiMessage>(
                                    line,
                                ).context_with("Error parsing a transcript", ea!(path = path.display()))?;
                            count += 1;
                            if message.role == AiRole::User {
                                if first.is_none() {
                                    first = Some(message.text.clone());
                                }
                                last_time = message.time;
                            }
                        }
                        let Some(first) = first else {
                            continue;
                        };
                        sessions.push(AiSessionInfo {
                            id: id.to_string(),
                            first: first,
                            last_time: last_time,
                            messages: count,
                        });
                    }
                    sessions.sort_by(|a, b| b.last_time.cmp(&a.last_time));
                    return Ok(RespAiSessions { sessions: sessions });
                })() {
                    Ok(v) => respond(v),
                    Err(e) => ServerResp::err(serde_json::to_string_pretty(&e).unwrap()),
                },
                ServerReq::AiResume(respond, req) => {
                    let result: Result<(), loga::Error> = async {
                        let id =
                            uuid::Uuid::parse_str(&req.id)
                                .context_with("Session ids are uuids", ea!(id = req.id))?
                                .to_string();
                        if !self.ai.logs.join(format!("{}.jsonl", id)).is_file() {
                            return Err(loga::err_with("There is no transcript for this session", ea!(id = id)));
                        }
                        let mut state = self.ai.state.lock().await;
                        if let Some(mut session) = state.session.take() {
                            session.reader.abort();
                            _ = session.child.kill().await;
                        }
                        state.session = Some(ai::ai_spawn(&self.ai, id, true)?);
                        ai::ai_status_set(&mut state, &self.ai.events, AiStatus::Waiting);
                        return Ok(());
                    }.await;
                    match result {
                        Ok(()) => respond(RespAiResume {}),
                        Err(e) => ServerResp::err(serde_json::to_string_pretty(&e).unwrap()),
                    }
                },
                ServerReq::LocationSet(respond, req) => match self.dir_contains(Path::new(&req.path)) {
                    Ok(path) => {
                        self.locations.insert(display(&path), req.location);
                        respond(RespLocationSet {})
                    },
                    Err(e) => ServerResp::err(serde_json::to_string_pretty(&e).unwrap()),
                },
                ServerReq::Open(respond, req) => match (|| -> Result<(RespOpen, String), loga::Error> {
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
                    self.events.events_stamp(&path);
                    let source =
                        std::fs::read_to_string(
                            &path,
                        ).context_with("Error reading file", ea!(path = path.display()))?;
                    return Ok((RespOpen {
                        syntax: syntax,
                        source: source,
                        location: None,
                    }, display(&path)));
                })() {
                    Ok((mut v, path)) => match self.locations.get(&path).await {
                        Ok(cached) => {
                            v.location = cached.map(|e| e.value().clone());
                            respond(v)
                        },
                        Err(e) => ServerResp::err(format!("Error reading the location cache: {}", e)),
                    },
                    Err(e) => ServerResp::err(serde_json::to_string_pretty(&e).unwrap()),
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
            "/merman.css" => response_bytes(include_bytes!("../static/merman.css"), "text/css"),
            "/merman_web.js" => response_bytes(include_bytes!("../static/merman_web.js"), "text/javascript"),
            "/merman_web_bg.wasm" => response_bytes(
                include_bytes!("../static/merman_web_bg.wasm"),
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

fn main() {
    match (|| -> Result<(), loga::Error> {
        let args = vark::<Args>();
        let cwd = std::env::current_dir().context("Error getting the working directory")?;
        let source = args.source.unwrap_or_else(|| cwd.clone());
        let source_abs = std::fs::canonicalize(&source).unwrap_or_else(|_| source.clone());
        let file = if source_abs.is_dir() {
            None
        } else {
            Some(source_abs.clone())
        };
        let dir = match &file {
            Some(file) => {
                let cwd = std::fs::canonicalize(&cwd).context("Error resolving the working directory")?;
                if !file.starts_with(&cwd) {
                    return Err(
                        loga::err_with(
                            "The source file must be inside the working directory",
                            ea!(source = file.display(), dir = cwd.display()),
                        ),
                    );
                }
                cwd
            },
            None => source_abs.clone(),
        };
        let config = config::config_load(&dir, &cwd)?;
        if let Err(errors) = Keymap::keymap_resolve(&config.keys) {
            return Err(
                loga::agg_err_with(
                    "Errors in key bindings",
                    errors.0.into_iter().map(|e| loga::err(e.to_string())).collect(),
                    ea!(config_files = config.config_files()),
                ),
            );
        }
        let keys_text = serde_json::to_string(&config.keys).unwrap();
        let theme_text = serde_json::to_string(&config.theme).unwrap();
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
                .replace("__MERMAN_THEME__", &embed_json(&theme_text))
        } else {
            include_str!("../static/editor.html").to_string()
        };
        let events = Arc::new(events::Events {
            buffer: std::sync::Mutex::new((ai::now_ms(), std::collections::VecDeque::new())),
            tx: broadcast::channel(1024).0,
            stamps: std::sync::Mutex::new(std::collections::HashMap::new()),
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
                if event.need_rescan() {
                    events.events_poll();
                    return;
                }
                if let EventKind::Access(_) = event.kind {
                    return;
                }
                for path in event.paths {
                    events.events_publish(merman_api::Event::FileChanged { path: display(&path) });
                }
            }
        }).context("Error creating file watcher")?;
        watcher
            .watch(&dir, RecursiveMode::Recursive)
            .context_with("Error watching directory", ea!(dir = dir.display()))?;
        let rt =
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("Error starting async runtime")?;
        rt.spawn({
            let events = events.clone();
            async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    interval.tick().await;
                    events.events_poll();
                }
            }
        });
        let base_dirs = directories::BaseDirs::new().context("Error finding the home directory")?;
        let locations_dir = base_dirs.cache_dir().join("merman").join("locations");
        std::fs::create_dir_all(
            &locations_dir,
        ).context_with("Error creating the location cache directory", ea!(path = locations_dir.display()))?;
        let locations = rt.block_on(async {
            let device = foyer::FsDeviceBuilder::new(&locations_dir).with_capacity(64 << 20).build()?;
            return foyer::HybridCacheBuilder::new()
                .with_policy(foyer::HybridCachePolicy::WriteOnInsertion)
                .memory(4 << 20)
                .storage()
                .with_engine_config(foyer::BlockEngineConfig::new(device))
                .build()
                .await;
        }).context_with("Error opening the location cache", ea!(path = locations_dir.display()))?;
        let logs = base_dirs.data_local_dir().join("merman").join("ai").join(dir.to_string_lossy().chars().map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                return c.to_string();
            }
            return format!("%{:02X}", c as u32);
        }).collect::<String>());
        let ai = Arc::new(ai::Ai {
            configs: config
                .sources
                .iter()
                .cloned()
                .chain(config.extensions.values().map(|m| m.syntax.clone()))
                .collect(),
            dir: dir.clone(),
            logs: logs,
            events: events.clone(),
            state: tokio::sync::Mutex::new(ai::AiState {
                status: AiStatus::Off,
                session: None,
                notification: None,
            }),
        });
        let handler = Arc::new(HandlerRoot {
            html: html.into_bytes(),
            config: config,
            dir: dir,
            file: file,
            keys: keys_text,
            theme: theme_text,
            events: events,
            ai: ai,
            locations: locations,
        });
        let log = loga::Log::new_root(loga::INFO);
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
                "{} - merman",
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
                use {
                    tao::platform::unix::WindowExtUnix,
                    wry::WebViewBuilderExtUnix,
                };

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
        Err(e) => loga::fatal(e),
    }
}

fn response_bytes(data: &'static [u8], content_type: &str) -> Response<Body> {
    return Response::builder()
        .status(200)
        .header(CONTENT_TYPE, content_type)
        .body(body_full(data.to_vec()))
        .unwrap();
}
