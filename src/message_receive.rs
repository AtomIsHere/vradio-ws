use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;

use crate::Clients;

#[async_trait]
pub trait Receiver: Send + Sync {
    // Function is implemented by receivers and ran when a message is received
    async fn receive_msg(&self, id: &str, msg: &str, clients: &Clients, redis_client: redis::Client);
}

// Structure for storing a list of receivers
pub struct ReceiverManager {
    pub receivers: HashMap<String, Arc<dyn Receiver>>,
}