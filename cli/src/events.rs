use {
    merman_api::Event,
    std::{
        collections::{
            HashMap,
            VecDeque,
        },
        path::{
            Path,
            PathBuf,
        },
        sync::Mutex,
        time::SystemTime,
    },
    tokio::sync::broadcast,
};

pub struct Events {
    pub buffer: Mutex<(u64, VecDeque<(u64, Event)>)>,
    pub tx: broadcast::Sender<(u64, Event)>,
    pub stamps: Mutex<HashMap<PathBuf, Option<(SystemTime, u64)>>>,
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    return Some((meta.modified().ok()?, meta.len()));
}

impl Events {
    pub fn events_publish(&self, event: Event) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.0 += 1;
        let seq = buffer.0;
        buffer.1.push_back((seq, event.clone()));
        if buffer.1.len() > 1024 {
            buffer.1.pop_front();
        }
        _ = self.tx.send((seq, event));
        return;
    }

    pub fn events_stamp(&self, path: &Path) {
        self.stamps.lock().unwrap().insert(path.to_path_buf(), stamp(path));
        return;
    }

    pub fn events_poll(&self) {
        let mut changed = vec![];
        for (path, old) in self.stamps.lock().unwrap().iter_mut() {
            let new = stamp(path);
            if new != *old {
                *old = new;
                changed.push(path.to_string_lossy().into_owned());
            }
        }
        for path in changed {
            self.events_publish(Event::FileChanged { path: path });
        }
        return;
    }
}
