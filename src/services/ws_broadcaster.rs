use tokio::sync::broadcast;

/// Broadcasts action verdict events to all connected admin dashboard WebSocket clients.
#[derive(Clone)]
pub struct WsBroadcaster {
    tx: broadcast::Sender<String>,
}

impl WsBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn send(&self, msg: String) {
        let _ = self.tx.send(msg);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }
}
