use anyhow::{anyhow, Result};
use redis::AsyncCommands;
use reqwest::{Client, Url};
use teloxide::{
    adaptors::AutoSend,
    prelude::Requester,
    types::{ChatId, InputFile, InputMedia, InputMediaPhoto, Recipient},
    Bot,
};
use time::{format_description, OffsetDateTime};
use tracing::{error, info};

use crate::{dynamic, RedisAsyncConnect};

pub async fn check_dynamic_update(
    con: &mut RedisAsyncConnect,
    uid: u64,
    client: &Client,
    bot: &AutoSend<Bot>,
) -> Result<()> {
    info!("checking {} dynamic update ...", uid);
    let key = format!("dynamic-{}", uid);
    let dynamic = dynamic::get_ailurus_dynamic(uid, client).await?;
    let v: Result<u64> = con.get(&key).await.map_err(|e| anyhow!("{}", e));
    if v.is_err() {
        info!("Creating new spy {}...", key);
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
