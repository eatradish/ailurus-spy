use anyhow::Result;
use dashmap::DashMap;
use lazy_static::lazy_static;
use reqwest::{header::HeaderMap, Client};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LiveRoomInit {
    data: LiveRoomInitData,
}

#[derive(Debug, Deserialize)]
struct LiveRoomInitData {
    room_id: u64,
}

#[derive(Debug, Deserialize)]
struct LiveRoomStatus {
    data: LiveRoomData,
}

#[derive(Debug, Deserialize)]
struct LiveRoomData {
    title: String,
    live_status: i32,
    live_time: String,
    user_cover: String,
}

#[derive(Debug, Deserialize)]
struct LiveUserStatus {
    data: LiveUserStatusData,
}

#[derive(Debug, Deserialize)]
struct LiveUserStatusData {
    info: LiveUserStatusDataInfo,
}

#[derive(Debug, Deserialize)]
struct LiveUserStatusDataInfo {
    uid: u64,
    uname: String,
}

#[derive(Debug, Deserialize)]
pub struct LiveStatusResult {
    pub room_id: u64,
    pub uid: u64,
    pub uname: String,
    pub title: String,
    pub live_status: i32,
    pub live_time: String,
    pub user_cover: String,
}

lazy_static! {
    static ref SHORT_ID_MAP: DashMap<String, u64> = DashMap::new();
}

pub async fn get_live_status(room_id: u64, client: &Client) -> Result<LiveStatusResult> {
    let room_id = get_room_id_from_short(room_id, client).await?;
    let mut header_map = HeaderMap::new();
    header_map.insert(
        "Referer",
        format!("https://live.bilibili.com/{}", room_id).parse()?,
    );
    let live_room_status = client
        .get(format!(
            "https://api.live.bilibili.com/room/v1/Room/get_info?room_id={}&from=room",
            room_id
        ))
        .headers(header_map)
        .send()
        .await?
        .error_for_status()?
        .json::<LiveRoomStatus>()
        .await?;
    let live_room_data = live_room_status.data;
    let live_user_status = get_live_user_info(room_id, client).await?;
    let uid = live_user_status.uid;
    let uname = live_user_status.uname;
    let title = live_room_data.title;
    let live_status = live_room_data.live_status;
    let live_time = live_room_data.live_time;
    let user_cover = live_room_data.user_cover;

    Ok(LiveStatusResult {
        room_id,
        uid,
        uname,
        title,
        live_status,
        live_time,
        user_cover,
    })
}

async fn get_room_id_from_short(room_id: u64, client: &Client) -> Result<u64> {
    let key = format!("short-id-{}", room_id);
    let room_id = if room_id < 10000 {
        if let Some(v) = SHORT_ID_MAP.get(&key) {
            return Ok(*v);
        }
        let mut header_map = HeaderMap::new();
        header_map.insert(
            "Referer",
            (format!("https://live.bilibili.com/{}", room_id)).parse()?,
        );
        let r = client
            .get(format!(
                "https://api.live.bilibili.com/room/v1/Room/room_init?id={}",
                room_id
            ))
            .headers(header_map)
            .send()
            .await?
            .error_for_status()?
            .json::<LiveRoomInit>()
            .await?;
        SHORT_ID_MAP.insert(key, r.data.room_id);

        r.data.room_id
    } else {
        room_id
    };

    Ok(room_id)
}

async fn get_live_user_info(room_id: u64, client: &Client) -> Result<LiveUserStatusDataInfo> {
    let mut header_map = HeaderMap::new();
    header_map.insert(
        "Referer",
        (format!("https://live.bilibili.com/{}", room_id)).parse()?,
    );
    let resp = client
        .get(format!(
            "https://api.live.bilibili.com/live_user/v1/UserInfo/get_anchor_in_room?roomid={}",
            room_id
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<LiveUserStatus>()
        .await?;

    Ok(resp.data.info)
}

#[tokio::test]
async fn test() {
    let client = Client::new();
    let s = get_live_status(22746343, &client).await.unwrap();
    dbg!(s);
}
