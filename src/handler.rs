use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warp::http::StatusCode;
use warp::Reply;
use warp::reply::{json};
use warp::ws::Message;
use crate::{Client, Clients, Receivers, Result, ws};


#[derive(Deserialize, Debug)]
pub struct RegisterRequest {
    user_id: usize,
}

#[derive(Serialize, Debug)]
pub struct RegisterResponse {
    url: String,
}

#[derive(Deserialize, Debug)]
pub struct Event {
    topic: String,
    user_id: Option<usize>,
    message: String,
}


pub async fn publish_handler(body: Event, clients: Clients) -> Result<impl Reply> {
    clients
        // Obtain read lock of client
        .read()
        .await
        // Create an iterator
        .iter()
        // Ensure user id matches with provided input
        .filter(|(_, client)| match body.user_id {
            Some(v) => client.user_id == v,
            None => true
        })
        // Filter out clients without provided topics
        .filter(|(_, client)| client.topics.contains(&body.topic))
        .for_each(|(_, client)| {
            if let Some(sender) = &client.sender {
                // Send a message to clients which met filters
                let _ = sender.send(Ok(Message::text(body.message.clone())));
            }
        });

    Ok(StatusCode::OK)
}

pub async fn register_handler(body: RegisterRequest, clients: Clients) -> Result<impl Reply> {
    let user_id = body.user_id;
    // Create UUID for connection
    let uuid = Uuid::new_v4().as_simple().to_string();

    // Add client ot client list
    register_client(uuid.clone(), user_id, clients).await;
    // Return join link to client
    Ok(json(&RegisterResponse {
        url: format!("ws://127.0.0.1:8000/ws/{}", uuid)
    }))
}

async fn register_client(id: String, user_id: usize, clients: Clients) {
    // Get client lock and insert a client
    clients.write().await.insert(
        // Make the connection uuid the key
        id,
        Client {
            user_id,
            // Create a list with a default value
            topics: vec![String::from("default")],
            // Placeholder value for sender until client connects to websocket
            sender: None,
        },
    );
}

pub async fn unregister_handler(id: String, clients: Clients) -> Result<impl Reply> {
    // Remove client from list
    clients.write().await.remove(&id);
    // Return a 200 status code to inform the client it was successful
    Ok(StatusCode::OK)
}

pub async fn ws_handler(ws: warp::ws::Ws, id: String, clients: Clients, redis_client: redis::Client, receiver_manger: Receivers) -> Result<impl Reply> {
    // Get the client
    let client = clients.read().await.get(&id).cloned();
    match client {
        // Attach a sender to client when the client joins the websocket
        Some(c) => Ok(ws.on_upgrade(move |socket| ws::client_connection(socket, id, clients, c, redis_client, receiver_manger))),
        // Return an error if it is a failure
        None => Err(warp::reject::not_found()),
    }
}

pub async fn health_handler() -> Result<impl Reply> {
    Ok(StatusCode::OK)
}