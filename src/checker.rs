use anyhow::{anyhow, Result};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use reqwest::Client;
use teloxide::{adaptors::AutoSend, Bot};
use time::{format_description, macros::offset, OffsetDateTime};
use tracing::{error, info};

use crate::{
    dynamic, live,
    sender::{self, TelegramSend},
    weibo::WeiboClient,
};

pub async fn check_dynamic_update(
    con: &MultiplexedConnection,
    uid: u64,
    client: &Client,
    bot: Option<&AutoSend<Bot>>,
    telegram_chat_id: Option<i64>,
) -> Result<()> {
    let mut con = con.clone();
    info!("checking {} dynamic update ...", uid);
    let key = format!("dynamic-{}", uid);
    let key2 = format!("dynamic-{}-updated-id", uid);
    let dynamic = dynamic::get_ailurus_dynamic(uid, client).await?;
    let v: Result<u64> = con.get(&key).await.map_err(|e| anyhow!("{}", e));
    if v.is_err() {
        info!("Creating new spy {}...", &key);
        con.set(
            &key,
            dynamic
                .first()
                .ok_or_else(|| anyhow!("{} dynamic is empty!", uid))?
                .timestamp,
        )
        .await?;
    }
    let mut is_update = false;
    let mut telegram_sends = vec![];
    if let Ok(t) = v {
        for i in &dynamic {
            if i.timestamp > t {
                is_update = true;
                let name = if let Some(name) = i.user.clone() {
                    name
                } else {
                    format!("{}", uid)
                };
                let desc = if let Some(desc) = i.description.clone() {
                    desc
                } else {
                    "None".to_string()
                };
                info!("用户 {} 有新动态！内容：{}", name, desc);
                let date = timestamp_to_date(i.timestamp)?;
                let url = format!("https://t.bilibili.com/{}", i.dynamic_id);
                let s = format!(
                    "<b>{} 有新动态啦！</b>\n{}\n{}\n\n{}",
                    name, date, desc, url
                );
                let group = if let Some(picture) = i.picture.clone() {
                    let mut group = Vec::new();
                    for i in picture {
                        if let Some(img) = i.img_src {
                            group.push(img);
                        }
                    }
                    Some(group)
                } else {
                    None
                };
                telegram_sends.push(TelegramSend {
                    msg: s,
                    photos: group,
                    photo: None,
                });
                con.set(&key2, i.dynamic_id).await?;
            }
        }
        check_and_send(bot, telegram_chat_id, telegram_sends, client).await?;
        if is_update {
            info!("Update {} timestamp", key);
            con.set(&key, dynamic[0].timestamp).await?;
        }
    } else {
        error!("{}", v.unwrap_err());
    }

    Ok(())
}

pub async fn check_live_status(
    con: &MultiplexedConnection,
    room_id: u64,
    client: &Client,
    bot: Option<&AutoSend<Bot>>,
    telegram_chat_id: Option<i64>,
) -> Result<()> {
    let mut con = con.clone();
    info!("checking room {} live status update ...", room_id);
    let key = format!("live-{}-status", room_id);
    let live = live::get_live_status(room_id, client).await?;
    let db_live_status: Result<bool> = con.get(&key).await.map_err(|e| anyhow!(e));
    let ls = live.live_status;
    let date = live.live_time;
    if db_live_status.is_err() {
        con.set(&key, ls == 1).await?;
    } else {
        let db_live_status = db_live_status.unwrap();
        if !db_live_status && ls == 1 {
            let s = format!(
                "<b>{} 开播啦！</b>\n{}\n{}\n\n{}",
                live.uname,
                date,
                live.title,
                format_args!("https://live.bilibili.com/{}", live.room_id)
            );
            info!("{}", s);
            let telegram_sends = vec![TelegramSend {
                msg: s,
                photos: None,
                photo: Some(live.user_cover),
            }];
            check_and_send(bot, telegram_chat_id, telegram_sends, client).await?;
            con.set(key, true).await?;
        } else if db_live_status && ls == 1 {
            con.set(key, true).await?;
        } else if ls != 1 {
            con.set(key, false).await?;
        }
    }

    Ok(())
}

pub async fn check_weibo(
    con: &MultiplexedConnection,
    bot: Option<&AutoSend<Bot>>,
    weibo: &WeiboClient,
    profile_url: String,
    client: &Client,
    telegram_chat_id: Option<i64>,
) -> Result<()> {
    info!("Checking {} weibo ...", profile_url);
    let mut con = con.clone();

    let key = format!("weibo-{}", profile_url);
    let key_container_id = format!("weibo-{}-containerid", profile_url);
    let v: Result<String> = con.get(&key).await.map_err(|e| anyhow!("{}", e));
    let containerid: Result<String> = con
        .get(&key_container_id)
        .await
        .map_err(|e| anyhow!("{}", e));

    let (ailurus, container_id) = weibo.get_ailurus(&profile_url, containerid.ok()).await?;
    con.set(&key_container_id, container_id).await?;

    let data = ailurus
        .data
        .cards
        .ok_or_else(|| anyhow!("Can not get weibo index!"))?;

    let first_mblog = data
        .first()
        .ok_or_else(|| anyhow!("mblog is empty!"))?
        .mblog
        .as_ref()
        .ok_or_else(|| anyhow!("Can not get mblog!"))?;

    if v.is_err() {
        con.set(&key, first_mblog.created_at.clone()).await?;
    }

    if let Ok(v) = v {
        let old_created_at_index = data
            .iter()
            .position(|x| x.mblog.as_ref().map(|x| &x.created_at) == Some(&v));

        if old_created_at_index.is_none() {
            con.set(&key, first_mblog.created_at.clone()).await?;
        }

        let old_created_at_index = old_created_at_index.unwrap_or(0);

        let mut telegram_sends = vec![];
        for (i, c) in data.iter().enumerate() {
            if i < old_created_at_index {
                let mblog = c
                    .mblog
                    .as_ref()
                    .ok_or_else(|| anyhow!("Can not get mblog!"))?;
                let username = mblog.user.screen_name.clone();
                let s = format!(
                    "<b>{} 发新微博啦！</b>\n{}\n\n{}",
                    username,
                    mblog.created_at,
                    html2text::from_read(mblog.text.as_bytes(), 80)
                );

                info!("{}", s);

                let photos = mblog
                    .pics
                    .as_ref()
                    .map(|x| x.iter().map(|x| x.url.clone()).collect::<Vec<_>>());

                telegram_sends.push(TelegramSend {
                    msg: s,
                    photos,
                    photo: None,
                });
            }
        }

        check_and_send(bot, telegram_chat_id, telegram_sends, client).await?;
        con.set(&key, first_mblog.created_at.clone()).await?;
    }

    Ok(())
}

async fn check_and_send(
    bot: Option<&AutoSend<Bot>>,
    telegram_chat_id: Option<i64>,
    mut telegram_sends: Vec<TelegramSend>,
    client: &Client,
) -> Result<()> {
    if let Some(bot) = bot {
        if let Some(chat_id) = telegram_chat_id {
            sender::send(&mut telegram_sends, bot, chat_id, client).await?;
        }
    }

    Ok(())
}

fn timestamp_to_date(t: u64) -> Result<String> {
    let format = format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]")?;
    let date = OffsetDateTime::from_unix_timestamp(t.try_into()?)?
        .to_offset(offset!(+8))
        .format(&format)?;

    Ok(date)
}
