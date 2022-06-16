use anyhow::Result;
use futures::future::BoxFuture;
use redis::aio::MultiplexedConnection;
use teloxide::prelude::*;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use weibo::WeiboClient;

mod checker;
mod dynamic;
mod live;
mod sender;
mod weibo;

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

    let (bot, chat_id) = if std::env::var("TELOXIDE_TOKEN").is_ok() {
        if let Ok(v) = std::env::var("AILURUS_CHATID") {
            (Some(Bot::from_env().auto_send()), Some(v))
        } else {
            error_and_exit!("TELOXIDE_TOKEN is set but AILURUS_CHATID not to set!");
        }
    } else {
        warn!("TELOXIDE_TOKEN variable is not set, if you need Telegram bot to send messages, please set this variable as your telegram bot token");
        (None, None)
    };

    let weibo = if let Ok(account) = std::env::var("AILURUS_WEIBO_ACCOUNT") {
        if let Ok(password) = std::env::var("AILURUS_WEIBO_PASSWORD") {
            let weibo = weibo::WeiboClient::new();
            if let Err(e) = weibo {
                error_and_exit!(e);
            }

            let weibo = weibo.unwrap();
            if let Err(e) = weibo.login(&account, &password).await {
                error_and_exit!(e);
            }

            Some(weibo)
        } else {
            None
        }
    } else if std::env::var("AILURUS_PROFILE_URL").is_ok()
        && std::env::var("AILURUS_CONTAINER_ID").is_ok()
    {
        let weibo = weibo::WeiboClient::new();

        weibo.ok()
    } else {
        None
    };

    if std::env::var("AILURUS_PROFILE_URL").is_ok()
        && weibo.is_none()
        && std::env::var("AILURUS_CONTAINER_ID").is_err()
    {
        error_and_exit!("You have no login to weibo or set container id!");
    }

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
            tasker(
                &con,
                resp_client,
                bot.as_ref(),
                dynamic_id,
                live_id,
                chat_id.and_then(|x| x.parse::<i64>().ok()),
                weibo,
                std::env::var("AILURUS_PROFILE_URL").ok(),
                std::env::var("AILURUS_CONTAINER_ID").ok(),
            )
            .await;
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
        .timeout(Duration::from_secs(30))
        .build()?;

    Ok((connect, resp_client))
}

async fn tasker(
    con: &MultiplexedConnection,
    resp_client: reqwest::Client,
    bot: Option<&AutoSend<Bot>>,
    dynamic_id: Option<u64>,
    live_id: Option<u64>,
    telegram_chat_id: Option<i64>,
    weibo: Option<WeiboClient>,
    weibo_profile_url: Option<String>,
    weibo_container_id: Option<String>,
) {
    loop {
        let mut tasks = vec![];

        let con = con.clone();
        let con_2 = con.clone();
        let con_3 = con.clone();

        if let Some(dyn_id) = dynamic_id {
            let check_dynamic: BoxFuture<'_, Result<()>> = Box::pin(checker::check_dynamic_update(
                con,
                dyn_id,
                &resp_client,
                bot,
                telegram_chat_id,
            ));
            tasks.push(check_dynamic);
        }

        if let Some(live_id) = live_id {
            let check_live: BoxFuture<'_, Result<()>> = Box::pin(checker::check_live_status(
                con_2,
                live_id,
                &resp_client,
                bot,
                telegram_chat_id,
            ));
            tasks.push(check_live);
        }

        if let Some(ref weibo) = weibo {
            let check_weibo: BoxFuture<'_, Result<()>> = Box::pin(checker::check_weibo(
                con_3,
                bot,
                weibo.clone(),
                weibo_profile_url.as_ref().unwrap().clone(),
                weibo_container_id.clone(),
                &resp_client,
                telegram_chat_id,
            ));
            tasks.push(check_weibo);
        }

        if let Err(e) = tokio::try_join!(futures::future::try_join_all(tasks)) {
            error!("{}", e.to_string());
        }

        sleep(Duration::from_secs(180)).await;
    }
}
