use redis::aio::Connection;
use redis::{AsyncCommands, FromRedisValue};
use crate::DirectError::{RedisClientError, RedisCMDError, RedisTypeError};
use crate::RedisError;

type Result<T> = std::result::Result<T, RedisError>;

// Establish connection to redis server with client struct
pub async fn get_con(client: redis::Client) -> Result<Connection> {
    client.get_async_connection().await.map_err(|e| RedisClientError(e).into())
}

// Get a string from redis
pub async fn get_str(con: &mut Connection, key: &str) -> Result<String> {
    let value = con.get(key).await.map_err(RedisCMDError)?;
    FromRedisValue::from_redis_value(&value).map_err(|e| RedisTypeError(e).into())
}