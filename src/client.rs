use reqwest::{Client, Method};
use serde_json::{Value, json};
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::info;

use crate::card::LarkCard;
use crate::models::{ChatMember, DriveFile};

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

    /// Send an interactive card to a chat and return the message_id for later updates.
    pub async fn send_card_returning_id(
        &self,
        chat_id: &str,
        card: &LarkCard,
    ) -> Result<String, String> {
        let token = self.get_token().await?;

        let payload = json!({
            "receive_id": chat_id,
            "msg_type": "interactive",
            "content": serde_json::to_string(card).unwrap_or_default(),
        });

        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type=chat_id",
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

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("send_message parse: {e}"))?;

        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(format!("send_message API code {code}: {body}"));
        }

        body.pointer("/data/message_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("no message_id in response: {body}"))
    }

    /// Update an existing interactive card message by message_id.
    ///
    /// Uses the Lark PATCH `/open-apis/im/v1/messages/:message_id` API.
    /// The card must have been sent within the last 14 days.
    pub async fn update_card(&self, message_id: &str, card: &LarkCard) -> Result<(), String> {
        let token = self.get_token().await?;

        let payload = json!({
            "content": serde_json::to_string(card).unwrap_or_default(),
        });

        let url = format!("{}/open-apis/im/v1/messages/{message_id}", self.base_url);
        let resp = self
            .http
            .patch(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("update_card failed: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("update_card parse: {e}"))?;

        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(format!("update_card API code {code}: {body}"));
        }

        Ok(())
    }
}

/// Docx / Drive / IM extensions used by the standup bot and similar workflows.
///
/// These methods share a thin `call` helper that handles token refresh, bearer
/// auth, HTTP status check, and `code != 0` unwrap into `Err`.
impl LarkBotClient {
    pub(crate) async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        let token = self.get_token().await?;
        let url = format!("{}{}", self.base_url(), path);
        let mut req = self.http.request(method, &url).bearer_auth(&token);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("lark request failed: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}"));
        }
        let value: Value =
            serde_json::from_str(&text).map_err(|e| format!("decode JSON failed ({e}): {text}"))?;
        let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(format!("lark API code {code}: {text}"));
        }
        Ok(value)
    }

    pub async fn list_chat_members(&self, chat_id: &str) -> Result<Vec<ChatMember>, String> {
        let mut out = Vec::new();
        let mut page_token = String::new();
        loop {
            let path = if page_token.is_empty() {
                format!("/open-apis/im/v1/chats/{chat_id}/members?page_size=100")
            } else {
                format!(
                    "/open-apis/im/v1/chats/{chat_id}/members?page_size=100&page_token={page_token}"
                )
            };
            let resp = self.call(Method::GET, &path, None).await?;
            if let Some(items) = resp.pointer("/data/items").and_then(|v| v.as_array()) {
                for item in items {
                    if let Ok(m) = serde_json::from_value::<ChatMember>(item.clone()) {
                        out.push(m);
                    }
                }
            }
            let next = resp
                .pointer("/data/page_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if next.is_empty() {
                break;
            }
            page_token = next;
        }
        Ok(out)
    }

    pub async fn list_files_in_folder(&self, folder_token: &str) -> Result<Vec<DriveFile>, String> {
        let mut out = Vec::new();
        let mut page_token = String::new();
        loop {
            let path = if page_token.is_empty() {
                format!("/open-apis/drive/v1/files?folder_token={folder_token}&page_size=200")
            } else {
                format!(
                    "/open-apis/drive/v1/files?folder_token={folder_token}&page_size=200&page_token={page_token}"
                )
            };
            let resp = self.call(Method::GET, &path, None).await?;
            if let Some(items) = resp.pointer("/data/files").and_then(|v| v.as_array()) {
                for item in items {
                    if let Ok(f) = serde_json::from_value::<DriveFile>(item.clone()) {
                        out.push(f);
                    }
                }
            }
            let next = resp
                .pointer("/data/next_page_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if next.is_empty() {
                break;
            }
            page_token = next;
        }
        Ok(out)
    }

    pub async fn create_docx_in_folder(
        &self,
        folder_token: &str,
        title: &str,
    ) -> Result<String, String> {
        let body = json!({ "folder_token": folder_token, "title": title });
        let resp = self
            .call(Method::POST, "/open-apis/docx/v1/documents", Some(&body))
            .await?;
        resp.pointer("/data/document/document_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| format!("no document_id: {resp}"))
    }

    /// List every block in a Docx document (paginated), returning raw JSON so
    /// callers can inspect table/cell/text structure as they please.
    pub async fn list_document_blocks(&self, document_id: &str) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        let mut page_token = String::new();
        loop {
            let path = if page_token.is_empty() {
                format!("/open-apis/docx/v1/documents/{document_id}/blocks?page_size=500")
            } else {
                format!(
                    "/open-apis/docx/v1/documents/{document_id}/blocks?page_size=500&page_token={page_token}"
                )
            };
            let resp = self.call(Method::GET, &path, None).await?;
            if let Some(items) = resp.pointer("/data/items").and_then(|v| v.as_array()) {
                out.extend(items.iter().cloned());
            }
            let next = resp
                .pointer("/data/page_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if next.is_empty() {
                break;
            }
            page_token = next;
        }
        Ok(out)
    }

    pub async fn insert_document_children(
        &self,
        document_id: &str,
        parent_block_id: &str,
        index: i64,
        children: Value,
    ) -> Result<Value, String> {
        let body = json!({ "index": index, "children": children });
        self.call(
            Method::POST,
            &format!(
                "/open-apis/docx/v1/documents/{document_id}/blocks/{parent_block_id}/children"
            ),
            Some(&body),
        )
        .await
    }

    pub async fn batch_update_document_blocks(
        &self,
        document_id: &str,
        requests: Value,
    ) -> Result<Value, String> {
        let body = json!({ "requests": requests });
        self.call(
            Method::PATCH,
            &format!("/open-apis/docx/v1/documents/{document_id}/blocks/batch_update"),
            Some(&body),
        )
        .await
    }

    /// Grant a chat group edit permission on a Drive file.
    pub async fn share_file_with_chat(
        &self,
        file_token: &str,
        file_type: &str,
        chat_id: &str,
    ) -> Result<(), String> {
        let body = json!({
            "member_type": "openchat",
            "member_id": chat_id,
            "perm": "edit",
        });
        self.call(
            Method::POST,
            &format!(
                "/open-apis/drive/v1/permissions/{file_token}/members?type={file_type}&need_notification=false"
            ),
            Some(&body),
        )
        .await
        .map(|_| ())
    }

    /// Send an interactive card to any receive_id (chat_id / open_id / user_id /
    /// email) and return the `message_id` for follow-up actions (urgent, edit).
    pub async fn send_interactive_returning_id(
        &self,
        receive_id: &str,
        receive_id_type: &str,
        card: &LarkCard,
    ) -> Result<String, String> {
        let body = json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": serde_json::to_string(card).unwrap_or_default(),
        });
        let resp = self
            .call(
                Method::POST,
                &format!("/open-apis/im/v1/messages?receive_id_type={receive_id_type}"),
                Some(&body),
            )
            .await?;
        resp.pointer("/data/message_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| format!("no message_id: {resp}"))
    }

    /// In-app urgent notification. `open_ids` are the users to escalate to.
    pub async fn urgent_app(&self, message_id: &str, open_ids: &[String]) -> Result<(), String> {
        let body = json!({ "user_id_list": open_ids });
        self.call(
            Method::PATCH,
            &format!("/open-apis/im/v1/messages/{message_id}/urgent_app?user_id_type=open_id"),
            Some(&body),
        )
        .await
        .map(|_| ())
    }

    /// Send a plain text message. `receive_id_type` is `chat_id` / `open_id` /
    /// `user_id` / `email`.
    pub async fn send_text(
        &self,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) -> Result<String, String> {
        let content = json!({ "text": text }).to_string();
        let body = json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": content,
        });
        let resp = self
            .call(
                Method::POST,
                &format!("/open-apis/im/v1/messages?receive_id_type={receive_id_type}"),
                Some(&body),
            )
            .await?;
        resp.pointer("/data/message_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| format!("no message_id: {resp}"))
    }

    /// Fetch the bot's own open_id (via `/bot/v3/info`). Note this endpoint
    /// returns data at the top-level `bot` key, not `data.bot`.
    pub async fn bot_open_id(&self) -> Result<String, String> {
        let token = self.get_token().await?;
        let url = format!("{}/open-apis/bot/v3/info", self.base_url());
        let resp: Value = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| format!("bot info failed: {e}"))?
            .json()
            .await
            .map_err(|e| format!("bot info decode: {e}"))?;
        let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(format!("bot info code {code}: {resp}"));
        }
        resp.pointer("/bot/open_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| format!("no bot.open_id: {resp}"))
    }
}
