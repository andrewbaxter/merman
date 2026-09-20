//! Looks up the syntax for a source file by its extension (see `config.rs`),
//! checks the two against each other, then serves a page that shows the source
//! laid out with that syntax and displays it in a window (or a browser).
use aargvark::{vark, Aargvark};
use htwrap::htserve::handler::{root_handle_http, Handler, HandlerArgs};
use htwrap::htserve::responses::{body_full, response_200_html, response_404, Body};
use http::header::CONTENT_TYPE;
use http::Response;
use loga::{ea, ResultContext};
use merman3_core::matcher::match_document;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::Arc;

mod config;
mod window;

const INDEX_HTML: &str = include_str!("../static/index.html");
const WEB_JS: &[u8] = include_bytes!("../static/merman3_web.js");
const WEB_WASM: &[u8] = include_bytes!("../static/merman3_web_bg.wasm");

/// View a document using the merman3 syntax configured for its file extension.
#[derive(Aargvark)]
struct Args {
    /// Path to the source file to view
    source: PathBuf,
    /// Show the page in a web browser instead of a window
    browser: Option<()>,
    /// Port to listen on; a free port is chosen if unspecified
    port: Option<u16>,
    /// Only serve the page, don't display it
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
        match args.subpath {
            "" => return response_200_html(self.html.clone()),
            "/merman3_web.js" => return response_bytes(WEB_JS, "text/javascript"),
            "/merman3_web_bg.wasm" => return response_bytes(WEB_WASM, "application/wasm"),
            _ => return response_404(),
        }
    }
}

/// Make JSON safe to embed in a `<script>` element.
fn embed_json(json: &str) -> String {
    return json.replace("</", "<\\/").replace("<!--", "<\\u0021--");
}

fn read(path: &Path) -> Result<String, loga::Error> {
    return Ok(std::fs::read_to_string(path).context_with("Error reading file", ea!(path = path.display()))?);
}

/// Check the source against its syntax and build the page that displays it.
fn build_page(args: &Args) -> Result<String, loga::Error> {
    let config = config::load(&args.source)?;
    let mapping = config.syntax_for(&args.source)?;
    let syntax_path = &mapping.syntax;
    let syntax_text = read(syntax_path).context_with(
        "Error reading the syntax configured for this file",
        ea!(config = mapping.config.display()),
    )?;
    let spec =
        serde_json::from_str::<SpecSyntax>(&syntax_text)
            .context_with("Error parsing syntax", ea!(syntax = syntax_path.display()))?;
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
            return Err(loga::err_with(format!("Source doesn't match syntax:\n{}", e.mismatch_format()), ea!(
                source = args.source.display(),
                syntax = syntax_path.display()
            )));
        },
    };
    eprintln!("Matched {} atoms", document.atoms.len());
    return Ok(
        INDEX_HTML
            .replace("__MERMAN_SYNTAX__", &embed_json(&syntax_text))
            .replace("__MERMAN_SOURCE__", &embed_json(&source_text)),
    );
}

/// Accept connections until the process ends.
async fn serve(log: loga::Log, listener: tokio::net::TcpListener, handler: Arc<dyn Handler<Body>>) {
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
}

fn run() -> Result<(), loga::Error> {
    let args = vark::<Args>();
    let html = build_page(&args)?;
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
    let serve = serve(log, listener, handler);
    if args.no_open.is_some() || args.browser.is_some() {
        if args.no_open.is_none() {
            if let Err(e) = open::that_detached(&url) {
                eprintln!("Error opening browser: {}", e);
            }
        }
        rt.block_on(serve);
        return Ok(());
    }

    // The window's event loop needs the main thread, so the server moves to the
    // runtime's threads.
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
    return window::show(&title, &url);
}

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        },
    }
}
