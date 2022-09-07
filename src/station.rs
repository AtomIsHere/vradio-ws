use std::collections::HashMap;
use async_trait::async_trait;
use redis::aio::Connection;
use uuid::{Uuid};
use crate::{Clients};
use crate::message_receive::Receiver;
use crate::redis_direct::{get_con, get_str};
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;
use warp::ws::Message;
use crate::timer::Timer;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Station {
    #[serde(rename = "id")]
    id: Uuid,
    #[serde(rename = "ownerUsername")]
    owner_username: String,

    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "mediaQueue")]
    media_queue: Vec<Media>
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Media {
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "url")]
    url: String,
    #[serde(rename = "duration")]
    duration: i64,
    #[serde(rename = "streamingService")]
    service: StreamingService
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum StreamingService {
    #[serde(rename = "SPOTIFY")]
    Spotify,
    #[serde(rename = "NETFLIX")]
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

pub async fn to_redis(station: &Station, redis_connection: &mut Connection) {
    let station_key = &("Station_".to_owned() + &station.id.to_string());

    if let Ok(to_json) = serde_json::to_string(&station) {
        let _ = redis::cmd("SET")
            .arg(station_key)
            .arg(to_json)
            .query_async::<_, Option<String>>(redis_connection)
            .await
            .expect("Could not set station");
    } else {
        eprintln!("Could not serialize station")
    }
}

pub struct StationManager {
    pub stations: RwLock<HashMap<Uuid, Vec<String>>>,
    timers: RwLock<HashMap<Uuid, Timer>>
}

#[async_trait]
impl Receiver for StationManager {
    async fn receive_msg(&self, id: &str, msg: &str, clients: &Clients, redis_client: redis::Client) {
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

        let station_id = match Uuid::parse_str(&result) {
            Ok(u) => u,
            Err(_) => {
                eprintln!("invalid uuid");
                return;
            }  
        };
        let station = match from_redis(station_id, &mut redis_con).await {
            Some(v) => v,
            None => {
                eprintln!("Could not find station");
                return;
            }
        };

        self.join_station(station_id, id).await;

        if let Some(currently_playing) = station.media_queue.get(0) {
            let clients_lock = clients.read().await;
            let client = match clients_lock.get(id) {
                Some(v) => v,
                None => return,
            };

            if let Some(sender) = &client.sender {
                let as_json = match serde_json::to_string(currently_playing) {
                    Ok(v) => v,
                    Err(_) => return,
                };

                let _ = sender.send(Ok(Message::text("playing=".to_string() + &as_json)));
            }
        }
    }
}

impl StationManager {
    pub fn new() -> StationManager {
        StationManager {
            stations: RwLock::new(HashMap::new()),
            timers: RwLock::new(HashMap::new()),
        }
    }

    pub async fn update_clients(&self, clients: &Clients, redis_client: redis::Client) {
        let mut redis_con: Connection = match get_con(redis_client).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("Could not connect to redis");
                return;
            }
        };

        let stations_lock = self.stations.read().await;
        let mut timers_lock = self.timers.write().await;
        let clients_lock = clients.read().await;

        for (station_id, joined_clients) in &*stations_lock {
            let mut station = match from_redis(station_id.clone(), &mut redis_con).await {
                Some(v) => v,
                None => {
                    eprintln!("Could not load station");
                    continue;
                }
            };

            if station.media_queue.len() != 0 {
                let current_time = match timers_lock.get(station_id) {
                    Some(v) => match v.get_time() {
                        Ok(v) => v,
                        Err(_) => {
                            eprintln!("Could not get time");

                            1
                        },
                    },
                    None => {
                        let new_timer = Timer::new();
                        timers_lock.insert(station_id.clone(), new_timer);

                        0
                    },
                };

                let currently_playing = match station.media_queue.get(0) {
                    Some(v) => v,
                    None => {
                        eprintln!("Could not find first media in queue");
                        continue;
                    },
                };
                let mut as_json: String = String::new();

                if currently_playing.duration <= current_time as i64 {
                    station.media_queue.remove(0);

                    to_redis(&station, &mut redis_con).await;

                    if let Some(new_play) = station.media_queue.get(0) {
                        as_json = match serde_json::to_string(new_play) {
                            Ok(v) => v,
                            Err(_) => {
                                eprintln!("Could not serialize media");
                                continue;
                            },
                        }
                    } else {
                        timers_lock.remove(station_id);
                        continue;
                    }

                    timers_lock.remove(station_id);
                } else if current_time == 0 {
                    as_json = match serde_json::to_string(currently_playing) {
                        Ok(v) => v,
                        Err(_) => {
                            eprintln!("Could not serialize media");
                            continue;
                        },
                    }
                }

                for (_, client) in clients_lock.iter()
                    .filter(|&(k, _)| joined_clients.contains(k)) {
                    if let Some(sender) = &client.sender {
                        if as_json.is_empty() {
                            let _ = sender.send(Ok(Message::text(current_time.to_string())));
                        } else {
                            let _ = sender.send(Ok(Message::text("playing=".to_string() + &as_json.clone())));
                        }
                    }
                }
            }
        }
    }

    pub async fn join_station(&self, station_id: Uuid, client_id: &str) {
        let mut stations_lock = self.stations.write().await;

        let mut joined_users = match stations_lock.get(&station_id) {
            Some(v) => v.clone(),
            None => Vec::new(),
        };

        joined_users.push(client_id.to_string());
        stations_lock.remove(&station_id);
        stations_lock.insert(station_id, joined_users);
    }
}