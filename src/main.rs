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

struct TaskArgs<'a> {
    con: &'a MultiplexedConnection,
    resp_client: reqwest::Client,
    bot: Option<&'a AutoSend<Bot>>,
    dynamic_id: Option<u64>,
    live_id: Option<u64>,
    telegram_chat_id: Option<i64>,
    weibo: Option<WeiboClient>,
    weibo_profile_url: Option<String>,
    weibo_container_id: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    ctrlc::set_handler(|| {
        error_and_exit!("吃我一拳！！！");
    })
    .expect("Error setting Ctrl-C handler");

    dotenv::dotenv().ok();

    let (bot, chat_id) = init_tgbot();
    let weibo = init_weibo_client().await;
    let (dynamic_id, live_id) = init_bilibili_dyn_and_live();

    if dynamic_id.is_none() && live_id.is_none() && weibo.is_none() {
        error_and_exit!(
            "Plaset set AILURUS_DYNAMIC to check dynamic or set AILURUS_LIVE to check live status!"
        );
    }

    match init_redis_and_network_client().await {
        Ok((con, resp_client)) => {
            let task_args = TaskArgs {
                con: &con,
                resp_client,
                bot: bot.as_ref(),
                dynamic_id,
                live_id,
                telegram_chat_id: chat_id.and_then(|x| x.parse::<i64>().ok()),
                weibo,
                weibo_profile_url: std::env::var("AILURUS_PROFILE_URL").ok(),
                weibo_container_id: std::env::var("AILURUS_CONTAINER_ID").ok(),
            };

            tasker(task_args).await;
        }
        Err(e) => {
            error_and_exit!(e);
        }
    }
}

fn init_bilibili_dyn_and_live() -> (Option<u64>, Option<u64>) {
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

    (dynamic_id, live_id)
}

async fn init_weibo_client() -> Option<WeiboClient> {
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

    weibo
}

fn init_tgbot() -> (Option<AutoSend<Bot>>, Option<String>) {
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

    (bot, chat_id)
}

async fn init_redis_and_network_client() -> Result<(MultiplexedConnection, reqwest::Client)> {
    info!("Connecting redis://127.0.0.1 ...");
    let redis_client = redis::Client::open("redis://127.0.0.1/")?;
    let connect = redis_client.get_multiplexed_tokio_connection().await?;
    let resp_client = reqwest::ClientBuilder::new()
        .user_agent("User-Agent: Mozilla/5.0 (X11; AOSC OS; Linux x86_64; rv:98.0) Gecko/20100101 Firefox/98.0")
        .timeout(Duration::from_secs(30))
        .build()?;

    Ok((connect, resp_client))
}

async fn tasker(task_args: TaskArgs<'_>) {
    loop {
        let mut tasks = vec![];

        if let Some(dyn_id) = task_args.dynamic_id {
            let check_dynamic: BoxFuture<'_, Result<()>> = Box::pin(checker::check_dynamic_update(
                &task_args.con,
                dyn_id,
                &task_args.resp_client,
                task_args.bot,
                task_args.telegram_chat_id,
            ));
            tasks.push(check_dynamic);
        }

        if let Some(live_id) = task_args.live_id {
            let check_live: BoxFuture<'_, Result<()>> = Box::pin(checker::check_live_status(
                &task_args.con,
                live_id,
                &task_args.resp_client,
                task_args.bot,
                task_args.telegram_chat_id,
            ));
            tasks.push(check_live);
        }

        if let Some(ref weibo) = task_args.weibo {
            let check_weibo: BoxFuture<'_, Result<()>> = Box::pin(checker::check_weibo(
                &task_args.con,
                task_args.bot,
                weibo.clone(),
                task_args.weibo_profile_url.as_ref().unwrap().clone(),
                task_args.weibo_container_id.clone(),
                &task_args.resp_client,
                task_args.telegram_chat_id,
            ));
            tasks.push(check_weibo);
        }

        if let Err(e) = futures::future::try_join_all(tasks).await {
            error!("{}", e.to_string());
        }

        sleep(Duration::from_secs(180)).await;
    }
}
