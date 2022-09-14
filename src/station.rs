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

// Tell the rust compiler that this value  can be serialized
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Station {
    // Ensure json keys match
    #[serde(rename = "id")]
    id: Uuid,
    #[serde(rename = "ownerUsername")]
    owner_username: String,

    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "mediaQueue")]
    media_queue: Vec<Media>
}

// Tell the rust compiler that this value  can be serialized
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Media {
    // Ensure json keys match
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "url")]
    url: String,
    #[serde(rename = "duration")]
    duration: i64,
    #[serde(rename = "streamingService")]
    service: StreamingService
}

// Tell the rust compiler that this value  can be serialized
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum StreamingService {
    // Ensure json keys match
    #[serde(rename = "SPOTIFY")]
    Spotify,
    #[serde(rename = "NETFLIX")]
    Netflix
}
pub async fn from_redis(id: Uuid, redis_connection: &mut Connection) -> Option<Station> {
    // Construct key for redis
    let station_key = &("Station_".to_owned() + &id.to_string());
    // Get raw value from redis
    let from_redis = match get_str(redis_connection, station_key).await {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Convert raw value into station structure
    let to_json: Station = match serde_json::from_str(&from_redis) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Return said structure
    return Some(to_json);
}

pub async fn to_redis(station: &Station, redis_connection: &mut Connection) {
    // Construct key for redis
    let station_key = &("Station_".to_owned() + &station.id.to_string());

    // Convert provided station into a json string
    if let Ok(to_json) = serde_json::to_string(&station) {
        // Create set command
        let _ = redis::cmd("SET")
            // Add key as first argument
            .arg(station_key)
            // Add json as second argument
            .arg(to_json)
            // execute command
            .query_async::<_, Option<String>>(redis_connection)
            // Wait for redis server
            .await
            .expect("Could not set station");
    } else {
        eprintln!("Could not serialize station")
    }
}

// Structure for storing stations
pub struct StationManager {
    pub stations: RwLock<HashMap<Uuid, Vec<String>>>,
    // Store time for each station
    timers: RwLock<HashMap<Uuid, Timer>>
}

// Handle join requests for stations
#[async_trait]
impl Receiver for StationManager {
    async fn receive_msg(&self, id: &str, msg: &str, clients: &Clients, redis_client: redis::Client) {
        // Establish connection to redis
        let mut redis_con: Connection =  match get_con(redis_client).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("could not connect to redis");
                return;
            },
        };

        let mut string_msg = msg.to_string().clone();

        // If message ends in a new line remove it
        if string_msg.chars().last().unwrap() == '\n' {
            string_msg.truncate(string_msg.len() - 1);
        }

        // Get the station id from join code through redis
        let result = match get_str(&mut redis_con, &("join-code:".to_owned() + &string_msg)).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("unable to find key");
                return;
            }
        };

        // Convert station id from string to UUID
        let station_id = match Uuid::parse_str(&result) {
            Ok(u) => u,
            Err(_) => {
                eprintln!("invalid uuid");
                return;
            }  
        };
        // Get station from redis
        let station = match from_redis(station_id, &mut redis_con).await {
            Some(v) => v,
            None => {
                eprintln!("Could not find station");
                return;
            }
        };

        // Add user to station
        self.join_station(station_id, id).await;

        // Check if a station is currently playing something
        if let Some(currently_playing) = station.media_queue.get(0) {
            // Get read lock on clients
            let clients_lock = clients.read().await;
            // Get client with id
            let client = match clients_lock.get(id) {
                Some(v) => v,
                None => return,
            };

            // Check if client has a sender
            if let Some(sender) = &client.sender {
                // Convert the currently playing media to json
                let as_json = match serde_json::to_string(currently_playing) {
                    Ok(v) => v,
                    Err(_) => return,
                };

                // Send the json with a key to indicate what the message is
                let _ = sender.send(Ok(Message::text("playing=".to_string() + &as_json)));
            }
        }
    }
}

impl StationManager {
    // Boilerplate for creating a new instance
    pub fn new() -> StationManager {
        StationManager {
            stations: RwLock::new(HashMap::new()),
            timers: RwLock::new(HashMap::new()),
        }
    }

    // Update loop for stations
    pub async fn update_clients(&self, clients: &Clients, redis_client: redis::Client) {
        // Obtain redis connection
        let mut redis_con: Connection = match get_con(redis_client).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("Could not connect to redis");
                return;
            }
        };

        // Get needed locks on RwLocks
        let stations_lock = self.stations.read().await;
        let mut timers_lock = self.timers.write().await;
        let clients_lock = clients.read().await;

        // Loop through stations
        for (station_id, joined_clients) in &*stations_lock {
            // Get each station from redis
            let mut station = match from_redis(station_id.clone(), &mut redis_con).await {
                Some(v) => v,
                None => {
                    eprintln!("Could not load station");
                    continue;
                }
            };

            // Check if the queue is not empty
            if station.media_queue.len() != 0 {
                // Get the time of the station
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

                // Get the currently playing media
                let currently_playing = match station.media_queue.get(0) {
                    Some(v) => v,
                    None => {
                        eprintln!("Could not find first media in queue");
                        continue;
                    },
                };
                let mut as_json: String = String::new();

                // Check if the time exceeds the duration of the currently playing media
                if currently_playing.duration <= current_time as i64 {
                    // Remove media from the queue
                    station.media_queue.remove(0);

                    // Update station in redis
                    to_redis(&station, &mut redis_con).await;

                    // Check if there is another media in the queue
                    if let Some(new_play) = station.media_queue.get(0) {
                        as_json = match serde_json::to_string(new_play) {
                            Ok(v) => v,
                            Err(_) => {
                                eprintln!("Could not serialize media");
                                continue;
                            },
                        }
                    } else {
                        // Otherwise remove timer and continue
                        timers_lock.remove(station_id);
                        continue;
                    }

                    // Remove timer
                    timers_lock.remove(station_id);
                } else if current_time == 0 {
                    // Convert currently playing media to json
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
                        // If media json is empty send the station time
                        if as_json.is_empty() {
                            let _ = sender.send(Ok(Message::text(current_time.to_string())));
                        } else {
                            // If media json is present send the currently playing media
                            let _ = sender.send(Ok(Message::text("playing=".to_string() + &as_json.clone())));
                        }
                    }
                }
            }
        }
    }

    // Add user to stations
    pub async fn join_station(&self, station_id: Uuid, client_id: &str) {
        // Get write lock on stations
        let mut stations_lock = self.stations.write().await;

        // Get mutable user list
        let mut joined_users = match stations_lock.get(&station_id) {
            Some(v) => v.clone(),
            None => Vec::new(),
        };

        // Add station to user to list
        joined_users.push(client_id.to_string());
        // Add user list to station
        stations_lock.remove(&station_id);
        stations_lock.insert(station_id, joined_users);
    }
}