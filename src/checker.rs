use std::time::SystemTime;

use anyhow::{anyhow, Result};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use reqwest::{Client, Url};
use teloxide::{
    adaptors::AutoSend,
    prelude::Requester,
    types::{ChatId, InputFile, InputMedia, InputMediaPhoto, Recipient},
    Bot,
};
use time::{format_description, OffsetDateTime};
use tracing::{error, info};

use crate::{dynamic, live};

pub async fn check_dynamic_update(
    con: &MultiplexedConnection,
    uid: u64,
    client: &Client,
    bot: &AutoSend<Bot>,
) -> Result<()> {
    let mut con = con.clone();
    info!("checking {} dynamic update ...", uid);
    let key = format!("dynamic-{}", uid);
    let dynamic = dynamic::get_ailurus_dynamic(uid, client).await?;
    let v: Result<u64> = con.get(&key).await.map_err(|e| anyhow!("{}", e));
    if v.is_err() {
        info!("Creating new spy {}...", &key);
        con.set(&key, dynamic[0].timestamp).await?;
    }
    if let Ok(t) = v {
        for i in dynamic {
            if i.timestamp > t {
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
                let t = OffsetDateTime::from_unix_timestamp(i.timestamp.try_into()?)?;
                let format =
                    format_description::parse("[year]-[month]-[day] [year]-[month]-[day]")?;
                let date = t.format(&format)?;
                let s = format!("{} 有新动态啦！\n{}\n{}\n{}", name, date, desc, i.url);
                if let Some(picture) = &i.picture {
                    let mut group = Vec::new();
                    for i in picture {
                        if let Some(img) = &i.img_src {
                            group.push(InputMedia::Photo(InputMediaPhoto {
                                media: InputFile::url(Url::parse(img)?),
                                caption: Some(s.clone()),
                                parse_mode: None,
                                caption_entities: None,
                            }));
                        }
                    }
                    bot.send_media_group(Recipient::Id(ChatId(-1001675012012)), group)
                        .await?;
                } else {
                    bot.send_message(Recipient::Id(ChatId(-1001675012012)), s)
                        .await?;
                }
                info!("Update {} timestamp", key);
                con.set(&key, i.timestamp).await?;
            }
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
    bot: &AutoSend<Bot>,
) -> Result<()> {
    let mut con = con.clone();
    info!("checking room {} live status update ...", room_id);
    let key = format!("live-{}-timestamp", room_id);
    let key2 = format!("live-{}-status", room_id);
    let live = live::get_live_status(room_id, client).await?;
    let timestamp: Result<u64> = con.get(&key).await.map_err(|e| anyhow!(e));
    let live_status: Result<bool> = con.get(&key2).await.map_err(|e| anyhow!(e));
    let ls = live.live_status;
    let t = SystemTime::now().elapsed()?.as_secs();
    con.set(&key, t).await?;
    if live_status.is_err() || timestamp.is_err() {
        con.set(&key, ls == 1).await?;
    } else if ls == 1 {
        let s = format!(
            "{} 开播啦！\n{}\n{}",
            live.uname,
            live.title,
            format!("https://live.billibili.com/{}", live.room_id)
        );
        info!("{}", s);
        bot.send_message(Recipient::Id(ChatId(-1001675012012)), s)
            .await?;
        con.set(key2, true).await?;
    } else {
        con.set(key2, false).await?;
    }

    Ok(())
}
