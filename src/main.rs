use std::pin::Pin;

use anyhow::Result;
use redis::aio::AsyncStream;
use teloxide::prelude::*;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

mod checker;
mod dynamic;
mod live;

pub type RedisAsyncConnect = redis::aio::Connection<Pin<Box<dyn AsyncStream + Send + Sync>>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();
    let bot = Bot::from_env().auto_send();

    match init().await {
        Ok((mut con, resp_client)) => {
            tasker(&mut con, &resp_client, &bot).await;
        }
        Err(e) => {
            error!("{}", e);
            std::process::exit(1);
        }
    }
}

async fn init() -> Result<(RedisAsyncConnect, reqwest::Client)> {
    info!("Connecting redis://127.0.0.1 ...");
    let redis_client = redis::Client::open("redis://127.0.0.1/")?;
    let connect = redis_client.get_async_connection().await?;
    let resp_client = reqwest::ClientBuilder::new()
        .user_agent("User-Agent: Mozilla/5.0 (X11; AOSC OS; Linux x86_64; rv:98.0) Gecko/20100101 Firefox/98.0")
        .build()?;

    Ok((connect, resp_client))
}

async fn tasker(con: &mut RedisAsyncConnect, resp_client: &reqwest::Client, bot: &AutoSend<Bot>) {
    loop {
        if let Err(e) = checker::check_dynamic_update(con, 1501380958, resp_client, bot).await {
            error!("{}", e);
        }
        sleep(Duration::from_secs(180)).await;
    }
}
