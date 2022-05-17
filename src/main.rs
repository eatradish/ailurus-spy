use std::pin::Pin;

use anyhow::Result;
use redis::{aio::AsyncStream, Connection};
use tracing::{error, info};

mod checker;
mod dynamic;
mod live;

pub type RedisAsyncConnect = redis::aio::Connection<Pin<Box<dyn AsyncStream + Send + Sync>>>;

macro_rules! ailurus_err {
    ($e:ident) => {
        error!("{}", $e);
        std::process::exit(1);
    };
}

#[tokio::main]
async fn main() {
    let connect;
    match connect_redis().await {
        Ok(con) => {
            connect = con;
        }
        Err(e) => {
            ailurus_err!(e);
        }
    }
}

async fn connect_redis() -> Result<RedisAsyncConnect> {
    info!("Connecting redis://127.0.0.1 ...");
    let client = redis::Client::open("redis://127.0.0.1/")?;
    let connect = client.get_async_connection().await?;

    Ok(connect)
}
