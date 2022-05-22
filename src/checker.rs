use anyhow::{anyhow, Result};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use reqwest::{Client, Url};
use teloxide::{
    adaptors::AutoSend,
    payloads::{SendMessageSetters, SendPhotoSetters},
    prelude::Requester,
    types::{ChatId, InputFile, InputMedia, InputMediaPhoto, ParseMode, Recipient},
    Bot,
};
use time::{format_description, macros::offset, OffsetDateTime};
use tracing::{error, info};

use crate::{dynamic, live};

pub async fn check_dynamic_update(
    con: &MultiplexedConnection,
    uid: u64,
    client: &Client,
    bot: Option<&AutoSend<Bot>>,
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
    let mut is_update = false;
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
                let s = format!(
                    "<b>{} 有新动态啦！</b>\n{}\n{}\n\n{}",
                    name, date, desc, i.url
                );
                if let Some(picture) = &i.picture {
                    let mut group = Vec::new();
                    for i in picture {
                        if let Some(img) = &i.img_src {
                            group.push(InputMedia::Photo(InputMediaPhoto {
                                media: InputFile::url(Url::parse(img)?),
                                caption: Some(s.clone()),
                                parse_mode: Some(ParseMode::Html),
                                caption_entities: None,
                            }));
                        }
                    }
                    if let Some(bot) = bot {
                        bot.send_media_group(Recipient::Id(ChatId(-1001675012012)), group.clone())
                            .await?;
                        if group.len() > 8 {
                            bot.send_message(Recipient::Id(ChatId(-1001675012012)), s).await?;
                        }
                    }
                } else {
                    if let Some(bot) = bot {
                        bot.send_message(Recipient::Id(ChatId(-1001675012012)), s)
                            .parse_mode(ParseMode::Html)
                            .await?;
                    }
                }
            }
        }
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
            if let Some(bot) = bot {
                bot.send_photo(
                    Recipient::Id(ChatId(-1001675012012)),
                    InputFile::url(Url::parse(&live.user_cover)?),
                )
                .caption(s)
                .parse_mode(ParseMode::Html)
                .await?;
            }
            con.set(key, true).await?;
        } else if db_live_status && ls == 1 {
            con.set(key, true).await?;
        } else if ls != 1 {
            con.set(key, false).await?;
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
