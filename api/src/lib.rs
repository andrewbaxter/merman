use {
    schemars::JsonSchema,
    serde::{
        Deserialize,
        Serialize,
    },
};

glove::reqresp!(pub api {
    Start(ReqStart) => RespStart,
    List(ReqList) => RespList,
    Open(ReqOpen) => RespOpen,
    AiSend(ReqAiSend) => RespAiSend,
    AiClear(ReqAiClear) => RespAiClear,
    AiHistory(ReqAiHistory) => RespAiHistory,
    AiSessions(ReqAiSessions) => RespAiSessions,
    AiResume(ReqAiResume) => RespAiResume,
    LocationSet(ReqLocationSet) => RespLocationSet,
    Edit(ReqEdit) => RespEdit,
    Undo(ReqUndo) => RespUndo,
    Redo(ReqRedo) => RespRedo,
    Sync(ReqSync) => RespSync,
    Flush(ReqFlush) => RespFlush,
});

pub const API_PATH: &str = "/api";
pub const WS_PATH: &str = "/api/ws";

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AiMessage {
    pub role: AiRole,
    pub text: String,
    pub time: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AiRole {
    Assistant,
    System,
    Tool,
    User,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AiSessionInfo {
    pub first: String,
    pub id: String,
    pub last_time: u64,
    pub messages: usize,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AiStatus {
    Off,
    Thinking,
    Waiting,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Ai {
        message: AiMessage,
    },
    AiStatus {
        status: AiStatus,
    },
    FileChanged {
        path: String,
    },
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListEntry {
    pub dir: bool,
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqAiClear {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqAiHistory {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqAiResume {
    pub id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqAiSend {
    pub text: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqAiSessions {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqEdit {
    pub new_level: bool,
    pub patches: String,
    pub path: String,
    pub revision: u64,
    pub select_after: Option<String>,
    pub select_before: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqFlush {
    pub path: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqList {
    pub dir: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqLocationSet {
    pub location: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqOpen {
    pub path: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqRedo {
    pub path: String,
    pub revision: u64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqStart {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqSync {
    pub path: String,
    pub revision: u64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqUndo {
    pub path: String,
    pub revision: u64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespAiClear {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespAiHistory {
    pub messages: Vec<AiMessage>,
    pub status: AiStatus,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespAiResume {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespAiSend {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespAiSessions {
    pub sessions: Vec<AiSessionInfo>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespEdit {
    pub accepted: bool,
    pub revision: u64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespFlush {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespHistoryStep {
    pub patches: String,
    pub select: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespList {
    pub dir: String,
    pub entries: Vec<ListEntry>,
    pub location: Option<String>,
    pub parent: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespLocationSet {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespOpen {
    pub location: Option<String>,
    pub revision: u64,
    pub source: String,
    pub syntax: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespRedo {
    pub revision: u64,
    pub step: Option<RespHistoryStep>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespStart {
    pub dir: String,
    pub file: Option<String>,
    pub keys: String,
    pub theme: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespSync {
    pub revision: u64,
    pub source: Option<String>,
    pub unwritten: bool,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespUndo {
    pub revision: u64,
    pub step: Option<RespHistoryStep>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WsClient {
    pub since: Option<u64>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WsServer {
    Event {
        seq: u64,
        event: Event,
    },
    Gap,
}
