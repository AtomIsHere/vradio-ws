use redis::Client;
use uuid::Uuid;
use crate::Clients;
use crate::message_receive::Receiver;

struct Station {
    id: Uuid,
    owner_username: String,

    name: String,
    media_queue: Vec<Media>
}

struct Media {
    name: String,
    url: String,
    duration: i64,
    service: StreamingService
}

enum StreamingService {
    Spotify,
    Netflix
}

struct JoinCodeReceiver;
impl Receiver for JoinCodeReceiver {
    async fn receive_msg(&self, id: &str, msg: &str, clients: &Clients, redis_client: Client) {
        todo!()
    }
}