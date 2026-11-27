use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use edgelink_core::runtime::{
    model::ElementId,
    nodes::StatusObject,
    status_channel::{StatusMessage, StatusObserver},
};
use tokio::sync::broadcast;

use super::{CommsManager, NodeRedMessage};

#[derive(Debug, Default)]
pub(super) struct EditorStatuses(Mutex<State>);

#[derive(Debug, Default)]
struct State {
    latest: HashMap<ElementId, StatusObject>,
    subscribers: HashMap<String, Subscriber>,
}

#[derive(Debug)]
struct Subscriber {
    topics: HashSet<String>,
    tx: broadcast::Sender<String>,
}

fn message(topic: String, status: &StatusObject) -> NodeRedMessage {
    NodeRedMessage { topic, data: serde_json::to_value(status).expect("serializable node status") }
}

impl State {
    fn publish(&self, id: ElementId, status: &StatusObject) {
        if self.subscribers.is_empty() {
            return;
        }
        let topic = format!("status/{id}");
        let mut encoded = None;
        for subscriber in self.subscribers.values() {
            if subscriber.topics.contains("status/#") || subscriber.topics.contains(&topic) {
                let frame =
                    encoded.get_or_insert_with(|| CommsManager::serialize_batch(&[message(topic.clone(), status)]));
                let _ = subscriber.tx.send(frame.clone());
            }
        }
    }
}

impl StatusObserver for EditorStatuses {
    fn update(&self, event: &StatusMessage) {
        let mut state = self.0.lock().unwrap();
        let status = &event.status;
        if status.fill.is_none() && status.shape.is_none() && status.text.as_deref().unwrap_or("").is_empty() {
            if state.latest.remove(&event.sender_id).is_some() {
                state.publish(event.sender_id, &StatusObject::empty());
            }
        } else {
            if state.subscribers.is_empty() && state.latest.get(&event.sender_id) == Some(status) {
                return;
            }
            state.latest.insert(event.sender_id, status.clone());
            state.publish(event.sender_id, status);
        }
    }

    fn clear(&self) {
        let mut state = self.0.lock().unwrap();
        for subscriber in state.subscribers.values() {
            let cleared: Vec<_> = state
                .latest
                .keys()
                .map(|id| format!("status/{id}"))
                .filter(|name| subscriber.topics.contains("status/#") || subscriber.topics.contains(name))
                .map(|name| message(name, &StatusObject::empty()))
                .collect();
            if !cleared.is_empty() {
                let _ = subscriber.tx.send(CommsManager::serialize_batch(&cleared));
            }
        }
        state.latest.clear();
    }
}

impl EditorStatuses {
    pub(super) fn subscribe(&self, id: &str, topic: &str, tx: &broadcast::Sender<String>) {
        // Share the lock with publication so the snapshot is queued before any newer updates.
        let mut state = self.0.lock().unwrap();
        let State { latest, subscribers } = &mut *state;
        let subscriber =
            subscribers.entry(id.to_owned()).or_insert_with(|| Subscriber { topics: HashSet::new(), tx: tx.clone() });
        if subscriber.topics.contains(topic) {
            return;
        }
        let batch = latest
            .iter()
            .filter_map(|(node_id, status)| {
                let name = format!("status/{node_id}");
                ((topic == "status/#" || topic == name)
                    && !subscriber.topics.contains("status/#")
                    && !subscriber.topics.contains(&name))
                .then(|| message(name, status))
            })
            .collect::<Vec<_>>();
        subscriber.topics.insert(topic.to_owned());
        if !batch.is_empty() {
            let _ = subscriber.tx.send(CommsManager::serialize_batch(&batch));
        }
    }

    pub(super) fn unsubscribe(&self, id: &str, topic: &str) {
        let mut state = self.0.lock().unwrap();
        if let Some(subscriber) = state.subscribers.get_mut(id) {
            subscriber.topics.remove(topic);
            if subscriber.topics.is_empty() {
                state.subscribers.remove(id);
            }
        }
    }

    pub(super) fn disconnect(&self, id: &str) {
        self.0.lock().unwrap().subscribers.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::comms::handle_websocket_message;
    use edgelink_core::runtime::status_channel::StatusChannel;
    use serde_json::{Value, json};

    fn event(id: u64, text: &str) -> StatusMessage {
        StatusMessage {
            sender_id: id.into(),
            status: StatusObject { text: Some(text.into()), ..StatusObject::empty() },
        }
    }

    #[tokio::test]
    async fn late_subscribers_receive_current_status_once_then_live_changes() {
        let comms = CommsManager::new();
        let channel = StatusChannel::new(8);
        comms.observe_status(&channel);
        let mut raw = channel.subscribe();
        for text in ["waiting", "ready", "ready"] {
            channel.send(event(1, text));
        }
        for _ in 0..3 {
            raw.try_recv().unwrap();
        } // Existing runtime subscribers still see each event.
        channel.send(event(2, "other"));
        let (tx, mut rx) = broadcast::channel(8);
        comms.add_connection("viewer".into(), tx.clone()).await;
        let topic = "status/0000000000000001";
        let parse = |s: String| serde_json::from_str::<Value>(&s).unwrap();
        handle_websocket_message(json!({"subscribe":topic}), &tx, "viewer", &comms, None).await;
        assert_eq!(parse(rx.try_recv().unwrap()), json!([{"topic":topic,"data":{"text":"ready"}}]));
        comms.subscribe("viewer", topic).await;
        assert!(rx.try_recv().is_err());
        channel.send(event(2, "second"));
        assert!(rx.try_recv().is_err());
        comms.subscribe("viewer", "status/#").await;
        assert_eq!(parse(rx.try_recv().unwrap())[0]["data"]["text"], "second");
        channel.send(StatusMessage { sender_id: 1.into(), status: StatusObject::empty() });
        assert_eq!(parse(rx.try_recv().unwrap()), json!([{"topic":topic,"data":{}}]));
        comms.remove_connection("viewer").await;
        channel.send(event(2, "latest"));
        assert!(rx.try_recv().is_err());
        comms.add_connection("later".into(), tx.clone()).await;
        comms.subscribe("later", "status/#").await;
        let snapshot = parse(rx.try_recv().unwrap());
        assert_eq!(snapshot.as_array().unwrap().len(), 1);
        assert_eq!(snapshot[0]["data"]["text"], "latest");
        comms.statuses.clear();
        assert_eq!(parse(rx.try_recv().unwrap())[0]["data"], json!({}));
        comms.unsubscribe("later", "status/#").await;
        channel.send(event(1, "new deployment"));
        assert!(rx.try_recv().is_err());
        comms.subscribe("later", "status/#").await;
        assert_eq!(parse(rx.try_recv().unwrap()), json!([{"topic":topic,"data":{"text":"new deployment"}}]));
    }
}
