use anyhow::Result;
use reqwest::{header::HeaderMap, Client};
use serde::Deserialize;
use tracing::error;

#[derive(Debug, Deserialize, Clone)]
struct BiliDynamic {
    data: Data,
}

#[derive(Debug, Deserialize, Clone)]
struct Data {
    cards: Vec<Card>,
}

#[derive(Debug, Deserialize, Clone)]
struct Card {
    desc: Desc,
    card: String,
    card_dese: Option<CardInner>,
}

#[derive(Debug, Deserialize, Clone)]
struct Desc {
    dynamic_id: u64,
    timestamp: u64,
}

#[derive(Debug, Deserialize, Clone)]
struct CardInner {
    user: Option<User>,
    item: Option<CardItem>,
    title: Option<String>,
    origin: Option<String>,
    origin_dese: Option<Origin>,
}

#[derive(Debug, Deserialize, Clone)]
struct Origin {
    item: Option<CardItem>,
}

#[derive(Debug, Deserialize, Clone)]
struct CardItem {
    description: Option<String>,
    pictures: Option<Vec<Picture>>,
    content: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct User {
    name: Option<String>,
    uname: Option<String>,
    uid: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Picture {
    pub img_src: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BiliDynamicResult {
    pub user: Option<String>,
    pub uid: Option<u64>,
    pub description: Option<String>,
    pub picture: Option<Vec<Picture>>,
    pub url: String,
    pub timestamp: u64,
}

fn trans(c: CardInner, desc: Desc) -> BiliDynamicResult {
    let user_info_clone = c.user.clone();
    let user_info_clone_2 = c.user.clone();

    let user = if let Some(u) = c.user.and_then(|x| x.name) {
        Some(u)
    } else {
        user_info_clone.and_then(|x| x.uname)
    };
    let uid = if let Some(user_info) = user_info_clone_2 {
        Some(user_info.uid)
    } else {
        None
    };
    let item_clone = c.item.clone();
    let item_clone_2 = c.item.clone();
    let description = if let Some(desc) = c.item.and_then(|x| x.description) {
        Some(desc)
    } else if let Some(content) = item_clone.and_then(|x| x.content) {
        if let Some(origin_desc) = c
            .origin_dese
            .and_then(|x| x.item.and_then(|x| x.description))
        {
            Some(format!("{} // {}", content, origin_desc))
        } else {
            Some(content)
        }
    } else {
        c.title
    };
    let dynamic_id = desc.dynamic_id;
    let url = format!("https://t.bilibili.com/{}", dynamic_id);
    let time = desc.timestamp;

    BiliDynamicResult {
        user,
        uid,
        description,
        picture: item_clone_2.and_then(|x| x.pictures),
        url,
        timestamp: time,
    }
}

pub async fn get_ailurus_dynamic(uid: u64, client: &Client) -> Result<Vec<BiliDynamicResult>> {
    let mut result = vec![];
    let mut headers = HeaderMap::new();
    headers.append(
        "Referred",
        (format!("https://space.bilibili.com/{}", uid)).parse()?,
    );
    let mut r = client
        .get(format!(
            "https://api.vc.bilibili.com/dynamic_svr/v1/dynamic_svr/space_history?host_uid={}",
            &uid
        ))
        .headers(headers)
        .send()
        .await?
        .error_for_status()?
        .json::<BiliDynamic>()
        .await?;

    let cards = r.data.cards.to_owned();

    for (i, c) in cards.iter().enumerate() {
        let json = serde_json::from_str::<CardInner>(&c.card);
        if json.is_err() {
            error!("{} {:?} {:?}", i, c, &json);
        }
        if let Ok(json) = json {
            r.data.cards[i].card_dese = Some(json);
        } else {
            continue;
        }
        if let Some(mut card_dese) = r.data.cards[i].card_dese.to_owned() {
            if let Some(origin) = &card_dese.origin {
                let s: Origin = serde_json::from_str(origin)?;
                card_dese.origin_dese = Some(s);
                r.data.cards[i].card_dese = Some(card_dese);
            }
        }
        result.push(trans(
            r.data.cards[i].card_dese.clone().unwrap(),
            r.data.cards[i].clone().desc,
        ));
    }

    Ok(result)
}

#[tokio::test]
async fn test() {
    let client = Client::new();
    let json = get_ailurus_dynamic(11554380, &client).await.unwrap();
    dbg!(json);
}
