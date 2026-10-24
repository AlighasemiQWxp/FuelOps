use mongodb::bson::Document;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppEvent {
    pub id: String,
    pub name: String,
    pub org_id: String,
    pub station_id: String,
    pub entity_id: String,
    pub occurred_at: i64,
    pub payload: Document,
}

pub trait DomainEvent: Serialize {
    const NAME: &'static str;
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AppEvent>,
}
impl Default for EventBus {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }
}
impl EventBus {
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.sender.subscribe()
    }
    pub fn publish(&self, event: AppEvent) {
        let _ = self.sender.send(event);
    }
}
