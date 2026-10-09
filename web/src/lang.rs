use {
    crate::client::client_send,
    merman_api::ReqLangFileState,
    merman_langserver::{
        Announce,
        CompileError,
        Compiled,
        Source,
    },
    std::{
        cell::RefCell,
        collections::{
            BTreeMap,
            HashMap,
            HashSet,
        },
        path::Path,
        rc::Rc,
    },
};

#[derive(Default, Clone)]
pub struct LangFile {
    pub dirty: HashSet<String>,
    pub compiles: BTreeMap<(String, i64), Compiled>,
}

pub struct Lang {
    pub changed: RefCell<Option<Rc<dyn Fn(&str)>>>,
    files: RefCell<HashMap<String, LangFile>>,
    servers: RefCell<BTreeMap<String, bool>>,
}

pub fn lang_source_key(source: &Source) -> &str {
    match source {
        Source::Local { path } => return path,
        Source::Remote { spec } => return spec,
    }
}

impl Lang {
    pub fn lang_new() -> Rc<Lang> {
        return Rc::new(Lang {
            changed: RefCell::new(None),
            files: RefCell::new(HashMap::new()),
            servers: RefCell::new(BTreeMap::new()),
        });
    }

    fn lang_notify(&self, path: &str) {
        let changed = self.changed.borrow().clone();
        if let Some(changed) = changed {
            changed(path);
        }
        return;
    }

    pub fn lang_announce(&self, server: &str, announce: Announce) {
        let path = {
            let mut files = self.files.borrow_mut();
            match announce {
                Announce::Dirty { path } => {
                    let file = files.entry(path.clone()).or_default();
                    file.dirty.insert(server.to_string());
                    file.compiles.retain(|(s, _), _| s != server);
                    path
                },
                Announce::Compiled(compiled) => {
                    let path = lang_source_key(&compiled.source).to_string();
                    files
                        .entry(path.clone())
                        .or_default()
                        .compiles
                        .insert((server.to_string(), compiled.import_state), compiled);
                    path
                },
                Announce::Settled { path } => {
                    files.entry(path.clone()).or_default().dirty.remove(server);
                    path
                },
                Announce::Removed { path } => {
                    if let Some(file) = files.get_mut(&path) {
                        file.dirty.remove(server);
                        file.compiles.retain(|(s, _), _| s != server);
                        if file.dirty.is_empty() && file.compiles.is_empty() {
                            files.remove(&path);
                        }
                    }
                    path
                },
            }
        };
        self.lang_notify(&path);
        return;
    }

    pub fn lang_status(&self, server: &str, running: bool) {
        self.servers.borrow_mut().insert(server.to_string(), running);
        return;
    }

    pub fn lang_running(&self) -> bool {
        return self.servers.borrow().values().any(|r| *r);
    }

    pub fn lang_path_relevant(path: &str) -> bool {
        return Path::new(path).extension().is_some();
    }

    pub fn lang_load(self: &Rc<Self>, path: String) {
        wasm_bindgen_futures::spawn_local({
            let lang = self.clone();
            async move {
                let Ok(resp) = client_send(ReqLangFileState { path: path.clone() }).await else {
                    return;
                };
                {
                    let mut files = lang.files.borrow_mut();
                    let file = files.entry(path.clone()).or_default();
                    for server in resp.servers {
                        lang.servers.borrow_mut().insert(server.server.clone(), server.running);
                        file.compiles.retain(|(s, _), _| *s != server.server);
                        file.dirty.remove(&server.server);
                        let Some(state) = server.state else {
                            continue;
                        };
                        if state.dirty {
                            file.dirty.insert(server.server.clone());
                        }
                        for compiled in state.compiles {
                            file.compiles.insert((server.server.clone(), compiled.import_state), compiled);
                        }
                    }
                }
                lang.lang_notify(&path);
            }
        });
        return;
    }

    pub fn lang_file(&self, path: &str) -> Option<LangFile> {
        return self.files.borrow().get(path).cloned();
    }

    pub fn lang_errors(&self, path: &str) -> Vec<CompileError> {
        let mut out = vec![];
        for file in self.files.borrow().values() {
            for compiled in file.compiles.values() {
                for error in &compiled.errors {
                    if lang_source_key(&error.location.source) == path {
                        out.push(error.clone());
                    }
                }
            }
        }
        return out;
    }
}
