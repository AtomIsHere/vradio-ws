use async_trait::async_trait;
use futures::{FutureExt, StreamExt};
use futures::executor::block_on;
use serde::Deserialize;
use serde_json::from_str;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use crate::{Client, Clients};
use crate::message_receive::{get_receiver, Receiver};

#[derive(Deserialize, Debug)]
pub struct TopicsRequest {
    topics: Vec<String>,
}

pub async fn client_connection(ws: WebSocket, id: String, clients: Clients, mut client: Client) {
    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    let client_rcv = UnboundedReceiverStream::new(client_rcv);
    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            eprintln!("error sending websocket msg: {}", e)
        }
    }));

    client.sender = Some(client_sender);
    clients.write().await.insert(id.clone(), client);

    println!("{} connected", id);

    while let Some(result) = client_ws_rcv.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("error receiving ws message for id: {}): {}", id.clone(), e);
                break;
            }
        };
        client_msg(&id, msg, &clients).await;
    }

    clients.write().await.remove(&id);
    println!("{} disconnected", id)
}

async fn client_msg(id: &str, msg: Message, clients: &Clients) {
    println!("received message from {}: {:?}", id, msg);

    let message = match msg.to_str() {
        Ok(v) => v,
        Err(_) => return,
    };

    if message == "ping" || message == "ping\n" {
        return;
    }

    println!("{}", message);

    match message.split_once('=') {
        Some((receiver_id, received)) => {
            match get_receiver(receiver_id) {
                Ok(v) => block_on(v.receive_msg(id, received, clients)),
                Err(_) => return,
            };
        }
        None => eprintln!("Expected <")
    }
}

pub struct TopicRequestReceiver;
#[async_trait]
impl Receiver for TopicRequestReceiver {
    async fn receive_msg(&self, id: &str, msg: &str, clients: &Clients) {
        let topics_req: TopicsRequest = match from_str(&msg) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error while passing message to topics request: {}", e);
                return;
            }
        };

        let mut locked = clients.write().await;
        if let Some(v) = locked.get_mut(id) {
            v.topics = topics_req.topics;
        }
    }
}