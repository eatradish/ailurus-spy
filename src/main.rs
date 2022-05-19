use anyhow::Result;
use redis::aio::MultiplexedConnection;
use teloxide::prelude::*;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

mod checker;
mod dynamic;
mod live;

macro_rules! error_and_exit {
    ($e:expr) => {
        error!("{}", $e);
        std::process::exit(1);
    };
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    ctrlc::set_handler(|| {
        error_and_exit!("吃我一拳！！！");
    })
    .expect("Error setting Ctrl-C handler");
    dotenv::dotenv().ok();
    let bot = Bot::from_env().auto_send();
    let mut args = vec![];
    for i in &["AILURUS_DYNAMIC", "AILURUS_LIVE"] {
        if let Ok(id) = std::env::var(i) {
            if let Ok(id) = id.parse::<u64>() {
                args.push(Some(id));
            } else {
                error_and_exit!(format!("var {} is not a number!", i));
            }
        } else {
            args.push(None)
        };
    }
    let dynamic_id = args[0];
    let live_id = args[1];
    if dynamic_id.is_none() && live_id.is_none() {
        error_and_exit!(
            "Plaset set AILURUS_DYNAMIC to check dynamic or set AILURUS_LIVE to check live status!"
        );
    }
    match init().await {
        Ok((con, resp_client)) => {
            tasker(&con, &resp_client, &bot, dynamic_id, live_id).await;
        }
        Err(e) => {
            error_and_exit!(e);
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

async fn tasker(
    con: &MultiplexedConnection,
    resp_client: &reqwest::Client,
    bot: &AutoSend<Bot>,
    dynamic_id: Option<u64>,
    live_id: Option<u64>,
) {
    if let Some(dyn_id) = dynamic_id {
        if let Some(live_id) = live_id {
            loop {
                if let Err(e) = tokio::try_join!(
                    checker::check_dynamic_update(con, dyn_id, resp_client, bot),
                    checker::check_live_status(con, live_id, resp_client, bot),
                ) {
                    error!("{}", e);
                }
                sleep(Duration::from_secs(180)).await;
            }
        } else {
            loop {
                if let Err(e) =
                    tokio::try_join!(checker::check_dynamic_update(con, dyn_id, resp_client, bot))
                {
                    error!("{}", e);
                }
                sleep(Duration::from_secs(180)).await;
            }
        }
    } else if let Some(live_id) = live_id {
        loop {
            if let Err(e) =
                tokio::try_join!(checker::check_live_status(con, live_id, resp_client, bot))
            {
                error!("{}", e);
            }
            sleep(Duration::from_secs(180)).await;
        }
    }
}
