//! `MinutesWebClient` struct + data-plane methods (list, media URL,
//! subtitle export). Session management lives in `super::session`.

use reqwest::Client;

use super::cookie::{bool_cn, extract_csrf, now_ms};
use super::types::{MinutesWebPage, MinutesWebRecord, SpaceName, SubtitleOptions};

/// Cookie-authenticated Minutes client.
///
/// Construct with [`new`](Self::new), attach a security host via
/// [`with_security_host`](Self::with_security_host) if you want `refresh()`
/// to work, then call the data methods below.
pub struct MinutesWebClient {
    pub(super) base: String,
    pub(super) cookie: String,
    pub(super) csrf: String,
    pub(super) security_host: Option<String>,
    pub(super) http: Client,
}

impl MinutesWebClient {
    /// Build a client from a host base and a browser-harvested cookie.
    /// Fails if the cookie doesn't contain a 36-character `bv_csrf_token`.
    pub fn new(base: String, cookie: String, http: Client) -> Result<Self, String> {
        let csrf = extract_csrf(&cookie)
            .ok_or_else(|| "cookie missing bv_csrf_token (expected 36-char value)".to_string())?;
        Ok(Self {
            base,
            cookie,
            csrf,
            security_host: None,
            http,
        })
    }

    /// List one page of Minutes. Pass `cursor = None` for the newest page;
    /// for subsequent pages use the previous page's last `share_time`.
    pub async fn list_page(
        &self,
        space: SpaceName,
        page_size: u32,
        cursor: Option<u64>,
    ) -> Result<MinutesWebPage, String> {
        let mut url = format!(
            "{}/minutes/api/space/list?size={}&space_name={}",
            self.base.trim_end_matches('/'),
            page_size,
            space.as_param()
        );
        if let Some(ts) = cursor {
            url.push_str(&format!("&timestamp={ts}"));
        }
        let body = self.get_json(&url).await?;
        let data = body
            .get("data")
            .ok_or_else(|| format!("missing data: {body}"))?;
        let list = data.get("list").and_then(|v| v.as_array()).ok_or_else(|| {
            "data.list missing (likely cookie expired — re-harvest bv_csrf_token)".to_string()
        })?;
        let mut items = Vec::with_capacity(list.len());
        for raw in list {
            match serde_json::from_value::<MinutesWebRecord>(raw.clone()) {
                Ok(rec) => items.push(rec),
                Err(e) => return Err(format!("parse record: {e}; raw={raw}")),
            }
        }
        let has_more = data
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(MinutesWebPage { items, has_more })
    }

    /// Page through the entire inbox (newest → oldest). Beware: on a large
    /// tenant this can be thousands of rows. Prefer [`list_page`](Self::list_page)
    /// + external dedup for production loops.
    pub async fn list_all(
        &self,
        space: SpaceName,
        page_size: u32,
    ) -> Result<Vec<MinutesWebRecord>, String> {
        let mut out: Vec<MinutesWebRecord> = Vec::new();
        let mut cursor: Option<u64> = None;
        loop {
            let page = self.list_page(space, page_size, cursor).await?;
            let next_cursor = page.items.last().and_then(|r| r.share_time);
            let page_empty = page.items.is_empty();
            out.extend(page.items);
            if !page.has_more || page_empty || next_cursor.is_none() {
                break;
            }
            cursor = next_cursor;
        }
        Ok(out)
    }

    /// Resolve the signed A/V download URL for a minute via
    /// `/minutes/api/status`. The URL is time-limited — fetch, then download.
    pub async fn get_media_url(&self, object_token: &str) -> Result<String, String> {
        let url = format!(
            "{}/minutes/api/status?object_token={}&language=zh_cn&_t={}",
            self.base.trim_end_matches('/'),
            object_token,
            now_ms()
        );
        let body = self.get_json(&url).await?;
        body.pointer("/data/video_info/video_download_url")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| format!("no video_download_url in: {body}"))
    }

    /// Export subtitles as text (SRT or TXT) via `/minutes/api/export`.
    /// Returns the raw text body — caller chooses whether to parse it.
    pub async fn export_subtitle(
        &self,
        object_token: &str,
        opts: &SubtitleOptions,
    ) -> Result<String, String> {
        let url = format!("{}/minutes/api/export", self.base.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .header("Cookie", &self.cookie)
            .header("bv-csrf-token", &self.csrf)
            .header("Referer", self.referer())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .query(&[
                ("object_token", object_token),
                ("add_speaker", bool_cn(opts.include_speaker)),
                ("add_timestamp", bool_cn(opts.include_timestamp)),
                ("format", &opts.format.as_param().to_string()),
            ])
            .send()
            .await
            .map_err(|e| format!("export http: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("export body: {e}"))?;
        if !status.is_success() {
            return Err(format!("export HTTP {status}: {text}"));
        }
        // The endpoint returns a JSON error envelope on failure, plain text on
        // success. Treat an exact `{...}` prefix as failure and surface it.
        if text.starts_with('{')
            && let Ok(j) = serde_json::from_str::<serde_json::Value>(&text)
            && j.get("code")
                .and_then(|v| v.as_i64())
                .is_some_and(|c| c != 0)
        {
            return Err(format!("export API error: {text}"));
        }
        Ok(text)
    }

    async fn get_json(&self, url: &str) -> Result<serde_json::Value, String> {
        let resp = self
            .http
            .get(url)
            .header("Cookie", &self.cookie)
            .header("bv-csrf-token", &self.csrf)
            .header("Referer", self.referer())
            .send()
            .await
            .map_err(|e| format!("GET {url}: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status} for {url}: {text}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("decode {url}: {e} body={text}"))
    }

    pub(super) fn referer(&self) -> String {
        format!("{}/minutes/me", self.base.trim_end_matches('/'))
    }
}
