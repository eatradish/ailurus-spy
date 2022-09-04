use anyhow::Result;
use async_trait::async_trait;
use futures::{stream::FuturesOrdered, StreamExt};
use reqwest::Url;
use teloxide::{
    adaptors::AutoSend,
    payloads::SendMessageSetters,
    prelude::{Requester, RequesterExt},
    types::{ChatId, InputFile, InputMedia, InputMediaPhoto, ParseMode, Recipient},
    Bot,
};

use tracing::error;

use crate::Sender;

#[derive(Clone)]
pub struct Message {
    pub text: String,
    pub photos: Option<Vec<String>>,
}

#[async_trait]
pub trait AilurusSender: Send + Sync {
    async fn send_msg(&self, chat_id: i64, msg: &str) -> Result<()>;

    async fn send_photos(&self, chat_id: i64, url: &[&str], caption: Option<&str>) -> Result<()>;

    async fn send(&self, chat_id: i64, msg: Message) -> Result<()> {
        if let Some(photos) = msg.photos {
            self.send_photos(
                chat_id,
                &photos.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
                Some(&msg.text),
            )
            .await?;
        } else {
            self.send_msg(chat_id, &msg.text).await?;
        }

        Ok(())
    }
}

pub struct Tg {
    bot: AutoSend<Bot>,
}

impl Tg {
    pub fn new() -> Self {
        Tg {
            bot: Bot::from_env().auto_send(),
        }
    }
}

// fn tg_sender_into_box(tg_sender: Tg) -> Box<dyn AilurusSender> {
//     Box::new(tg_sender)
// }

pub async fn sends(senders: &[Sender], msg: Message, is_admin: bool) -> Result<()> {
    let mut tasks = FuturesOrdered::new();

    for sender in senders {
        let msg = msg.clone();

        let chat_id = if is_admin {
            sender.admin_chat_id
        } else {
            sender.target_chat_id
        };

        tasks.push(sender.sender.send(chat_id, msg));
    }

    tasks
        .map(|x| {
            if let Err(e) = x {
                error!("{}", e);
            }
        })
        .collect::<Vec<_>>()
        .await;

    Ok(())
}

#[async_trait]
impl AilurusSender for Tg {
    async fn send_msg(&self, chat_id: i64, msg: &str) -> Result<()> {
        let _ = self
            .bot
            .send_message(Recipient::Id(ChatId(chat_id)), msg)
            .parse_mode(ParseMode::Html)
            .await?;

        Ok(())
    }

    async fn send_photos(&self, chat_id: i64, urls: &[&str], caption: Option<&str>) -> Result<()> {
        let mut urls_after = vec![];
        for url in urls {
            urls_after.push(InputMedia::Photo(InputMediaPhoto {
                media: InputFile::url(Url::parse(url)?),
                caption: caption.map(|x| x.to_string()),
                parse_mode: Some(ParseMode::Html),
                caption_entities: None,
            }));
        }

        let _ = self
            .bot
            .send_media_group(Recipient::Id(ChatId(chat_id)), urls_after)
            .await?;

        Ok(())
    }
}
