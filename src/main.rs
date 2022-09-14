use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc};
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use warp::{Filter, Rejection};
use warp::ws::Message;
use thiserror::Error;
use tokio::time;
use crate::message_receive::{Receiver, ReceiverManager};
use crate::station::StationManager;
use crate::ws::TopicRequestReceiver;

mod handler;
mod ws;
mod message_receive;
mod redis_direct;
mod station;
mod timer;

type Result<T> = std::result::Result<T, Rejection>;
type Clients = Arc<RwLock<HashMap<String, Client>>>;
type Receivers = Arc<ReceiverManager>;

const REDIS_CON_STRING: &str = "redis://127.0.0.1/";

#[derive(Debug, Clone)]
pub struct Client {
    pub user_id: usize,
    pub topics: Vec<String>,
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>
}

#[tokio::main]
async fn main() {
    // Register clients list
    let clients: Clients = Arc::new(RwLock::new(HashMap::new()));

    // Create client
    let redis_client = redis::Client::open(REDIS_CON_STRING).expect("can create redis client");

    // Create map of receivers
    let mut receiver_map: HashMap<String, Arc<dyn Receiver>> = HashMap::new();
    // Create the station manager list
    let stations = Arc::new(StationManager::new());
    // Clone the arc to allow safe moving between threads
    let stations_clone = stations.clone();

    // Add the receivers
    receiver_map.insert("topic_request".to_string(), Arc::new(TopicRequestReceiver {}));
    receiver_map.insert("join_station".to_string(), stations);

    // Wrap receivers in an arc to allow safe movement between threads
    let receiver_manager: Receivers = Arc::new(ReceiverManager {receivers: receiver_map});

    // Add health handler to ensure websocket server is started
    let health_route = warp::path!("health").and_then(handler::health_handler);


    // Add route to register and delete clients
    let register = warp::path("register");
    let register_routes = register
        .and(warp::post())
        .and(warp::body::json())
        .and(with_clients(clients.clone()))
        .and_then(handler::register_handler)
        .or(register
            .and(warp::delete())
            .and(warp::path::param())
            .and(with_clients(clients.clone()))
            .and_then(handler::unregister_handler));

    // Add route to publish a message to the websocket server
    let publish = warp::path!("publish")
        .and(warp::body::json())
        .and(with_clients(clients.clone()))
        .and_then(handler::publish_handler);

    // Add route to join the web socket
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(warp::path::param())
        .and(with_clients(clients.clone()))
        .and(with_redis_client(redis_client))
        .and(with_receiver_manager(receiver_manager))
        .and_then(handler::ws_handler);

    // Register all routes
    let routes = health_route
        .or(register_routes)
        .or(ws_route)
        .or(publish)
        // Effectively disable CORS
        .with(warp::cors().allow_any_origin().allow_headers(vec!["content-type"]).allow_methods(vec!["POST", "GET"]));

    // Clone clients to allow it to move to the update task
    let clients_clone = clients.clone();

    // Spawn station update task
    tokio::spawn(async move {
        // Create interval to run task every 30 seconds
        let mut interval = time::interval(Duration::from_secs(30));
        // Create a new redis client
        let new_client = redis::Client::open(REDIS_CON_STRING).expect("can create redis client");

        loop {
            // Ensure interval is reached
            interval.tick().await;
            // Update all clients on station status
            stations_clone.update_clients(&clients_clone, new_client.clone()).await;
        }
    });

    warp::serve(routes).run(([127,0,0,1], 8000)).await
}

fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = Infallible> + Clone {
    warp::any().map(move || clients.clone())
}

fn with_redis_client(client: redis::Client) -> impl Filter<Extract = (redis::Client,), Error = Infallible> + Clone {
    warp::any().map(move || client.clone())
}

fn with_receiver_manager(receivers: Receivers) -> impl Filter<Extract = (Receivers,), Error = Infallible> + Clone {
    warp::any().map(move || receivers.clone())
}

#[derive(Error, Debug)]
pub enum RedisError {
    #[error("direct redis error: {0}")]
    DirectError(#[from] DirectError)
}

#[derive(Error, Debug)]
pub enum DirectError {
    #[error("error paring string from redis_direct result: {0}")]
    RedisTypeError(redis::RedisError),
    #[error("error executing redis_direct command: {0}")]
    RedisCMDError(redis::RedisError),
    #[error("error creating Redis client: {0}")]
    RedisClientError(redis::RedisError),
}
