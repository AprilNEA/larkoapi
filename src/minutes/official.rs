//! Minutes (妙记) v1 — metadata and media download URL.
//!
//! Scopes: `minutes:minutes:readonly` (meta), `minutes:minutes.media:export`
//! (media). The official Open API does not expose transcript text, subtitle
//! export, or AI summary — only metadata and the A/V file.

use reqwest::Method;
use serde::Deserialize;

use crate::client::LarkBotClient;

/// Minutes metadata returned by [`LarkBotClient::get_minute_meta`].
#[derive(Debug, Clone)]
pub struct MinuteMeta {
    pub token: String,
    pub title: String,
    /// Owner identifier in the ID type configured on the Minutes record.
    pub owner_id: String,
    pub duration_ms: u64,
    /// URL to view the Minutes in the Lark app.
    pub url: String,
}

#[derive(Default, Deserialize)]
struct MinuteEnvelope {
    #[serde(default)]
    minute: MinuteWire,
}

#[derive(Default, Deserialize)]
struct MinuteWire {
    #[serde(default)]
    token: String,
    #[serde(default)]
    owner_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    duration: String,
    #[serde(default)]
    url: String,
}

#[derive(Default, Deserialize)]
struct MediaEnvelope {
    #[serde(default)]
    download_url: String,
}

impl LarkBotClient {
    /// `GET /open-apis/minutes/v1/minutes/:minute_token`
    pub async fn get_minute_meta(&self, minute_token: &str) -> Result<MinuteMeta, String> {
        let resp = self
            .call(
                Method::GET,
                &format!("/open-apis/minutes/v1/minutes/{minute_token}"),
                None,
            )
            .await?;
        let env: MinuteEnvelope = parse_data(&resp, "minute")?;
        Ok(MinuteMeta {
            token: env.minute.token,
            title: env.minute.title,
            owner_id: env.minute.owner_id,
            duration_ms: env.minute.duration.parse().unwrap_or(0),
            url: env.minute.url,
        })
    }

    /// `GET /open-apis/minutes/v1/minutes/:minute_token/media`
    ///
    /// Returns a signed A/V download URL valid for ~1 day. Rate-limited to
    /// 5 req/s by the server.
    pub async fn get_minute_media_url(&self, minute_token: &str) -> Result<String, String> {
        let resp = self
            .call(
                Method::GET,
                &format!("/open-apis/minutes/v1/minutes/{minute_token}/media"),
                None,
            )
            .await?;
        let env: MediaEnvelope = parse_data(&resp, "media")?;
        if env.download_url.is_empty() {
            return Err("empty download_url".into());
        }
        Ok(env.download_url)
    }
}

fn parse_data<T: for<'de> Deserialize<'de> + Default>(
    resp: &serde_json::Value,
    label: &str,
) -> Result<T, String> {
    let data = resp.get("data").cloned().unwrap_or_default();
    serde_json::from_value(data).map_err(|e| format!("parse {label}: {e}"))
}
