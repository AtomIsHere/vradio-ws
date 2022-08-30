use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc};
use tokio::sync::{mpsc, RwLock};
use warp::{Filter, Rejection};
use warp::ws::Message;
use thiserror::Error;

mod handler;
mod ws;
mod message_receive;
mod redis_direct;
mod station;

type Result<T> = std::result::Result<T, Rejection>;
type Clients = Arc<RwLock<HashMap<String, Client>>>;

const REDIS_CON_STRING: &str = "redis://127.0.0.1/";

#[derive(Debug, Clone)]
pub struct Client {
    pub user_id: usize,
    pub topics: Vec<String>,
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>
}

#[tokio::main]
async fn main() {
    let clients: Clients = Arc::new(RwLock::new(HashMap::new()));

    let redis_client = redis::Client::open(REDIS_CON_STRING).expect("can create redis client");

    let health_route = warp::path!("health").and_then(handler::health_handler);

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

    let publish = warp::path!("publish")
        .and(warp::body::json())
        .and(with_clients(clients.clone()))
        .and_then(handler::publish_handler);

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(warp::path::param())
        .and(with_clients(clients.clone()))
        .and(with_redis_client(redis_client))
        .and_then(handler::ws_handler);

    let routes = health_route
        .or(register_routes)
        .or(ws_route)
        .or(publish)
        .with(warp::cors().allow_any_origin());

    warp::serve(routes).run(([127,0,0,1], 8000)).await
}

fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = Infallible> + Clone {
    warp::any().map(move || clients.clone())
}

fn with_redis_client(client: redis::Client) -> impl Filter<Extract = (redis::Client,), Error = Infallible> + Clone {
    warp::any().map(move || client.clone())
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
