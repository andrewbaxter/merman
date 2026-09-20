use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

pub const API_PATH: &str = "/api";

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

glove::reqresp!(pub api {
    Start(ReqStart) => RespStart,
    List(ReqList) => RespList,
    Open(ReqOpen) => RespOpen,
});
