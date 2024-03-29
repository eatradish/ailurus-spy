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
    user_profile: Option<UserProfile>,
}

#[derive(Debug, Deserialize, Clone)]
struct UserProfile {
    info: UserProfileInfo,
}

#[derive(Debug, Deserialize, Clone)]
struct UserProfileInfo {
    uid: u64,
    uname: String,
}

#[derive(Debug, Deserialize, Clone)]
struct CardInner {
    user: Option<User>,
    item: Option<CardItem>,
    title: Option<String>,
    short_link: Option<String>,
    short_link_v2: Option<String>,
    pic: Option<String>,
    origin: Option<String>,
    origin_dese: Box<Option<CardInner>>,
    origin_user: Option<UserProfile>,
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
    pub dynamic_id: u64,
    pub timestamp: u64,
}

fn trans(c: CardInner, desc: Desc) -> BiliDynamicResult {
    let c_user_clone = c.user.clone();
    let c_user_clone_2 = c.user.clone();
    let desc_user_profile_clone = desc.user_profile.clone();

    let user = if let Some(u) = c.user.and_then(|x| x.name) {
        Some(u)
    } else if let Some(u) = c_user_clone.and_then(|x| x.uname) {
        Some(u)
    } else {
        desc.user_profile.map(|x| x.info.uname)
    };
    let uid = if let Some(id) = c_user_clone_2.map(|x| x.uid) {
        Some(id)
    } else {
        desc_user_profile_clone.map(|x| x.info.uid)
    };
    let item_clone = c.item.clone();
    let item_clone_2 = c.item.clone();
    let description = if let Some(desc) = c.item.and_then(|x| x.description) {
        Some(desc)
    } else if let Some(content) = item_clone.and_then(|x| x.content) {
        if let Some(origin_desc) = c
            .origin_dese
            .clone()
            .and_then(|x| x.item.and_then(|x| x.description))
        {
            if let Some(origin_user) = c.origin_user.map(|x| x.info.uname) {
                Some(format!("{} // {}: {}", content, origin_user, origin_desc))
            } else {
                Some(format!("{} // {}", content, origin_desc))
            }
        } else if let Some(origin_title) = c.origin_dese.clone().and_then(|x| x.title) {
            Some(format!(
                "{} // {}{}",
                content,
                origin_title,
                if let Some(link) = c.origin_dese.clone().and_then(|x| x.short_link_v2) {
                    format!("({})", link)
                } else if let Some(link) = c.origin_dese.clone().and_then(|x| x.short_link) {
                    format!("({})", link)
                } else {
                    "".to_string()
                }
            ))
        } else {
            Some(content)
        }
    } else {
        c.title.map(|x| {
            format!(
                "{}\n{}",
                x,
                if let Some(url) = c.short_link_v2 {
                    url
                } else if let Some(url) = c.short_link {
                    url
                } else {
                    "".to_string()
                }
            )
        })
    };
    let dynamic_id = desc.dynamic_id;
    let time = desc.timestamp;
    let picture = if let Some(pics) = item_clone_2.and_then(|x| x.pictures) {
        Some(pics)
    } else if let Some(pic) = c.pic {
        Some(vec![Picture { img_src: Some(pic) }])
    } else {
        c.origin_dese.and_then(|x| x.item.and_then(|x| x.pictures))
    };

    BiliDynamicResult {
        user,
        uid,
        description,
        picture,
        dynamic_id,
        timestamp: time,
    }
}

pub async fn get_ailurus_dynamic(uid: u64, client: &Client) -> Result<Vec<BiliDynamicResult>> {
    let mut result = vec![];
    let mut headers = HeaderMap::new();
    headers.append(
        "Referer",
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
                let s: CardInner = serde_json::from_str(origin)?;
                card_dese.origin_dese = Box::new(Some(s));
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
    let json = get_ailurus_dynamic(1501380958, &client).await.unwrap();
    dbg!(json[4].to_owned());
}
