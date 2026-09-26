use merman3_api::Event;
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::broadcast;

pub struct Events {
    pub buffer: Mutex<(u64, VecDeque<(u64, Event)>)>,
    pub tx: broadcast::Sender<(u64, Event)>,
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
}
