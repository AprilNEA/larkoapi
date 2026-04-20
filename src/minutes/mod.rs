//! Minutes (妙记) — both the official Open API client and the opt-in
//! unofficial web-session scraper.
//!
//! - [`official`] is always compiled; it wraps the documented
//!   `/open-apis/minutes/v1/*` endpoints that only expose metadata and the
//!   media download URL.
//! - [`unofficial`] is gated behind the `minutes-unofficial` feature. It
//!   talks to the undocumented `meetings.{feishu,larksuite}.cn/minutes/api/*`
//!   endpoints used by the web app, authenticated with a browser cookie.
//!   It covers far more (every minute the user can see, SRT transcript,
//!   AI summary), but is fragile and can break without notice.

pub mod official;

#[cfg(feature = "minutes-unofficial")]
pub mod unofficial;

pub use official::MinuteMeta;

#[cfg(feature = "minutes-unofficial")]
pub use unofficial::{
    FEISHU_BASE, LARK_BASE, MinutesWebClient, MinutesWebPage, MinutesWebRecord, SpaceName,
    SubtitleFormat, SubtitleOptions, infer_security_host_from_base,
};
