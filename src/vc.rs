//! Video Conferencing v1 — meeting metadata and recording access.
//!
//! Scopes: `vc:meeting:readonly`, `vc:record:readonly`.
//! Tenant access tokens reach any meeting in the org; user tokens are limited
//! to the meetings they own.

use reqwest::Method;
use serde::Deserialize;

use crate::client::LarkBotClient;

/// Meeting metadata returned by [`LarkBotClient::get_meeting`].
#[derive(Debug, Clone)]
pub struct MeetingMeta {
    pub topic: String,
    /// Unix milliseconds. `0` when the wire value was missing/unparseable.
    pub start_time_ms: u64,
    pub end_time_ms: u64,
    /// Owner's `open_id`, when the API returned one.
    pub owner_open_id: Option<String>,
}

/// Recording file descriptor returned by [`LarkBotClient::get_recording`].
#[derive(Debug, Clone)]
pub struct RecordingFile {
    /// Signed URL to the recording media. Validity window is set by Lark.
    pub url: String,
    /// Total recording duration, in milliseconds.
    pub duration_ms: u64,
}

#[derive(Default, Deserialize)]
struct MeetingEnvelope {
    #[serde(default)]
    meeting: MeetingWire,
}

#[derive(Default, Deserialize)]
struct MeetingWire {
    #[serde(default)]
    topic: String,
    #[serde(default)]
    start_time: String,
    #[serde(default)]
    end_time: String,
    #[serde(default)]
    owner: Option<OwnerWire>,
}

#[derive(Default, Deserialize)]
struct OwnerWire {
    #[serde(default)]
    id: String,
}

#[derive(Default, Deserialize)]
struct RecordingEnvelope {
    #[serde(default)]
    recording: RecordingWire,
}

#[derive(Default, Deserialize)]
struct RecordingWire {
    #[serde(default)]
    url: String,
    #[serde(default)]
    duration: String,
}

impl LarkBotClient {
    /// `GET /open-apis/vc/v1/meetings/:meeting_id?user_id_type=open_id`
    pub async fn get_meeting(&self, meeting_id: &str) -> Result<MeetingMeta, String> {
        let resp = self
            .call(
                Method::GET,
                &format!("/open-apis/vc/v1/meetings/{meeting_id}?user_id_type=open_id"),
                None,
            )
            .await?;
        let env: MeetingEnvelope = parse_data(&resp, "meeting")?;
        Ok(MeetingMeta {
            topic: env.meeting.topic,
            start_time_ms: env.meeting.start_time.parse().unwrap_or(0),
            end_time_ms: env.meeting.end_time.parse().unwrap_or(0),
            owner_open_id: env.meeting.owner.map(|o| o.id).filter(|s| !s.is_empty()),
        })
    }

    /// `GET /open-apis/vc/v1/meetings/:meeting_id/recording`
    ///
    /// Fails with an API error if no recording exists yet (meeting still
    /// uploading, too short, or recording never started).
    pub async fn get_recording(&self, meeting_id: &str) -> Result<RecordingFile, String> {
        let resp = self
            .call(
                Method::GET,
                &format!("/open-apis/vc/v1/meetings/{meeting_id}/recording"),
                None,
            )
            .await?;
        let env: RecordingEnvelope = parse_data(&resp, "recording")?;
        Ok(RecordingFile {
            url: env.recording.url,
            duration_ms: env.recording.duration.parse().unwrap_or(0),
        })
    }
}

fn parse_data<T: for<'de> Deserialize<'de> + Default>(
    resp: &serde_json::Value,
    label: &str,
) -> Result<T, String> {
    let data = resp.get("data").cloned().unwrap_or_default();
    serde_json::from_value(data).map_err(|e| format!("parse {label}: {e}"))
}
