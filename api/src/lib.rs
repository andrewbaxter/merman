use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

pub const API_PATH: &str = "/api";
pub const WS_PATH: &str = "/api/ws";

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqStart {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespStart {
    pub dir: String,
    pub file: Option<String>,
    pub keys: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqList {
    pub dir: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListEntry {
    pub name: String,
    pub path: String,
    pub dir: bool,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespList {
    pub dir: String,
    pub parent: Option<String>,
    pub entries: Vec<ListEntry>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqOpen {
    pub path: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespOpen {
    pub syntax: String,
    pub source: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqAiSend {
    pub text: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespAiSend {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqAiClear {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespAiClear {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReqAiHistory {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RespAiHistory {
    pub status: AiStatus,
    pub messages: Vec<AiMessage>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AiStatus {
    Off,
    Thinking,
    Waiting,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AiRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AiMessage {
    pub role: AiRole,
    pub text: String,
    pub time: u64,
}

glove::reqresp!(pub api {
    Start(ReqStart) => RespStart,
    List(ReqList) => RespList,
    Open(ReqOpen) => RespOpen,
    AiSend(ReqAiSend) => RespAiSend,
    AiClear(ReqAiClear) => RespAiClear,
    AiHistory(ReqAiHistory) => RespAiHistory,
});

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    FileChanged {
        path: String,
    },
    Ai {
        message: AiMessage,
    },
    AiStatus {
        status: AiStatus,
    },
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

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WsClient {
    pub since: Option<u64>,
}
