use std::pin::Pin;

use anyhow::Result;
use redis::aio::{AsyncStream, MultiplexedConnection};
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
        Ok((con, resp_client)) => {
            tasker(&con, &resp_client, &bot).await;
        }
        Err(e) => {
            error!("{}", e);
            std::process::exit(1);
        }
    }
}

async fn init() -> Result<(MultiplexedConnection, reqwest::Client)> {
    info!("Connecting redis://127.0.0.1 ...");
    let redis_client = redis::Client::open("redis://127.0.0.1/")?;
    let connect = redis_client.get_multiplexed_tokio_connection().await?;
    let resp_client = reqwest::ClientBuilder::new()
        .user_agent("User-Agent: Mozilla/5.0 (X11; AOSC OS; Linux x86_64; rv:98.0) Gecko/20100101 Firefox/98.0")
        .build()?;

    Ok((connect, resp_client))
}

async fn tasker(con: &MultiplexedConnection, resp_client: &reqwest::Client, bot: &AutoSend<Bot>) {
    loop {
        if let Err(e) = tokio::try_join!(
            checker::check_dynamic_update(con, 1501380958, resp_client, bot),
            checker::check_live_status(con, 22746343, resp_client, bot),
        ) {
            error!("{}", e);
        }
        sleep(Duration::from_secs(180)).await;
    }
}
