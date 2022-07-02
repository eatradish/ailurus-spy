use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Result};
use cookie_store::CookieStore;
use fancy_regex::Regex;
use reqwest::{header::HeaderMap, Client, Response, Url};
use reqwest_cookie_store::CookieStoreMutex;
use rustyline::Editor;
use serde::Deserialize;
use tracing::info;

const SEND_SMS_URL: &str = "https://passport.weibo.cn/signin/secondverify/ajsend";
const CODE_CHECK_URL: &str = "https://passport.weibo.cn/signin/secondverify/ajcheck";
const LOGIN_URL: &str = "https://passport.sina.cn/sso/login";
const SEND_PRIVATE_MSG_URL: &str = "https://passport.weibo.cn/signin/secondverify/index";
// const WEIBO_HOME_URL: &str = "https://weibo.com";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/86.0.4240.183 Safari/537.36";

macro_rules! API_URL {
    () => {
        "https://m.weibo.cn/api/container/getIndex?uid={}&luicode=10000011&lfid=231093_-_selffollowed&type=uid&value={}&containerid={}"
    };
}

#[derive(Debug, Deserialize)]
struct LoginResponseResult {
    retcode: i64,
    data: LoginResponseResultData,
    msg: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponseResultData {
    errurl: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PhoneList {
    #[serde(rename = "maskMobile")]
    mask_mobile: String,
    number: u64,
}

#[derive(Debug, Deserialize)]
struct VerifSMS {
    retcode: i64,
    msg: String,
}

#[derive(Debug, Deserialize)]
struct VeriCheck {
    retcode: i64,
    msg: String,
    data: VeriCheckData,
}

#[derive(Debug, Deserialize)]
struct VeriCheckData {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WeiboIndex {
    pub data: WeiboIndexData,
}

#[derive(Debug, Deserialize)]
pub struct WeiboIndexData {
    pub cards: Option<Vec<WeiboIndexDataCard>>,
    #[serde(rename = "tabsInfo")]
    pub tabs_info: Option<WeiboIndexDataTabsInfo>,
}

#[derive(Debug, Deserialize)]
pub struct WeiboIndexDataTabsInfo {
    pub tabs: Vec<WeiboIndexDataTabsInfoTab>,
}

#[derive(Debug, Deserialize)]
pub struct WeiboIndexDataTabsInfoTab {
    pub tab_type: String,
    pub containerid: String,
}

#[derive(Debug, Deserialize)]
pub struct WeiboIndexDataCard {
    pub card_type: Option<u64>,
    pub mblog: Option<WeiboIndexDataCardMblog>,
}

#[derive(Debug, Deserialize)]
pub struct WeiboIndexDataCardMblog {
    pub id: String,
    pub user: WeiboIndexDataCardMblogUser,
    pub created_at: String,
    pub pics: Option<Vec<WeiboIndexDataCardMblogPic>>,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct WeiboIndexDataCardMblogUser {
    pub screen_name: String,
}

#[derive(Debug, Deserialize)]
pub struct WeiboIndexDataCardMblogPic {
    pub url: String,
}

#[derive(Clone)]
pub struct WeiboClient {
    client: Client,
    cookie_store: Arc<CookieStoreMutex>,
}

impl WeiboClient {
    pub fn new() -> Result<Self> {
        let cookie_store = reqwest_cookie_store::CookieStoreMutex::new(CookieStore::default());
        let cookie_store = Arc::new(cookie_store);
        let cookie_store_clone = cookie_store.clone();

        Ok(Self {
            client: Client::builder()
                .cookie_store(true)
                .cookie_provider(cookie_store)
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(30))
                .build()?,
            cookie_store: cookie_store_clone,
        })
    }

    async fn get(
        &self,
        url: &str,
        query: Option<&[(&str, &str)]>,
        headers: Option<HeaderMap>,
    ) -> Result<Response> {
        let resp = self
            .client
            .get(url)
            .query(query.unwrap_or_default())
            .headers(headers.unwrap_or_default())
            .timeout(Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?;

        Ok(resp)
    }

    async fn post(
        &self,
        url: &str,
        headers: Option<HeaderMap>,
        body: Option<&[(&str, &str)]>,
    ) -> Result<Response> {
        let resp = self
            .client
            .post(url)
            .form(body.unwrap_or_default())
            .headers(headers.unwrap_or_default())
            .timeout(Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?;

        Ok(resp)
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<()> {
        let body = &[
            ("username", username),
            ("password", password),
            ("savestate", "1"),
            ("ec", "1"),
            ("pagerefer", ""),
            ("entry", "wapsso"),
            ("sinacnlogin", "1"),
        ];

        let mut headers = HeaderMap::new();
        headers.insert("Content", "application/x-www-form-urlencoded".parse()?);
        headers.insert("Origin", "https://passport.sina.cn".parse()?);
        headers.insert("Referer", "https://passport.sina.cn/signin/signin".parse()?);
        headers.insert("Content-Type", "application/x-www-form-urlencoded".parse()?);

        let resp = self.post(LOGIN_URL, Some(headers), Some(body)).await?;
        let json = resp.json::<LoginResponseResult>().await?;

        match json.retcode {
            50011002 => bail!("Failed to login: fail to login, username or password error!"),
            50050011 => {
                let login_url = self
                    .verification(
                        &json
                            .data
                            .errurl
                            .ok_or_else(|| anyhow!("Can not get verif url!"))?,
                    )
                    .await?;

                self.get(&login_url, None, None).await?;
                info!("Login successfully! Hello {}", username);
            }
            20000000 => {
                info!("Login successfully! Hello {}", username);
            }
            _ => bail!("{}", json.msg),
        };

        Ok(())
    }

    async fn verification(&self, verif_url: &str) -> Result<String> {
        let resp = self.get(verif_url, None, None).await?;
        let text = resp.text().await?;
        let json = self.send_verif(&text, None).await?;
        // let mut num_times = 0;
        let mut msg_type = "sms";
        // let mut msg_type = "private_msg";

        let mut s =
            "You have to secondverify your account, please input the sms code your phone received: ";

        if json.retcode != 100000 {
            // num_times += 1;
            // if num_times > 1 {
            //     bail!("{}", json.msg)
            // }
            if json.retcode == 8513 {
                s = "You have to secondverify your account, please input the verification code in your private message: ";
                msg_type = "private_msg";
                self.send_verif(&text, Some(msg_type)).await?;
            } else {
                bail!("{}", json.msg)
            }
        }

        let mut reader = Editor::<()>::new();
        let code = reader.readline(s)?;

        let query = &[("code", code.as_str()), ("msg_type", msg_type)];
        let resp = self.get(CODE_CHECK_URL, Some(query), None).await?;
        let json = resp.json::<VeriCheck>().await?;
        if json.retcode != 100000 {
            bail!("{}", json.msg)
        }
        let login_url = json
            .data
            .url
            .ok_or_else(|| anyhow!("Can not get login url!"))?;

        Ok(login_url)
    }

    async fn send_verif(&self, text: &str, msg_type: Option<&str>) -> Result<VerifSMS> {
        let msg_type = msg_type.unwrap_or("sms");

        let mut query = vec![("msg_type".to_string(), msg_type.to_string())];

        if msg_type == "sms" {
            let phone_list = Regex::new(r"phoneList: JSON.parse\(\'([^\']+)\'\)")?
                .find(text)?
                .ok_or_else(|| anyhow!("Can not get phone list!"))?
                .as_str();

            let phone_list = phone_list
                .split_once("('")
                .map(|x| x.1)
                .and_then(|x| x.split_once("')"))
                .map(|x| x.0)
                .ok_or_else(|| anyhow!("Can not split phone list!"))?;

            let json: Vec<PhoneList> = serde_json::from_str(phone_list)?;

            let first_json = json
                .first()
                .ok_or_else(|| anyhow!("phone list is empty!"))?;

            query.push(("number".to_string(), format!("{}", first_json.number)));
            query.push(("mask_mobile".to_string(), first_json.mask_mobile.clone()));
        } else {
            self.get(SEND_PRIVATE_MSG_URL, Some(&[("way", "private_msg")]), None)
                .await?;
        }

        let query = query
            .iter()
            .map(|(x, y)| (x.as_str(), y.as_str()))
            .collect::<Vec<_>>();

        let resp = self.get(SEND_SMS_URL, Some(&query), None).await?;
        let json = resp.json::<VerifSMS>().await?;

        Ok(json)
    }

    async fn get_container_id(
        &self,
        profile_url: &str,
        uid: Option<&str>,
    ) -> Result<(String, String)> {
        self.get(profile_url, None, None).await?;

        let mut container_id = {
            let mut match_cookie = None;
            let regex = Regex::new(r"fid%3D(\d+)%26")?;
            let store = self.cookie_store.lock().map_err(|e| anyhow!("{}", e))?;
            for c in store.iter_any() {
                if let Some(v) = regex.find(c.value())? {
                    match_cookie = Some(v.as_str());
                    break;
                }
            }

            let container_id = match_cookie.ok_or_else(|| anyhow!("Can not get container id!"))?;

            container_id.replace("fid%3D", "").replace("%26", "")
        };

        let uid = if let Some(uid) = uid {
            uid.to_string()
        } else {
            get_uid(profile_url)?
        };

        let api_url = format!(API_URL!(), uid, uid, container_id);

        let resp = self.get(&api_url, None, None).await?;

        let json = resp.json::<WeiboIndex>().await?;

        let tabs = json
            .data
            .tabs_info
            .ok_or_else(|| anyhow!("Can not get weibo index tabs field!"))?
            .tabs;

        for i in tabs {
            if i.tab_type == "weibo" {
                container_id = i.containerid;
            }
        }

        Ok((container_id, uid))
    }

    pub async fn get_ailurus(
        &self,
        profile_url: &str,
        container_id: Option<String>,
    ) -> Result<(WeiboIndex, String)> {
        let (container_id, uid) = if container_id.is_none() {
            self.get_container_id(profile_url, None).await?
        } else {
            (container_id.unwrap(), get_uid(profile_url)?)
        };

        let api_url = format!(API_URL!(), uid, uid, container_id);
        let resp = self.get(&api_url, None, None).await;

        let (json, container_id) = if let Ok(resp) = resp {
            let json = resp.json::<WeiboIndex>().await?;

            (json, container_id)
        } else {
            let (container_id, uid) = self.get_container_id(profile_url, None).await?;
            let api_url = format!(API_URL!(), uid, uid, container_id);
            let resp = self.get(&api_url, None, None).await?;
            let json = resp.json::<WeiboIndex>().await?;

            (json, container_id)
        };

        Ok((json, container_id))
    }
}

pub fn get_uid(profile_url: &str) -> Result<String> {
    let url = Url::parse(profile_url)?;
    let query = url
        .query()
        .ok_or_else(|| anyhow!("Can not get url query!"))?;
    let query_vec = query.split('&');
    let mut uid = None;
    for i in query_vec {
        if i.starts_with("uid") {
            uid = i.split('=').nth(1);
            break;
        }
    }

    Ok(uid.ok_or_else(|| anyhow!("Can not get uid!"))?.to_string())
}
