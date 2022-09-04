use std::collections::HashMap;
use async_trait::async_trait;

use crate::Clients;
use crate::ws::TopicRequestReceiver;

#[async_trait]
pub trait Receiver: Send + Sync {
    async fn receive_msg(&self, id: &str, msg: &str, clients: &Clients, redis_client: redis::Client);
}

pub struct ReceiverManager {
    pub receivers: HashMap<String, Box<dyn Receiver>>,
}