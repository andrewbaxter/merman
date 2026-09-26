use loga::{
    ea,
    ResultContext,
};
use crate::events::Events;
use merman3_api::{
    AiMessage,
    AiRole,
    AiStatus,
    Event,
};
use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};
use tokio::process::{
    Child,
    ChildStdin,
};
use tokio::sync::Mutex;

pub struct Ai {
    pub dir: PathBuf,
    pub logs: PathBuf,
    pub events: Arc<Events>,
    pub state: Mutex<AiState>,
}

pub struct AiState {
    pub status: AiStatus,
    pub session: Option<AiSession>,
}

pub struct AiSession {
    pub child: Child,
    pub stdin: ChildStdin,
    pub log: PathBuf,
    pub reader: tokio::task::JoinHandle<()>,
}

pub fn now_ms() -> u64 {
    return SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
}

pub fn ai_status_set(state: &mut AiState, events: &Events, status: AiStatus) {
    state.status = status;
    events.events_publish(Event::AiStatus { status: status });
    return;
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
