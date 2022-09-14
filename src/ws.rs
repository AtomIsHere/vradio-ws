use async_trait::async_trait;
use futures::{FutureExt, StreamExt};
use futures::executor::block_on;
use serde::Deserialize;
use serde_json::from_str;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use crate::{Client, Clients, Receivers};
use crate::message_receive::{Receiver};

// Structure for adding a new topic
#[derive(Deserialize, Debug)]
pub struct TopicsRequest {
    topics: Vec<String>,
}

// Handle a new connection to a websocket
pub async fn client_connection(ws: WebSocket, id: String, clients: Clients, mut client: Client, redis_client: redis::Client, receiver_manager: Receivers) {
    // Define senders
    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    // Create stream of client data
    let client_rcv = UnboundedReceiverStream::new(client_rcv);
    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            eprintln!("error sending websocket msg: {}", e)
        }
    }));

    // Wrap client sender in a optional
    client.sender = Some(client_sender);
    // Add client sender to the client struct
    clients.write().await.insert(id.clone(), client);

    println!("{} connected", id);

    // Listen for messages from client
    while let Some(result) = client_ws_rcv.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("error receiving ws message for id: {}): {}", id.clone(), e);
                break;
            }
        };

        // Run a function when a message is received
        client_msg(&id, msg, &clients, redis_client.clone(), &receiver_manager).await;
    }

    // Delete client when they disconnect
    clients.write().await.remove(&id);
    println!("{} disconnected", id)
}

// Handle a message from a client
async fn client_msg(id: &str, msg: Message, clients: &Clients, redis_client: redis::Client, receiver_manager: &Receivers) {
    println!("received message from {}: {:?}", id, msg);

    // Convert message to a reference
    let message = match msg.to_str() {
        Ok(v) => v,
        Err(_) => return,
    };

    // Check if client is just pinging
    if message == "ping" || message == "ping\n" {
        return;
    }

    // Print message to console
    println!("{}", message);

    match message.split_once('=') {
        // Split message at equals to determine the message type
        Some((receiver_id, received)) => {
            match receiver_manager.receivers.get(receiver_id) {
                // Pass message on to the receiver for the message type
                Some(v) => block_on(v.receive_msg(id, received, clients, redis_client)),
                None => return,
            };
        }
        None => eprintln!("Expected <id>=<value>")
    }
}

// Receiver for adding a listen topic
pub struct TopicRequestReceiver;
#[async_trait]
impl Receiver for TopicRequestReceiver {
    // Handle receiving a message
    async fn receive_msg(&self, id: &str, msg: &str, clients: &Clients, _redis_client: redis::Client) {
        // Create a topic from json
        let topics_req: TopicsRequest = match from_str(&msg) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error while passing message to topics request: {}", e);
                return;
            }
        };

        // Add topic to client
        let mut locked = clients.write().await;
        if let Some(v) = locked.get_mut(id) {
            v.topics = topics_req.topics;
        }
    }
}