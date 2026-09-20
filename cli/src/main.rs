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
    response_404,
    Body,
};
use http::header::CONTENT_TYPE;
use http::Response;
use loga::{
    ea,
    ResultContext,
};
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
    source: PathBuf,
    browser: Option<()>,
    /// Port to listen on; a free port is chosen if unspecified
    port: Option<u16>,
    /// Don't open the page in a browser
    no_open: Option<()>,
}

struct HandlerStatic {
    html: Vec<u8>,
}

fn response_bytes(data: &'static [u8], content_type: &str) -> Response<Body> {
    return Response::builder()
        .status(200)
        .header(CONTENT_TYPE, content_type)
        .body(body_full(data.to_vec()))
        .unwrap();
}

#[htwrap::htserve::handler::async_trait::async_trait]
impl Handler<Body> for HandlerStatic {
    async fn handle(&self, args: HandlerArgs<'_>) -> Response<Body> {
        return match args.subpath {
            "" => response_200_html(self.html.clone()),
            "/merman3_web.js" => response_bytes(include_bytes!("../static/merman3_web.js"), "text/javascript"),
            "/merman3_web_bg.wasm" => response_bytes(
                include_bytes!("../static/merman3_web_bg.wasm"),
                "application/wasm",
            ),
            _ => response_404(),
        };
    }
}

/// Make JSON safe to embed in a `<script>` element.
fn embed_json(json: &str) -> String {
    return json.replace("</", "<\\/").replace("<!--", "<\\u0021--");
}

fn read(path: &Path) -> Result<String, loga::Error> {
    return Ok(std::fs::read_to_string(path).context_with("Error reading file", ea!(path = path.display()))?);
}

fn main() {
    match (|| -> Result<(), loga::Error> {
        let args = vark::<Args>();
        let html = (|| -> Result<String, loga::Error> {
            let source = &args.source;
            let config = {
                let mut extensions = std::collections::HashMap::new();
                let mut sources = vec![];
                let dirs = {
                    let mut out = vec![];
                    let mut add = |dir: PathBuf| {
                        if !out.contains(&dir) {
                            out.push(dir);
                        }
                    };
                    let source_abs = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
                    for dir in source_abs.parent().into_iter().flat_map(|p| p.ancestors()) {
                        add(dir.to_path_buf());
                    }
                    if let Ok(cwd) = std::env::current_dir() {
                        for dir in cwd.ancestors() {
                            add(dir.to_path_buf());
                        }
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
                        sources.push(path);
                    }
                }
                config::Config {
                    extensions: extensions,
                    sources: sources,
                }
            };
            let mapping = 'syntax_for: {
                let Some(ext) = source.extension().and_then(|e| e.to_str()) else {
                    break 'syntax_for Err(
                        loga::err_with(
                            "Source file has no extension, so no syntax can be looked up for it",
                            ea!(source = source.display()),
                        ),
                    );
                };
                let Some(mapping) = config.extensions.get(&config::normalize_ext(ext)) else {
                    let mut known = config.extensions.keys().cloned().collect::<Vec<_>>();
                    known.sort();
                    break 'syntax_for Err(
                        loga::err_with(
                            "No syntax is configured for this file extension",
                            ea!(
                                source = source.display(),
                                extension = ext,
                                configured_extensions = known.join(", "),
                                config_files =
                                    config
                                        .sources
                                        .iter()
                                        .map(|p| p.display().to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                            ),
                        ),
                    );
                };
                Ok(mapping)
            }?;
            let syntax_path = &mapping.syntax;
            let syntax_text =
                read(
                    syntax_path,
                ).context_with(
                    "Error reading the syntax configured for this file",
                    ea!(config = mapping.config.display()),
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
                            errors.into_iter().map(loga::err).collect(),
                            ea!(syntax = syntax_path.display()),
                        ),
                    );
                },
            };
            let source_text = read(&args.source)?;
            let value =
                serde_json::from_str::<serde_json::Value>(
                    &source_text,
                ).context_with("Error parsing source", ea!(source = args.source.display()))?;
            let document = match match_document(&syntax, &value) {
                Ok(d) => d,
                Err(e) => {
                    return Err(
                        loga::err_with(
                            format!("Source doesn't match syntax:\n{}", e.mismatch_format()),
                            ea!(source = args.source.display(), syntax = syntax_path.display()),
                        ),
                    );
                },
            };
            eprintln!("Matched {} atoms", document.atoms.len());
            return Ok(
                include_str!("../static/index.html")
                    .replace("__MERMAN_SYNTAX__", &embed_json(&syntax_text))
                    .replace("__MERMAN_SOURCE__", &embed_json(&source_text)),
            );
        })()?;
        let handler: Arc<dyn Handler<Body>> = Arc::new(HandlerStatic { html: html.into_bytes() });
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
        if args.no_open.is_some() || args.browser.is_some() {
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
                args
                    .source
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| args.source.display().to_string())
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
