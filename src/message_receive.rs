use async_trait::async_trait;

use crate::Clients;
use crate::ws::TopicRequestReceiver;

#[async_trait]
pub trait Receiver {
    async fn receive_msg(&self, id: &str, msg: &str, clients: &Clients);
}

pub fn get_receiver(id: &str) -> Result<&dyn Receiver, String> {
    let receive = match id {
        "topic_request" => Ok(&(TopicRequestReceiver {}) as &dyn Receiver),
        _ => Err("Invalid receiver id: ".to_owned() + id)
    };

    return receive;
}