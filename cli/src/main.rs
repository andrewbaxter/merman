use aargvark::{
    vark,
    Aargvark,
};
use htwrap::htserve::handler::{
    root_handle_http,
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
use http::header::CONTENT_TYPE;
use http::Response;
use http_body_util::BodyExt;
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
    ListEntry,
    RespList,
    RespOpen,
    RespStart,
    API_PATH,
};
use merman3_core::keys::Keymap;
use merman3_core::matcher::match_document;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use std::path::{
    Path,
    PathBuf,
};
use std::process::exit;
use std::sync::Arc;
use tao::event::{
    Event,
    WindowEvent,
};
use tao::event_loop::{
    ControlFlow,
    EventLoop,
};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

mod config;

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
}

struct HandlerRoot {
    html: Vec<u8>,
    config: config::Config,
    dir: PathBuf,
    file: Option<PathBuf>,
    keys: String,
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
                    let dir =
                        std::fs::canonicalize(
                            &req.dir,
                        ).context_with("Error resolving directory", ea!(dir = req.dir))?;
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
                        parent: dir.parent().map(display),
                        dir: display(&dir),
                        entries: dirs,
                    });
                })() {
                    Ok(v) => respond(v),
                    Err(e) => ServerResp::err(e.to_string()),
                },
                ServerReq::Open(respond, req) => match (|| -> Result<RespOpen, loga::Error> {
                    let path = PathBuf::from(&req.path);
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
        let keys_text = serde_json::to_string(&config.keys).unwrap();
        let demo = args.browser.is_some();
        let html = if demo {
            (|| -> Result<String, loga::Error> {
                let Some(source) = &file else {
                    return Err(
                        loga::err_with(
                            "The demo shows one file, so it needs a file to show",
                            ea!(source = source_abs.display()),
                        ),
                    );
                };
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
                    std::fs::read_to_string(
                        source,
                    ).context_with("Error reading file", ea!(path = source.display()))?;
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
                eprintln!("Matched {} atoms", document.atoms.len());
                return Ok(
                    include_str!("../static/demo.html")
                        .replace("__MERMAN_SYNTAX__", &embed_json(&syntax_text))
                        .replace("__MERMAN_SOURCE__", &embed_json(&source_text))
                        .replace("__MERMAN_KEYS__", &embed_json(&keys_text)),
                );
            })()?
        } else {
            include_str!("../static/editor.html").to_string()
        };
        let handler: Arc<dyn Handler<Body>> = Arc::new(HandlerRoot {
            html: html.into_bytes(),
            config: config,
            dir: dir,
            file: file,
            keys: keys_text,
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
                let (stream, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(e) => {
                        log.log(loga::DEBUG, format!("Error accepting connection: {}", e));
                        continue;
                    },
                };
                let handler = handler.clone();
                let log = log.clone();
                tokio::spawn(async move {
                    if let Err(e) = root_handle_http(&log, handler, stream).await {
                        log.log_err(loga::DEBUG, e.context("Error handling connection"));
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
