//! Checks a syntax and source file, then serves a page that shows the source
//! laid out with that syntax and opens it in the browser.
use aargvark::{vark, Aargvark};
use htwrap::htserve::handler::{root_handle_http, Handler, HandlerArgs};
use htwrap::htserve::responses::{body_full, response_200_html, response_404, Body};
use http::header::CONTENT_TYPE;
use http::Response;
use merman3_core::matcher::match_document;
use merman3_core::spec::SpecSyntax;
use merman3_core::syntax::Syntax;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;

const INDEX_HTML: &str = include_str!("../static/index.html");
const WEB_JS: &[u8] = include_bytes!("../static/merman3_web.js");
const WEB_WASM: &[u8] = include_bytes!("../static/merman3_web_bg.wasm");

/// View a JSON source file using a merman3 syntax definition.
#[derive(Aargvark)]
struct Args {
    /// Path to the syntax definition JSON
    syntax: PathBuf,
    /// Path to the source JSON to view
    source: PathBuf,
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

fn read(path: &PathBuf) -> String {
    match std::fs::read_to_string(path) {
        Ok(s) => return s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path.display(), e);
            exit(1);
        }
    }
}

#[tokio::main]
async fn main() {
    let args = vark::<Args>();
    let syntax_text = read(&args.syntax);
    let spec: SpecSyntax = match serde_json::from_str(&syntax_text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error parsing syntax {}: {}", args.syntax.display(), e);
            exit(1);
        }
    };
    let syntax = match Syntax::syntax_resolve(spec) {
        Ok(s) => s,
        Err(errors) => {
            eprintln!("Errors in syntax {}:", args.syntax.display());
            for e in errors {
                eprintln!("  {}", e);
            }
            exit(1);
        }
    };
    let source_text = read(&args.source);
    let value: serde_json::Value = match serde_json::from_str(&source_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing source {}: {}", args.source.display(), e);
            exit(1);
        }
    };
    let document = match match_document(&syntax, &value) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "Source {} doesn't match syntax {}:\n{}",
                args.source.display(),
                args.syntax.display(),
                e.mismatch_format()
            );
            exit(1);
        }
    };
    eprintln!("Matched {} atoms", document.atoms.len());
    let html = INDEX_HTML
        .replace("__MERMAN_SYNTAX__", &embed_json(&syntax_text))
        .replace("__MERMAN_SOURCE__", &embed_json(&source_text));
    let handler: Arc<dyn Handler<Body>> = Arc::new(HandlerStatic {
        html: html.into_bytes(),
    });
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", args.port.unwrap_or(0))).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error binding port: {}", e);
            exit(1);
        }
    };
    let url = format!("http://{}/", listener.local_addr().unwrap());
    eprintln!("Serving at {} (ctrl-c to stop)", url);
    if args.no_open.is_none() {
        if let Err(e) = open::that_detached(&url) {
            eprintln!("Error opening browser: {}", e);
        }
    }
    let log = loga::Log::new_root(loga::INFO);
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                log.log(loga::DEBUG, format!("Error accepting connection: {}", e));
                continue;
            }
        };
        if let Err(e) = root_handle_http(&log, handler.clone(), stream).await {
            log.log_err(loga::DEBUG, e.context("Error handling connection"));
        }
    }
}
