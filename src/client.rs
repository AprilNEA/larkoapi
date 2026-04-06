use reqwest::Client;
use serde_json::json;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::info;

use crate::card::LarkCard;

pub struct LarkBotClient {
    app_id: String,
    app_secret: String,
    base_url: String,
    token: Mutex<CachedToken>,
    http: Client,
}

struct CachedToken {
    value: String,
    expires_at: Instant,
}

impl LarkBotClient {
    pub fn new(app_id: String, app_secret: String, base_url: String, http: Client) -> Self {
        Self {
            app_id,
            app_secret,
            base_url,
            token: Mutex::new(CachedToken {
                value: String::new(),
                expires_at: Instant::now(),
            }),
            http,
        }
    }

    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    pub fn app_secret(&self) -> &str {
        &self.app_secret
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn get_token(&self) -> Result<String, String> {
        let mut cached = self.token.lock().await;

        if !cached.value.is_empty()
            && cached.expires_at > Instant::now() + std::time::Duration::from_secs(300)
        {
            return Ok(cached.value.clone());
        }

        let url = format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            self.base_url
        );
        let resp = self
            .http
            .post(&url)
            .json(&json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret,
            }))
            .send()
            .await
            .map_err(|e| format!("token request failed: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("token response parse failed: {e}"))?;

        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(format!("token API error: {body}"));
        }

        let token = body
            .get("tenant_access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing tenant_access_token".to_string())?
            .to_string();

        let expire = body.get("expire").and_then(|v| v.as_u64()).unwrap_or(7200);

        cached.value = token.clone();
        cached.expires_at = Instant::now() + std::time::Duration::from_secs(expire);

        info!("refreshed lark bot tenant access token (expires in {expire}s)");
        Ok(token)
    }

    /// Send an interactive card message to a recipient.
    ///
    /// `receive_id_type` can be `"chat_id"`, `"open_id"`, `"user_id"`, or `"email"`.
    pub async fn send_message(
        &self,
        receive_id: &str,
        receive_id_type: &str,
        card: &LarkCard,
    ) -> Result<(), String> {
        let token = self.get_token().await?;

        let payload = json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": serde_json::to_string(card).unwrap_or_default(),
        });

        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type={receive_id_type}",
            self.base_url
        );
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("send_message failed: {e}"))?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status.is_success() {
            let parsed: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            let code = parsed.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
            if code != 0 {
                return Err(format!("send_message API code {code}: {body}"));
            }
            Ok(())
        } else {
            Err(format!("send_message returned {status}: {body}"))
        }
    }

    /// Send an interactive card to a chat by chat_id.
    pub async fn reply_to_chat(&self, chat_id: &str, card: &LarkCard) -> Result<(), String> {
        self.send_message(chat_id, "chat_id", card).await
    }

    /// Send an interactive card as a DM to a user by email.
    pub async fn send_dm(&self, email: &str, card: &LarkCard) -> Result<(), String> {
        self.send_message(email, "email", card).await
    }

    /// Upload an image and return the image_key.
    pub async fn upload_image(&self, jpeg_data: &[u8]) -> Result<String, String> {
        let token = self.get_token().await?;

        let url = format!("{}/open-apis/im/v1/images", self.base_url);
        let part = reqwest::multipart::Part::bytes(jpeg_data.to_vec())
            .file_name("snapshot.jpg")
            .mime_str("image/jpeg")
            .map_err(|e| format!("mime: {e}"))?;

        let form = reqwest::multipart::Form::new()
            .text("image_type", "message")
            .part("image", part);

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("upload failed: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("upload parse: {e}"))?;

        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(format!("upload error: {body}"));
        }

        body.pointer("/data/image_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("no image_key: {body}"))
    }
}
