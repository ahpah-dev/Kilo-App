use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Default)]
pub struct CancelToken {
    flag: AtomicBool,
    notify: Notify,
}

impl CancelToken {
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        loop {
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// In-flight operations (streams, shell commands) addressable by id.
#[derive(Default)]
pub struct OperationRegistry {
    ops: Mutex<HashMap<String, Arc<CancelToken>>>,
}

impl OperationRegistry {
    pub fn register(&self, id: &str) -> Arc<CancelToken> {
        let token = Arc::new(CancelToken::default());
        self.ops
            .lock()
            .expect("registry lock poisoned")
            .insert(id.to_string(), token.clone());
        token
    }

    pub fn finish(&self, id: &str) {
        self.ops.lock().expect("registry lock poisoned").remove(id);
    }

    pub fn cancel(&self, id: &str) -> bool {
        match self.ops.lock().expect("registry lock poisoned").get(id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }
}
