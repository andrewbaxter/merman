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
    /// The cached state of one file: whether it is being compiled and the results of
    /// every compile of it the server knows.
    FileState {
        path: String,
    },
    /// The text of a module the editor can't read itself (a remote source).
    SourceRead {
        source: Source,
    },
    /// While paused the server still announces `Dirty` for changed files but doesn't
    /// compile them until unpaused or flushed.
    Pause {
        paused: bool,
    },
    /// Compile everything pending now, even while paused, and reply once every
    /// affected source has settled.
    Flush,
    /// The metadata of every type an expression evaluated to, from each compile that
    /// evaluated it.
    ExprMeta {
        source: Source,
        expr: i64,
    },
    /// The named scope entries visible to an expression, from each compile of its source:
    /// what was bound before its enclosing statement and not unbound since.
    ScopeMeta {
        source: Source,
        expr: i64,
    },
    /// Metadata records by id, for following a field's reference to its type.
    Meta {
        ids: Vec<i64>,
    },
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
    /// Every compile unit compiled by the flush.
    Flush {
        compiled: Vec<Compiled>,
    },
    ExprMeta {
        metas: Vec<ExprMeta>,
    },
    ScopeMeta {
        scopes: Vec<ScopeMeta>,
    },
    /// Records in the order asked, leaving out ids that don't exist.
    Meta {
        metas: Vec<Meta>,
    },
    Failed {
        message: String,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Announce {
    /// The file (or something it depends on) changed; its previous results are void.
    Dirty {
        path: String,
    },
    /// One compile unit of a source finished, or was loaded from the cache.
    Compiled(Compiled),
    /// Nothing is pending for the source any more.
    Settled {
        path: String,
    },
    /// The file was deleted.
    Removed {
        path: String,
    },
}

/// The cached state of one file: whether it is being compiled and the results of
/// every compile of it the server knows.
#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileState {
    pub dirty: bool,
    pub compiles: Vec<Compiled>,
}

/// One compile of a source. A source is compiled once per import state it is
/// imported under, so a file can have several.
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
    /// The `id` of the expression in the module's JSON; none for the module as a whole.
    pub expr: Option<i64>,
}

/// The type an expression evaluated to in one compile of a source.
#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExprMeta {
    pub source: Source,
    pub import_state: i64,
    pub meta: Meta,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ScopeMeta {
    pub source: Source,
    pub import_state: i64,
    /// Innermost last; a name bound twice appears twice.
    pub entries: Vec<ScopeEntry>,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ScopeEntry {
    pub name: String,
    pub meta: Option<Meta>,
}

/// A type's metadata: an id for the kind of type, a data object the type chose, and the
/// fields a value of the type has.
#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub id: i64,
    pub kind: String,
    pub data: serde_json::Value,
    pub fields: Vec<MetaField>,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct MetaField {
    pub name: String,
    pub data: serde_json::Value,
    /// The id of the field's type's metadata, when the type has one.
    pub meta: Option<i64>,
}

#[derive(Serialize, Deserialize, JsonSchema, Maskoidy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Local {
        path: String,
    },
    /// The server's own name for a non-file source, usable in `SourceRead`.
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
