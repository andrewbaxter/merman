use {
    crate::events::Events,
    loga::{
        ResultContext,
        ea,
    },
    merman_langserver::{
        ClientMessage,
        Request,
        Response,
    },
    std::{
        collections::{
            BTreeMap,
            HashMap,
        },
        io::Write,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{
                AtomicBool,
                Ordering,
            },
        },
        time::Duration,
    },
    tokio::{
        io::AsyncWriteExt,
        process::{
            Child,
            ChildStdin,
        },
        sync::{
            Mutex,
            oneshot,
        },
    },
};

pub struct LangServers {
    pub dir: PathBuf,
    pub events: Arc<Events>,
    pub logs: PathBuf,
    pub paused: AtomicBool,
    pub servers: BTreeMap<String, LangServer>,
}

pub struct LangServer {
    pub command: Vec<String>,
    pub state: Mutex<LangServerState>,
}

#[derive(Default)]
pub struct LangServerState {
    pub child: Option<Child>,
    pub stdin: Option<ChildStdin>,
    pub next_id: u64,
    pub pending: HashMap<u64, oneshot::Sender<Response>>,
}

pub fn langserver_log(ls: &LangServers, name: &str, line: &str) {
    _ = (|| -> Result<(), loga::Error> {
        std::fs::create_dir_all(&ls.logs).context("Error creating the language server log directory")?;
        let path = ls.logs.join(format!("{}.log", name));
        let mut file =
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)
                .context_with("Error opening the language server log", ea!(path = path.display()))?;
        writeln!(file, "{}", line).context_with("Error writing the language server log", ea!(path = path.display()))?;
        return Ok(());
    })();
}

pub async fn langserver_request(ls: &LangServers, name: &str, req: Request) -> Result<Response, loga::Error> {
    let server = ls.servers.get(name).context_with("No such language server", ea!(server = name))?;
    let rx = {
        let mut state = server.state.lock().await;
        let id = state.next_id;
        state.next_id += 1;
        let line = serde_json::to_string(&ClientMessage {
            id: id,
            req: req,
        }).unwrap();
        let Some(stdin) = state.stdin.as_mut() else {
            return Err(loga::err_with("The language server isn't running", ea!(server = name)));
        };
        stdin
            .write_all(format!("{}\n", line).as_bytes())
            .await
            .context_with("Error writing to the language server", ea!(server = name))?;
        let (tx, rx) = oneshot::channel();
        state.pending.insert(id, tx);
        rx
    };
    match tokio::time::timeout(Duration::from_secs(600), rx).await {
        Ok(Ok(res)) => return Ok(res),
        Ok(Err(_)) => return Err(
            loga::err_with("The language server exited before answering", ea!(server = name)),
        ),
        Err(_) => return Err(loga::err_with("The language server didn't answer in time", ea!(server = name))),
    }
}

pub async fn langservers_pause(ls: &LangServers, paused: bool) {
    ls.paused.store(paused, Ordering::SeqCst);
    for name in ls.servers.keys() {
        _ = langserver_request(ls, name, Request::Pause { paused: paused }).await;
    }
    return;
}

pub async fn langservers_kill(ls: &LangServers) {
    for server in ls.servers.values() {
        let mut state = server.state.lock().await;
        if let Some(mut child) = state.child.take() {
            _ = child.kill().await;
        }
    }
    return;
}
