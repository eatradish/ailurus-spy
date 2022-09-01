use anyhow::Result;
use futures::future::BoxFuture;
use rand::Rng;
use redis::aio::MultiplexedConnection;
use sender::{AilurusSender, Tg};
use tokio::time::{sleep, Duration};
use tracing::{error, info};
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
    senders: &'a mut [Sender],
    weibo: Option<WeiboInit>,
    bili_init: Option<BilibiliInit>,
}

pub struct Sender {
    sender: Box<dyn AilurusSender>,
    admin_chat_id: i64,
    target_chat_id: i64,
}

#[derive(Clone)]
struct BilibiliInit {
    dynamic_id: Option<u64>,
    live_id: Option<u64>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    ctrlc::set_handler(|| {
        error_and_exit!("吃我一拳！！！");
    })
    .expect("Error setting Ctrl-C handler");

    dotenv::dotenv().ok();

    let tg = init_tgbot();

    let weibo = init_weibo_client().await;

    let bilibili = init_bilibili_dyn_and_live();

    if tg.is_none() && weibo.is_none() && bilibili.is_none() {
        error_and_exit!(
            "Plaset set AILURUS_DYNAMIC to check dynamic \n
            or set AILURUS_LIVE to check live status \n
            or set AILURUS_WEIBO_USERNAME and AILURUS_WEIBO_PASSWORD and AILURUS_PROFILE_URL to check weibo!"
        );
    }

    let senders = &mut [tg.expect("Must unwrap success")][..];

    let con = unwrap_or_exit!(init_redis().await);

    let network_client = unwrap_or_exit!(init_network_client());

    let task_args = TaskArgs {
        con: &con,
        resp_client: network_client,
        senders,
        weibo,
        bili_init: bilibili,
    };

    tasker(task_args).await;
}

fn init_bilibili_dyn_and_live() -> Option<BilibiliInit> {
    let dynamic_id = std::env::var("AILURUS_DYNAMIC")
        .ok()
        .and_then(|x| x.parse::<u64>().ok());

    let live_id = std::env::var("AILURUS_LIVE")
        .ok()
        .and_then(|x| x.parse::<u64>().ok());

    Some(BilibiliInit {
        dynamic_id,
        live_id,
    })
}

pub struct WeiboInit {
    weibo: WeiboClient,
    target_profile_url: String,
}

async fn init_weibo_client() -> Option<WeiboInit> {
    let weibo = if let Ok(weibo) = login_weibo().await {
        weibo
    } else {
        return None;
    };

    let target_profile_url = unwrap_or_exit!(std::env::var("AILURUS_PROFILE_URL"));

    Some(WeiboInit {
        weibo,
        target_profile_url,
    })
}

async fn login_weibo() -> Result<WeiboClient> {
    let account = std::env::var("AILURUS_WEIBO_ACCOUNT")?;
    let password = std::env::var("AILURUS_WEIBO_PASSWORD")?;
    let weibo = weibo::WeiboClient::new()?;

    weibo.login(&account, &password).await?;

    Ok(weibo)
}

fn init_tgbot() -> Option<Sender> {
    if std::env::var("TELOXIDE_TOKEN").is_err() {
        return None;
    }

    let target_chat_id = unwrap_or_exit!(std::env::var("AILURUS_CHATID"));
    let admin_chat_id = unwrap_or_exit!(std::env::var("AILURUS_ADMIN_CHATID"));

    let target_chat_id = unwrap_or_exit!(target_chat_id.parse());
    let admin_chat_id = unwrap_or_exit!(admin_chat_id.parse());

    Some(Sender {
        sender: Box::new(Tg::new()),
        admin_chat_id,
        target_chat_id,
    })
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

        let bili_init = task_args.bili_init.clone();
        let bili_init_clone = task_args.bili_init.clone();

        let senders = &*task_args.senders;

        if let Some(Some(dyn_id)) = bili_init.map(|x| x.dynamic_id) {
            let check_dynamic: BoxFuture<'_, Result<()>> = Box::pin(checker::check_dynamic_update(
                task_args.con,
                dyn_id,
                &task_args.resp_client,
                senders,
            ));
            tasks.push(check_dynamic);
        }

        if let Some(Some(live_id)) = bili_init_clone.map(|x| x.live_id) {
            let check_live: BoxFuture<'_, Result<()>> = Box::pin(checker::check_live_status(
                task_args.con,
                live_id,
                &task_args.resp_client,
                senders,
            ));
            tasks.push(check_live);
        }

        if let Some(ref weibo) = task_args.weibo {
            let check_weibo: BoxFuture<'_, Result<()>> = Box::pin(checker::check_weibo(
                task_args.con,
                senders,
                weibo,
            ));
            tasks.push(check_weibo);
        }

        let results = futures::future::join_all(tasks).await;

        for i in results {
            if let Err(e) = i {
                error!("{}", e);
            }
        }

        sleep(Duration::from_secs(sleep_time)).await;
    }
}
