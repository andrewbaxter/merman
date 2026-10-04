use {
    crate::events::Events,
    loga::{
        ResultContext,
        ea,
    },
    notify_rust::{
        Notification,
        NotificationHandle,
        Timeout,
        Urgency,
    },
    merman_api::{
        AiMessage,
        AiRole,
        AiStatus,
        Event,
    },
    std::{
        io::Write,
        path::{
            Path,
            PathBuf,
        },
        process::Stdio,
        sync::Arc,
        time::{
            Duration,
            SystemTime,
            UNIX_EPOCH,
        },
    },
    tokio::{
        io::{
            AsyncBufReadExt,
            AsyncReadExt,
            AsyncWriteExt,
            BufReader,
        },
        process::{
            Child,
            ChildStdin,
            Command,
        },
        sync::Mutex,
    },
};

pub struct Ai {
    pub configs: Vec<PathBuf>,
    pub dir: PathBuf,
    pub events: Arc<Events>,
    pub logs: PathBuf,
    pub state: Mutex<AiState>,
}

pub fn ai_message(log: &Path, events: &Events, role: AiRole, text: String) -> Result<(), loga::Error> {
    let message = AiMessage {
        role: role,
        text: text,
        time: now_ms(),
    };
    let mut file =
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(log)
            .context_with("Error opening the transcript", ea!(path = log.display()))?;
    writeln!(
        file,
        "{}",
        serde_json::to_string(&message).unwrap()
    ).context_with("Error writing the transcript", ea!(path = log.display()))?;
    events.events_publish(Event::Ai { message: message });
    return Ok(());
}

pub fn ai_notify(ai: &Arc<Ai>, status: AiStatus, summary: &str, body: String) {
    let mut notification = Notification::new();
    notification
        .appname("merman")
        .summary(summary)
        .body(&body.chars().take(300).collect::<String>())
        .timeout(Timeout::Never)
        .urgency(Urgency::Critical);
    tokio::spawn({
        let ai = ai.clone();
        async move {
            let Ok(handle) = notification.show_async().await else {
                return;
            };
            let mut state = ai.state.lock().await;
            if state.status != status {
                handle.close_async().await;
                return;
            }
            if let Some(old) = state.notification.replace(handle) {
                old.close_async().await;
            }
        }
    });
    return;
}

pub async fn ai_control(session: &mut AiSession, request: serde_json::Value) -> Result<(), loga::Error> {
    let line = serde_json::json!({
        "type": "control_request",
        "request_id": format !("{}-{}", request["subtype"].as_str().unwrap_or_default(), now_ms()),
        "request": request
    });
    session
        .stdin
        .write_all(format!("{}\n", line).as_bytes())
        .await
        .context("Error sending a control request to claude")?;
    return Ok(());
}

pub fn ai_spawn(ai: &Arc<Ai>, id: String, resume: bool) -> Result<AiSession, loga::Error> {
    std::fs::create_dir_all(
        &ai.logs,
    ).context_with("Error creating the transcript directory", ea!(path = ai.logs.display()))?;
    let log = ai.logs.join(format!("{}.jsonl", id));
    let home = directories::BaseDirs::new().context("Error finding the home directory")?.home_dir().to_path_buf();
    let mut cmd = Command::new("bwrap");
    cmd.args(
        [
            "--unshare-all",
            "--share-net",
            "--die-with-parent",
            "--bind",
            "/proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
        ],
    );
    let mut bound: Vec<PathBuf> = vec![];
    let mut bind = |cmd: &mut Command, path: &Path, writable: bool| {
        if bound.iter().any(|b| path.starts_with(b)) || !path.exists() {
            return;
        }
        cmd.arg(if writable {
            "--bind"
        } else {
            "--ro-bind"
        }).arg(path).arg(path);
        bound.push(path.to_path_buf());
    };
    for root in ["/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc", "/opt", "/run/current-system"] {
        bind(&mut cmd, Path::new(root), false);
    }
    if let Ok(resolv) = std::fs::canonicalize("/etc/resolv.conf") {
        bind(&mut cmd, &resolv, false);
    }
    for extra in std::env::split_paths(&std::env::var_os("MERMAN_SANDBOX_RO").unwrap_or_default()) {
        bind(&mut cmd, &extra, false);
    }
    let exe_dir =
        std::env::current_exe()
            .context("Error finding the editor's own binary")?
            .parent()
            .context("The editor's own binary has no directory")?
            .to_path_buf();
    let mut path_dirs = vec![exe_dir];
    path_dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    for dir in path_dirs.iter().filter(|d| d.is_absolute()) {
        bind(&mut cmd, dir, false);
    }
    cmd.arg("--dir").arg(&home);
    bind(&mut cmd, &home.join(".claude"), true);
    bind(&mut cmd, &home.join(".claude.json"), true);
    bind(&mut cmd, &ai.dir, true);
    cmd.arg("--chdir").arg(&ai.dir);
    for config in &ai.configs {
        bind(&mut cmd, config, false);
    }
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
            if resume {
                "--resume"
            } else {
                "--session-id"
            },
            &id,
            "--append-system-prompt",
            "The user is editing this project's syntax tree files in the merman editor and refers to \
            places in them as FILE#REF. REF is the id of the nearest enclosing element that has one \
            (every element's id is at `id.value` in the file), then a jq path below that element, e.g. \
            `a.at#16.variant.bind.name`; without an id the jq path starts from the file's root, e.g. \
            `a.at#.v1.exprs[0]`. A trailing jq slice selects a range: `#16.variant.seq.exprs[1:3]` is elements \
            1 and 2, `#16.variant.bind.name[0:3]` the first three characters. The files are single-line JSON, \
            so use `merman-tool` (REF is from the `#` on) rather than text tools: `merman-tool get FILE REF` \
            prints the canonical reference, the element id and the JSON there (`--up N` goes N jq levels up from REF \
            first, a slice counting as one, to see what encloses it; `--depth N` elides what's nested deeper than \
            N); `merman-tool find FILE PATTERN \
            [--within REF] [--regex] [--depth N]` prints the same for every value matching a JSON pattern (an \
            object matches objects having at least its keys, e.g. `'{\"bind\":{\"name\":\"x\"}}'`); \
            `merman-tool set FILE REF` replaces the JSON at REF with the JSON on stdin (a slice takes an array \
            or string to splice in, so `[i:i]` inserts); `merman-tool delete FILE REF` removes it. Edits are \
            checked against the syntax before the file is written. Give new elements an id of -1 and it is \
            assigned; ids copied from elsewhere in the file are reassigned too.",
        ],
    );
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
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
                            ai_message(
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
                                    _ = ai_message(&log, &ai.events, AiRole::Assistant, text.to_string());
                                },
                                Some("tool_use") => {
                                    _ =
                                        ai_message(
                                            &log,
                                            &ai.events,
                                            AiRole::Tool,
                                            format!(
                                                "{} {}",
                                                block["name"].as_str().unwrap_or_default(),
                                                serde_json::to_string_pretty(&block["input"]).unwrap()
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
                                ai_message(
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
                                ai_message(
                                    &log,
                                    &ai.events,
                                    AiRole::System,
                                    value["result"].as_str().unwrap_or("Claude reported an error").to_string(),
                                );
                        }
                        let mut state = ai.state.lock().await;
                        ai_status_set(&mut state, &ai.events, AiStatus::Waiting);
                        ai_notify(
                            &ai,
                            AiStatus::Waiting,
                            "Claude is waiting for you",
                            value["result"].as_str().unwrap_or_default().to_string(),
                        );
                    },
                    _ => { },
                }
            }
            let stderr = stderr_task.await.unwrap_or_default();
            let mut state = ai.state.lock().await;
            let Some(mut session) = state.session.take() else {
                return;
            };
            let status = match tokio::time::timeout(Duration::from_secs(2), session.child.wait()).await {
                Ok(Ok(status)) => status.to_string(),
                _ => {
                    _ = session.child.kill().await;
                    "killed".to_string()
                },
            };
            let stderr = stderr.trim();
            let text = if stderr.is_empty() {
                format!("Claude exited ({})", status)
            } else {
                format!("Claude exited ({}):\n{}", status, stderr)
            };
            _ = ai_message(&log, &ai.events, AiRole::System, text.clone());
            ai_status_set(&mut state, &ai.events, AiStatus::Off);
            ai_notify(&ai, AiStatus::Off, "Claude exited", text);
        }
    });
    return Ok(AiSession {
        child: child,
        stdin: stdin,
        log: log,
        reader: reader,
    });
}

pub fn ai_status_set(state: &mut AiState, events: &Events, status: AiStatus) {
    state.status = status;
    if let Some(notification) = state.notification.take() {
        tokio::spawn(async move {
            notification.close_async().await;
        });
    }
    events.events_publish(Event::AiStatus { status: status });
    return;
}

pub struct AiSession {
    pub child: Child,
    pub log: PathBuf,
    pub reader: tokio::task::JoinHandle<()>,
    pub stdin: ChildStdin,
}

pub struct AiState {
    pub notification: Option<NotificationHandle>,
    pub session: Option<AiSession>,
    pub status: AiStatus,
}

pub fn now_ms() -> u64 {
    return SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
}
