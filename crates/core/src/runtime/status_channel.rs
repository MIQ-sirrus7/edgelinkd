use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tokio::sync::broadcast;

use crate::runtime::{model::ElementId, nodes::StatusObject};

/// Status 消息结构，匹配 Node-RED 的格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMessage {
    pub sender_id: ElementId,
    pub status: StatusObject,
}

/// Synchronous observers keep editor state in publication order, including across redeploys.
pub trait StatusObserver: std::fmt::Debug + Send + Sync {
    fn update(&self, message: &StatusMessage);
    fn clear(&self);
}

type Observers = OnceLock<Mutex<Vec<Weak<dyn StatusObserver>>>>;

/// Status 消息通道
#[derive(Debug, Clone)]
pub struct StatusChannel {
    sender: broadcast::Sender<StatusMessage>,
    observers: Arc<Observers>,
}

impl StatusChannel {
    /// 创建新的 Status 通道
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender, observers: Arc::default() }
    }

    /// 发送 Status 消息
    pub fn send(&self, message: StatusMessage) {
        self.notify(|observer| observer.update(&message));
        // `broadcast::Sender::send` only fails when nobody is subscribed, which is the normal state
        // in headless mode (or with the editor closed): that is not a failure worth a warning.
        if self.sender.send(message).is_err() {
            log::trace!("Dropped a status message: no subscriber is listening");
        }
    }

    /// 获取 Status 消息接收器
    pub fn subscribe(&self) -> broadcast::Receiver<StatusMessage> {
        self.sender.subscribe()
    }

    pub fn observe(&self, observer: Weak<dyn StatusObserver>) {
        let mut observers = self.observers.get_or_init(Mutex::default).lock().unwrap();
        observers.retain(|entry| entry.strong_count() > 0);
        if !observers.iter().any(|entry| entry.ptr_eq(&observer)) {
            observers.push(observer);
        }
    }

    fn notify(&self, visit: impl Fn(&dyn StatusObserver)) {
        if let Some(observers) = self.observers.get() {
            for observer in observers.lock().unwrap().iter().filter_map(Weak::upgrade) {
                visit(observer.as_ref());
            }
        }
    }

    pub(crate) fn clear_retained(&self) {
        self.notify(|observer| observer.clear());
    }
}
