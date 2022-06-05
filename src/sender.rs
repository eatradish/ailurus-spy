use std::io::Cursor;

use anyhow::Result;
use image::io::Reader as ImageReader;
use reqwest::{Client, Url};
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

macro_rules! send_photo_with_bytes {
    ($bot:ident, $chat_id:ident, $photo:ident, $msg:expr) => {
        $bot.send_photo(Recipient::Id(ChatId($chat_id)), InputFile::memory($photo))
            .caption($msg)
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
    client: &Client,
) -> Result<()> {
    async fn send_bytes_photo(
        url: &str,
        client: &Client,
        msg: &str,
        chat_id: i64,
        bot: &AutoSend<Bot>,
    ) -> Result<()> {
        let photo = get_photo(url, client).await?;
        send_photo_with_bytes!(bot, chat_id, photo, msg)?;

        Ok(())
    }

    async fn send_bytes_groups(
        urls: &[String],
        msg: &str,
        client: &Client,
        bot: &AutoSend<Bot>,
        chat_id: i64,
    ) -> Result<()> {
        let mut groups = Vec::new();
        for url in urls {
            let photo = get_photo(url, client).await?;
            groups.push(InputMedia::Photo(InputMediaPhoto {
                media: InputFile::memory(photo),
                caption: Some(msg.to_string()),
                parse_mode: Some(ParseMode::Html),
                caption_entities: None,
            }));
        }
        send_group!(bot, chat_id, groups)?;

        Ok(())
    }

    telegram_sends.reverse();

    for i in telegram_sends {
        if let Some(photo) = &i.photo {
            if let Err(e) = send_photo!(bot, chat_id, photo, i.msg) {
                warn!(
                    "Telegram send photo has error! {}, Trying covert image ...",
                    e
                );
                if let Err(e) = send_bytes_photo(photo, client, &i.msg, chat_id, bot).await {
                    warn!(
                        "Telegram send convert photo has error! {}, Trying only send text msg ...",
                        e
                    );
                    send_msg!(bot, chat_id, i.msg);
                }
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
                warn!(
                    "Telegram send group has error! {}, Trying convert image ...",
                    e
                );

                if let Err(e) = send_bytes_groups(photos, &i.msg, client, bot, chat_id).await {
                    warn!(
                        "Telegram send convert group has error! {}, Trying only send text msg ...",
                        e
                    );
                    send_msg!(bot, chat_id, i.msg);
                }
                return Ok(());
            }
            if photos.len() > 1 {
                send_msg!(bot, chat_id, i.msg);
            }
        } else {
            send_msg!(bot, chat_id, i.msg);
        }
    }

    Ok(())
}

pub async fn get_photo(url: &str, client: &Client) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let img = ImageReader::new(Cursor::new(resp))
        .with_guessed_format()?
        .decode()?;
    let mut bytes: Vec<u8> = Vec::new();
    img.write_to(
        &mut Cursor::new(&mut bytes),
        image::ImageOutputFormat::Jpeg(75),
    )?;

    Ok(bytes)
}

#[tokio::test]
async fn test() {
    let client = reqwest::Client::new();
    dbg!(get_photo(
        "https://i0.hdslb.com/bfs/album/199aa8384b65b90d6ab2ecdbc6ef3a652cebf573.jpg",
        &client
    )
    .await
    .unwrap());
}
