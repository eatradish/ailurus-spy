use anyhow::{bail, Result};
use futures::future::BoxFuture;
use rand::Rng;
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

macro_rules! unwrap_or_exit {
    ($f:expr) => {
        match $f {
            Ok(v) => v,
            Err(e) => {
                error_and_exit!(e);
            }
        }
    };
}

struct TaskArgs<'a> {
    con: &'a MultiplexedConnection,
    resp_client: reqwest::Client,
    bot: Option<&'a AutoSend<Bot>>,
    dynamic_id: Option<u64>,
    live_id: Option<u64>,
    telegram_chat_id: Option<i64>,
    weibo: Option<&'a WeiboClient>,
    weibo_profile_url: Option<String>,
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

    let weibo_and_profile_url = unwrap_or_exit!(init_weibo_client().await);

    let (weibo, profile_url) = weibo_and_profile_url;

    let (dynamic_id, live_id) = init_bilibili_dyn_and_live();

    if dynamic_id.is_none() && live_id.is_none() && weibo.is_none() {
        error_and_exit!(
            "Plaset set AILURUS_DYNAMIC to check dynamic \n
            or set AILURUS_LIVE to check live status \n
            or set AILURUS_WEIBO_USERNAME and AILURUS_WEIBO_PASSWORD and AILURUS_PROFILE_URL to check weibo!"
        );
    }

    let con = unwrap_or_exit!(init_redis().await);

    let network_client = unwrap_or_exit!(init_network_client());

    let task_args = TaskArgs {
        con: &con,
        resp_client: network_client,
        bot: bot.as_ref(),
        dynamic_id,
        live_id,
        telegram_chat_id: chat_id.and_then(|x| x.parse::<i64>().ok()),
        weibo: weibo.as_ref(),
        weibo_profile_url: profile_url,
    };

    tasker(task_args).await;
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

async fn init_weibo_client() -> Result<(Option<WeiboClient>, Option<String>)> {
    let account_and_password = if let Ok(account) = std::env::var("AILURUS_WEIBO_ACCOUNT") {
        if let Ok(password) = std::env::var("AILURUS_WEIBO_PASSWORD") {
            Some((account, password))
        } else {
            None
        }
    } else {
        None
    };

    let weibo = if let Some((account, password)) = account_and_password {
        login_weibo(&account, &password).await.ok()
    } else {
        None
    };

    let profile_url = std::env::var("AILURUS_PROFILE_URL").ok();

    if weibo.is_none() && profile_url.is_some() {
        bail!(
            "AILURUS_PROFILE_URL is set but weibo account info not to set!\n
        Please set AILURUS_WEIBO_USERNAME and AILURUS_WEIBO_PASSWORD!"
        );
    }

    if weibo.is_some() && profile_url.is_none() {
        bail!("Weibo account info is set but profile url not to set!\nPlease set AILURUS_PROFILE_URL!");
    }

    Ok((weibo, profile_url))
}

async fn login_weibo(account: &str, password: &str) -> Result<WeiboClient> {
    let weibo = weibo::WeiboClient::new()?;
    weibo.login(account, password).await?;

    Ok(weibo)
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

async fn init_redis() -> Result<MultiplexedConnection> {
    let redis_client = loop {
        info!("Try connecting redis://127.0.0.1 ...");
        if let Ok(client) = redis::Client::open("redis://127.0.0.1/") {
            break client;
        } else {
            error!("Connect failed! Please check your redis server is opened?")
        }
    };
    let connect = redis_client.get_multiplexed_tokio_connection().await?;

    Ok(connect)
}

fn init_network_client() -> Result<reqwest::Client> {
    let resp_client = reqwest::ClientBuilder::new()
        .user_agent("User-Agent: Mozilla/5.0 (X11; AOSC OS; Linux x86_64; rv:98.0) Gecko/20100101 Firefox/98.0")
        .timeout(Duration::from_secs(30))
        .build()?;

    Ok(resp_client)
}

async fn tasker(task_args: TaskArgs<'_>) {
    let mut rng = rand::thread_rng();

    loop {
        let mut tasks = vec![];

        let sleep_time = rng.gen_range(60..=180);

        if let Some(dyn_id) = task_args.dynamic_id {
            let check_dynamic: BoxFuture<'_, Result<()>> = Box::pin(checker::check_dynamic_update(
                task_args.con,
                dyn_id,
                &task_args.resp_client,
                task_args.bot,
                task_args.telegram_chat_id,
            ));
            tasks.push(check_dynamic);
        }

        if let Some(live_id) = task_args.live_id {
            let check_live: BoxFuture<'_, Result<()>> = Box::pin(checker::check_live_status(
                task_args.con,
                live_id,
                &task_args.resp_client,
                task_args.bot,
                task_args.telegram_chat_id,
            ));
            tasks.push(check_live);
        }

        if let Some(weibo) = task_args.weibo {
            let check_weibo: BoxFuture<'_, Result<()>> = Box::pin(checker::check_weibo(
                task_args.con,
                task_args.bot,
                weibo,
                task_args.weibo_profile_url.as_ref().unwrap().clone(),
                &task_args.resp_client,
                task_args.telegram_chat_id,
            ));
            tasks.push(check_weibo);
        }

        if let Err(e) = futures::future::try_join_all(tasks).await {
            error!("{}", e.to_string());
        }

        sleep(Duration::from_secs(sleep_time)).await;
    }
}
