use {
    schemars::JsonSchema,
    schemask_core::{
        Maskoidy,
        Schemask,
    },
    schemask_derive::Maskoidy,
    serde::{
        Deserialize,
        Serialize,
    },
};

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ClientMessage {
    pub id: u64,
    pub req: Request,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    FileState {
        path: String,
    },
    SourceRead {
        source: Source,
    },
    Pause {
        paused: bool,
    },
    Flush,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerMessage {
    Res {
        id: u64,
        res: Response,
    },
    Ev {
        ev: Announce,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    FileState(FileState),
    SourceRead {
        text: Option<String>,
    },
    Pause,
    Flush {
        compiled: Vec<Compiled>,
    },
    Failed {
        message: String,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Announce {
    Dirty {
        path: String,
    },
    Compiled(Compiled),
    Settled {
        path: String,
    },
    Removed {
        path: String,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileState {
    pub dirty: bool,
    pub compiles: Vec<Compiled>,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Compiled {
    pub source: Source,
    pub import_state: i64,
    pub started_ms: i64,
    pub ended_ms: i64,
    pub logs: Vec<Log>,
    pub errors: Vec<CompileError>,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Log {
    pub at_ms: i64,
    pub message: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CompileError {
    pub location: Location,
    pub message: String,
    pub related: Vec<Related>,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Related {
    pub location: Location,
    pub description: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub source: Source,
    pub expr: Option<i64>,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Local {
        path: String,
    },
    Remote {
        spec: String,
    },
}

pub fn schema() -> Schemask {
    let Schemask::V1(mut server) = ServerMessage::schemask();
    let Schemask::V1(client) = ClientMessage::schemask();
    server.bindings.extend(client.bindings);
    return Schemask::V1(server);
}

#[cfg(test)]
mod tests {
    use super::schema;

    #[test]
    fn schema_json_current() {
        let expected = serde_json::to_string_pretty(&schema()).unwrap();
        let actual = include_str!("../schema.json");
        assert!(
            actual.trim_end() == expected.trim_end(),
            "schema.json is stale; regenerate with `cargo run -p merman_langserver --bin merman-langserver-schema > langserver/schema.json`"
        );
    }
}
