use std::collections::HashMap;
use std::sync::{Arc};
use async_trait::async_trait;
use redis::aio::Connection;
use uuid::{Uuid};
use crate::{Clients};
use crate::message_receive::Receiver;
use crate::redis_direct::{get_con, get_str};
use serde::Deserialize;
use tokio::sync::RwLock;

#[derive(Debug, Deserialize, PartialEq)]
pub struct Station {
    #[serde(rename(deserialize = "id"))]
    id: Uuid,
    #[serde(rename(deserialize = "ownerUsername"))]
    owner_username: String,

    #[serde(rename(deserialize = "name"))]
    name: String,
    #[serde(rename(deserialize = "mediaQueue"))]
    media_queue: Vec<Media>
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Media {
    #[serde(rename(deserialize = "name"))]
    name: String,
    #[serde(rename(deserialize = "url"))]
    url: String,
    #[serde(rename(deserialize = "duration"))]
    duration: i64,
    #[serde(rename(deserialize = "streamingService"))]
    service: StreamingService
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum StreamingService {
    #[serde(rename(deserialize = "SPOTIFY"))]
    Spotify,
    #[serde(rename(deserialize = "NETFLIX"))]
    Netflix
}
pub async fn from_redis(id: Uuid, redis_connection: &mut Connection) -> Option<Station> {
    let station_key = &("Station_".to_owned() + &id.to_string());
    let from_redis = match get_str(redis_connection, station_key).await {
        Ok(v) => v,
        Err(_) => return None,
    };

    let to_json: Station = match serde_json::from_str(&from_redis) {
        Ok(v) => v,
        Err(_) => return None,
    };

    return Some(to_json);
}

pub struct StationManager {
    pub(crate) stations: Arc<RwLock<HashMap<Uuid, Station>>>
}

#[async_trait]
impl Receiver for StationManager {
    async fn receive_msg(&self, _id: &str, msg: &str, _clients: &Clients, redis_client: redis::Client) {
        let mut redis_con: Connection =  match get_con(redis_client).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("could not connect to redis");
                return;
            },
        };

        let mut string_msg = msg.to_string().clone();
        string_msg.truncate(string_msg.len() - 1);

        let result = match get_str(&mut redis_con, &("join-code:".to_owned() + &string_msg)).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("unable to find key");
                return;
            }
        };

        let id = match Uuid::parse_str(&result) {
            Ok(u) => u,
            Err(_) => {
                eprintln!("invalid uuid");
                return;
            }
        };

        let station: Station = match from_redis(id, &mut redis_con).await {
            Some(v) => v,
            None => {
                eprintln!("invalid redis hash");
                return;
            }
        };

    }
}