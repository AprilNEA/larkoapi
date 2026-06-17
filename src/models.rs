//! Shared response model types for Lark server APIs.

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ChatMember {
    /// Usually an `open_id` (when `member_id_type` is `open_id`, which is the default).
    pub member_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tenant_key: String,
}

/// A user the bot can reach, from [`LarkBotClient::list_users`](crate::LarkBotClient::list_users):
/// a deduplicated chat member, addressable as a DM by its `open_id`.
#[derive(Debug, Clone)]
pub struct User {
    /// The user's app-scoped `open_id` (use as the DM `receive_id`).
    pub open_id: String,
    /// The user's display name.
    pub name: String,
}

/// A group chat the bot is a member of, from `GET /open-apis/im/v1/chats`.
#[derive(Debug, Deserialize, Clone)]
pub struct Chat {
    pub chat_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub avatar: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DriveFile {
    pub token: String,
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub parent_token: String,
}
