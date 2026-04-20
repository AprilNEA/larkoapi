//! Unofficial Minutes (妙记) web client — undocumented endpoints used by the
//! browser app, authenticated with a harvested cookie.
//!
//! ## Stability
//!
//! These endpoints are **not** part of Lark's public Open API. They can
//! change or disappear without notice. Reference implementation this was
//! ported from: <https://github.com/bingsanyu/feishu_minutes>.
//!
//! ## Layout
//!
//! - [`types`] — public data types ([`SpaceName`], [`SubtitleFormat`],
//!   [`SubtitleOptions`], [`MinutesWebRecord`], [`MinutesWebPage`]).
//! - [`cookie`] — cookie/JWT/URL helpers; [`infer_security_host_from_base`]
//!   is the one externally-exported item.
//! - [`client`] — [`MinutesWebClient`] struct and data-plane methods
//!   (`list_page`, `list_all`, `get_media_url`, `export_subtitle`).
//! - [`session`] — session management on the same struct (`refresh`,
//!   `session_expires_at`, `needs_refresh`, `reload_cookie`,
//!   `with_security_host`).
//!
//! ## Quick start
//!
//! ```no_run
//! # #[cfg(feature = "minutes-unofficial")]
//! # async fn demo() -> Result<(), String> {
//! use larkoapi::{MinutesWebClient, SpaceName, LARK_BASE, infer_security_host_from_base};
//! let http = reqwest::Client::new();
//! let cookie = std::env::var("LARK_MINUTES_COOKIE").unwrap();
//! let mut c = MinutesWebClient::new(LARK_BASE.into(), cookie, http)?;
//! if let Some(host) = infer_security_host_from_base(LARK_BASE) {
//!     c = c.with_security_host(host);
//! }
//! let page = c.list_page(SpaceName::Personal, 20, None).await?;
//! println!("got {} records", page.items.len());
//! # Ok(()) }
//! ```

/// Web base for Feishu China tenants.
pub const FEISHU_BASE: &str = "https://meetings.feishu.cn";
/// Web base for Lark International tenants.
pub const LARK_BASE: &str = "https://meetings.larksuite.com";

mod client;
mod cookie;
mod session;
mod types;

pub use client::MinutesWebClient;
pub use cookie::infer_security_host_from_base;
pub use types::{MinutesWebPage, MinutesWebRecord, SpaceName, SubtitleFormat, SubtitleOptions};
