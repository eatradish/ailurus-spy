use anyhow::Result;
use reqwest::Url;
use teloxide::{
    adaptors::AutoSend,
    payloads::{SendMessageSetters, SendPhotoSetters},
    prelude::Requester,
    types::{ChatId, InputFile, InputMedia, InputMediaPhoto, ParseMode, Recipient},
    Bot,
};
use tracing::warn;

pub struct TelegramSend {
    pub msg: String,
    pub photos: Option<Vec<String>>,
    pub photo: Option<String>,
}

macro_rules! send_msg {
    ($bot:ident, $chat_id:ident, $msg:expr) => {
        $bot.send_message(Recipient::Id(ChatId($chat_id)), &$msg)
            .parse_mode(ParseMode::Html)
            .await?;
    };
}

macro_rules! send_photo {
    ($bot:ident, $chat_id:ident, $photo:ident, $msg:expr) => {
        $bot.send_photo(
            Recipient::Id(ChatId($chat_id)),
            InputFile::url(Url::parse($photo)?),
        )
        .caption(&$msg)
        .parse_mode(ParseMode::Html)
        .await
    };
}

macro_rules! send_group {
    ($bot:ident, $chat_id:ident, $groups:ident) => {
        $bot.send_media_group(Recipient::Id(ChatId($chat_id)), $groups)
            .await
    };
}

pub async fn send(
    telegram_sends: &mut [TelegramSend],
    bot: &AutoSend<Bot>,
    chat_id: i64,
) -> Result<()> {
    telegram_sends.reverse();
    for i in telegram_sends {
        if let Some(photo) = &i.photo {
            if let Err(e) = send_photo!(bot, chat_id, photo, i.msg) {
                warn!("Telegram send photo has error! {}", e);
                send_msg!(bot, chat_id, i.msg);
            }
        } else if let Some(photos) = &i.photos {
            let mut groups = vec![];
            for j in photos {
                groups.push(InputMedia::Photo(InputMediaPhoto {
                    media: InputFile::url(Url::parse(j)?),
                    caption: Some(i.msg.clone()),
                    parse_mode: Some(ParseMode::Html),
                    caption_entities: None,
                }));
            }
            if let Err(e) = send_group!(bot, chat_id, groups) {
                warn!("Telegram send group has error! {}", e);
                send_msg!(bot, chat_id, i.msg);

                return Ok(());
            }
            if photos.len() > 8 {
                send_msg!(bot, chat_id, i.msg);
            }
        } else {
            send_msg!(bot, chat_id, i.msg);
        }
    }

    Ok(())
}
