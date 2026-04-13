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
