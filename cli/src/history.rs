use {
    merman::compress::{
        encode_document,
        format_document,
        read_document,
    },
    merman_core::spec::SpecCompression,
    good_ormning::sqlite::{
        good_query,
        good_query_many,
        good_query_one,
        good_query_opt,
    },
    loga::{
        ResultContext,
        ea,
    },
    merman_core::{
        matcher::source_parse,
        patch::{
            Patch,
            SetTarget,
            patch_apply_json,
        },
    },
    serde_json::Value,
    std::{
        collections::HashMap,
        path::{
            Path,
            PathBuf,
        },
        sync::{
            Arc,
            Mutex,
        },
        time::Duration,
    },
    tokio::task::JoinHandle,
};

good_ormning::good_module!(pub dbm);

pub fn apply_all(value: &mut Value, patches: &[Patch]) -> Result<Vec<(Patch, Patch)>, String> {
    let mut steps = vec![];
    for patch in patches {
        match patch_apply_json(value, patch) {
            Ok(reverse) => steps.push((patch.clone(), reverse)),
            Err(e) => {
                for (_, reverse) in steps.iter().rev() {
                    patch_apply_json(value, reverse).expect("undoing a patch just applied failed");
                }
                return Err(e);
            },
        }
    }
    return Ok(steps);
}

pub struct FileState {
    pub compression: SpecCompression,
    pub disk: String,
    pub dirty: bool,
    pub id: i64,
    pub mergeable: bool,
    pub position: i64,
    pub revision: u64,
    pub unwritten_revision: u64,
    pub value: Value,
    pub write: Option<JoinHandle<()>>,
}

pub struct History {
    pub db: Mutex<dbm::Db<rusqlite::Connection>>,
    pub files: Mutex<HashMap<PathBuf, FileState>>,
}

impl History {
    pub fn history_schedule_write(self: &Arc<Self>, path: &Path) {
        let task = tokio::spawn({
            let history = self.clone();
            let path = path.to_path_buf();
            async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if let Err(e) = history.history_write(&path) {
                    eprintln!("Error writing {}: {}", path.display(), e);
                }
            }
        });
        let mut files = self.files.lock().unwrap();
        let Some(state) = files.get_mut(path) else {
            return;
        };
        if let Some(old) = state.write.replace(task) {
            old.abort();
        }
    }

    pub fn history_state<
        'a,
    >(
        &self,
        db: &mut dbm::Db<rusqlite::Connection>,
        files: &'a mut HashMap<PathBuf, FileState>,
        path: &Path,
        compression: impl FnOnce() -> Result<SpecCompression, loga::Error>,
    ) -> Result<&'a mut FileState, loga::Error> {
        if !files.contains_key(path) {
            let path_text = path.to_string_lossy().into_owned();
            let compression = compression()?;
            let text = read_document(path, compression)?;
            let row = good_query_opt!(
                dbm,
                "select rowid, position, disk, disk_position from file where path = ${string = &path_text}";
                &mut *db
            ).map_err(|e| loga::err(e.to_string()))?;
            let (id, position, disk, disk_position) = match row {
                Some(r) => (r.rowid, r.position, r.disk, r.disk_position),
                None => {
                    let id = good_query_one!(
                        dbm,
                        "insert into file (path, position, disk, disk_position) values (${string = &path_text}, 0, ${string = &text}, 0) returning rowid";
                        &mut *db
                    ).map_err(|e| loga::err(e.to_string()))?;
                    (id, 0, text.clone(), Some(0))
                },
            };
            let mut state = FileState {
                compression: compression,
                disk: disk.clone(),
                dirty: false,
                id: id,
                mergeable: false,
                position: position,
                revision: 0,
                unwritten_revision: 0,
                value: Value::Null,
                write: None,
            };
            let recovered = (|| -> Result<Value, String> {
                let mut value = source_parse(&disk).map_err(|e| e.to_string())?;
                let Some(disk_position) = disk_position else {
                    return Err("the written file isn't in the history".to_string());
                };
                let (from, to) = (disk_position.min(position), disk_position.max(position));
                let levels = good_query_many!(
                    dbm,
                    "select steps from level where file = ${i64 = id} and seq >= ${i64 = from} and seq < ${i64 = to} order by seq";
                    &mut *db
                ).map_err(|e| e.to_string())?;
                let mut levels: Vec<Vec<(Patch, Patch)>> =
                    levels
                        .into_iter()
                        .map(|l| serde_json::from_str(&l))
                        .collect::<Result<_, _>>()
                        .map_err(|e| e.to_string())?;
                if levels.len() as i64 != to - from {
                    return Err("levels are missing".to_string());
                }
                if disk_position > position {
                    levels.reverse();
                }
                for steps in levels {
                    let patches: Vec<Patch> = if disk_position < position {
                        steps.into_iter().map(|(forward, _)| forward).collect()
                    } else {
                        steps.into_iter().rev().map(|(_, reverse)| reverse).collect()
                    };
                    apply_all(&mut value, &patches)?;
                }
                return Ok(value);
            })();
            match recovered {
                Ok(value) => {
                    state.dirty = disk_position != Some(position);
                    state.value = value;
                },
                Err(e) => {
                    eprintln!("Starting the undo history of {} over: {}", path.display(), e);
                    good_query!(
                        dbm,
                        "delete from level where file = ${i64 = id}";
                        &mut *db
                    ).map_err(|e| loga::err(e.to_string()))?;
                    state.position = 0;
                    state.disk = text.clone();
                    state.value =
                        source_parse(&text).context_with("Error parsing source JSON", ea!(path = path.display()))?;
                    good_query!(
                        dbm,
                        "update file set position = 0, disk = ${string = &text}, disk_position = 0 where rowid = ${i64 = id}";
                        &mut *db
                    ).map_err(|e| loga::err(e.to_string()))?;
                },
            }
            history_take_disk(db, &mut state, path)?;
            files.insert(path.to_path_buf(), state);
        }
        return Ok(files.get_mut(path).unwrap());
    }

    pub fn history_write(&self, path: &Path) -> Result<(), loga::Error> {
        let mut db = self.db.lock().unwrap();
        let mut files = self.files.lock().unwrap();
        let Some(state) = files.get_mut(path) else {
            return Ok(());
        };
        if !state.dirty {
            return Ok(());
        }
        if history_take_disk(&mut db, state, path)? {
            return Ok(());
        }
        let text = format_document(&state.value);
        let temp =
            path.with_file_name(
                format!(
                    ".{}.merman_write",
                    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                ),
            );
        let bytes = encode_document(&text, state.compression)?;
        std::fs::write(&temp, &bytes).context_with("Error writing file", ea!(path = temp.display()))?;
        std::fs::rename(&temp, path).context_with("Error replacing file", ea!(path = path.display()))?;
        state.disk = text;
        state.dirty = false;
        good_query!(
            dbm,
            "update file set disk = ${string = &state.disk}, disk_position = ${opt i64 = Some(state.position)} where rowid = ${i64 = state.id}";
            &mut *db
        ).map_err(|e| loga::err(e.to_string()))?;
        return Ok(());
    }
}

pub fn history_level_push(
    db: &mut dbm::Db<rusqlite::Connection>,
    state: &mut FileState,
    steps: &[(Patch, Patch)],
    select_before: Option<String>,
    select_after: Option<String>,
) -> Result<(), loga::Error> {
    good_query!(
        dbm,
        "delete from level where file = ${i64 = state.id} and seq >= ${i64 = state.position}";
        &mut *db
    ).map_err(|e| loga::err(e.to_string()))?;
    good_query!(
        dbm,
        "insert into level (file, seq, steps, select_before, select_after) values (${i64 = state.id}, ${i64 = state.position}, ${string = &serde_json::to_string(steps).unwrap()}, ${opt string = select_before.as_deref()}, ${opt string = select_after.as_deref()})";
        &mut *db
    ).map_err(|e| loga::err(e.to_string()))?;
    state.position += 1;
    good_query!(
        dbm,
        "update file set position = ${i64 = state.position}, disk_position = case when disk_position >= ${i64 = state.position} then null else disk_position end where rowid = ${i64 = state.id}";
        &mut *db
    ).map_err(|e| loga::err(e.to_string()))?;
    return Ok(());
}

pub fn history_take_disk(
    db: &mut dbm::Db<rusqlite::Connection>,
    state: &mut FileState,
    path: &Path,
) -> Result<bool, loga::Error> {
    let text = read_document(path, state.compression)?;
    if text == state.disk {
        return Ok(false);
    }
    let Ok(value) = source_parse(&text) else {
        return Ok(false);
    };
    let unwritten = state.dirty;
    if let Some(write) = state.write.take() {
        write.abort();
    }
    if value != state.value {
        let old = std::mem::replace(&mut state.value, value.clone());
        history_level_push(db, state, &[(Patch::Set {
            path: vec![],
            target: SetTarget::Document,
            value: value,
        }, Patch::Set {
            path: vec![],
            target: SetTarget::Document,
            value: old,
        })], None, None)?;
        state.revision += 1;
        if unwritten {
            state.unwritten_revision = state.revision;
        }
    }
    state.disk = text;
    state.dirty = false;
    state.mergeable = false;
    good_query!(
        dbm,
        "update file set disk = ${string = &state.disk}, disk_position = ${opt i64 = Some(state.position)} where rowid = ${i64 = state.id}";
        &mut *db
    ).map_err(|e| loga::err(e.to_string()))?;
    return Ok(true);
}
