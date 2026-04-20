//! Public data types for the unofficial Minutes client.

use serde::Deserialize;

/// Which "space" in the Minutes inbox to page through. `Personal` covers the
/// default "我的" space; shared spaces use their numeric ID.
#[derive(Debug, Clone, Copy)]
pub enum SpaceName {
    /// Personal space (`space_name=1`).
    Personal,
    /// Any other space by numeric ID.
    Other(u32),
}

impl SpaceName {
    pub(super) fn as_param(self) -> u32 {
        match self {
            SpaceName::Personal => 1,
            SpaceName::Other(n) => n,
        }
    }
}

/// Subtitle export format.
#[derive(Debug, Clone, Copy, Default)]
pub enum SubtitleFormat {
    /// Plain text.
    Txt,
    /// SubRip timed subtitles.
    #[default]
    Srt,
}

impl SubtitleFormat {
    pub(super) fn as_param(self) -> u8 {
        match self {
            Self::Txt => 2,
            Self::Srt => 3,
        }
    }
}

/// Options for the `export` endpoint.
#[derive(Debug, Clone)]
pub struct SubtitleOptions {
    pub format: SubtitleFormat,
    pub include_speaker: bool,
    pub include_timestamp: bool,
}

impl Default for SubtitleOptions {
    fn default() -> Self {
        Self {
            format: SubtitleFormat::Srt,
            include_speaker: true,
            include_timestamp: true,
        }
    }
}

/// A single row from the Minutes inbox.
#[derive(Debug, Clone, Deserialize)]
pub struct MinutesWebRecord {
    /// Minute identifier — pass this to `get_media_url` / `export_subtitle`.
    pub object_token: String,
    /// Source kind: `0` = meeting recording, others = uploaded file.
    #[serde(default)]
    pub object_type: i32,
    #[serde(default)]
    pub topic: String,
    /// Meeting start, unix milliseconds. `0` for uploads.
    #[serde(default)]
    pub start_time: u64,
    /// Meeting end, unix milliseconds. `0` for uploads.
    #[serde(default)]
    pub stop_time: u64,
    /// Minute creation time, unix milliseconds.
    #[serde(default)]
    pub create_time: u64,
    /// Cursor value for the next page (share_time of this row).
    #[serde(default)]
    pub share_time: Option<u64>,
}

impl MinutesWebRecord {
    /// `true` when this minute originated from a meeting recording.
    pub fn is_meeting(&self) -> bool {
        self.object_type == 0
    }
}

/// One page of [`MinutesWebRecord`] plus pagination hints.
#[derive(Debug, Clone)]
pub struct MinutesWebPage {
    pub items: Vec<MinutesWebRecord>,
    /// `true` if another page exists. Use the last item's `share_time` as the
    /// next `cursor` value passed into `MinutesWebClient::list_page`.
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_param_encoding() {
        assert_eq!(SpaceName::Personal.as_param(), 1);
        assert_eq!(SpaceName::Other(42).as_param(), 42);
    }

    #[test]
    fn subtitle_param_encoding() {
        assert_eq!(SubtitleFormat::Txt.as_param(), 2);
        assert_eq!(SubtitleFormat::Srt.as_param(), 3);
        assert!(matches!(SubtitleFormat::default(), SubtitleFormat::Srt));
    }

    #[test]
    fn subtitle_options_default() {
        let o = SubtitleOptions::default();
        assert!(matches!(o.format, SubtitleFormat::Srt));
        assert!(o.include_speaker);
        assert!(o.include_timestamp);
    }

    #[test]
    fn record_parses_meeting() {
        let j = serde_json::json!({
            "object_token": "tok_xyz",
            "object_type": 0,
            "topic": "Weekly Sync",
            "start_time": 1712000000000_u64,
            "stop_time": 1712003600000_u64,
            "create_time": 1712003700000_u64,
            "share_time": 1712003700000_u64,
        });
        let r: MinutesWebRecord = serde_json::from_value(j).unwrap();
        assert_eq!(r.object_token, "tok_xyz");
        assert!(r.is_meeting());
        assert_eq!(r.stop_time - r.start_time, 3_600_000);
        assert_eq!(r.share_time, Some(1712003700000));
    }

    #[test]
    fn record_parses_upload_with_missing_fields() {
        let j = serde_json::json!({
            "object_token": "tok_up",
            "object_type": 1,
            "topic": "Ad-hoc upload",
            "create_time": 1712000000000_u64,
        });
        let r: MinutesWebRecord = serde_json::from_value(j).unwrap();
        assert!(!r.is_meeting());
        assert_eq!(r.start_time, 0);
        assert_eq!(r.stop_time, 0);
        assert!(r.share_time.is_none());
    }
}
