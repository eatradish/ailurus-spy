use anyhow::{anyhow, Result};
use redis::AsyncCommands;
use reqwest::Client;
use tracing::info;

use crate::{dynamic, RedisAsyncConnect};

async fn check_dynamic_update(
    con: &mut RedisAsyncConnect,
    uid: u64,
    client: &Client,
) -> Result<()> {
    let key = format!("dynamic-{}", uid);
    let dynamic = dynamic::get_ailurus_dynamic(uid, client).await?;
    let v: Result<u64> = con.get(&key).await.map_err(|e| anyhow!("{}", e));
    if v.is_err() {
        con.set(&key, dynamic[0].timestamp).await?;
    }
    if let Ok(t) = v {
        if dynamic[0].timestamp > t {
            let name = if let Some(name) = dynamic[0].user.clone() {
                name
            } else {
                format!("{}", uid)
            };
            let desc = if let Some(desc) = dynamic[0].description.clone() {
                desc
            } else {
                "None".to_string()
            };
            info!("用户 {} 有新动态！内容：{}", name, desc);
            con.set(&key, dynamic[0].timestamp).await?;
        }
    }

    Ok(())
}
