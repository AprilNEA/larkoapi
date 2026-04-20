//! Session management for [`MinutesWebClient`] — JWT expiry inspection,
//! cookie-file reload, and the compliance-ping refresh path.

use std::time::{Duration, SystemTime};

use super::client::MinutesWebClient;
use super::cookie::{
    extract_csrf, extract_sl_session_exp, parse_set_cookie_value, replace_cookie_value,
};

impl MinutesWebClient {
    /// Set the security/compliance host used by [`refresh`](Self::refresh).
    /// For Lark International with a regional tenant this looks like
    /// `https://internal-api-security-jp.larksuite.com`.
    ///
    /// Prefer [`super::infer_security_host_from_base`] to derive it from the
    /// tenant base URL automatically.
    pub fn with_security_host(mut self, host: impl Into<String>) -> Self {
        self.security_host = Some(host.into());
        self
    }

    /// Trigger Lark's compliance-ping heartbeat to rotate `sl_session`.
    ///
    /// Calls `GET {security_host}/lark/scs/compliance/ping`, which re-issues a
    /// fresh 12-hour `sl_session` via `Set-Cookie`. The new value replaces
    /// `sl_session` in the in-memory cookie string.
    ///
    /// Requires [`with_security_host`](Self::with_security_host) to have been
    /// called. Returns an error if the response was successful but did not
    /// contain a new `sl_session`.
    pub async fn refresh(&mut self) -> Result<(), String> {
        let host = self
            .security_host
            .as_deref()
            .ok_or("refresh: security_host not set (call with_security_host first)")?;
        let url = format!("{}/lark/scs/compliance/ping", host.trim_end_matches('/'));
        let origin = self.base.trim_end_matches('/').to_string();
        let referer = format!("{origin}/");

        let resp = self
            .http
            .get(&url)
            .header("Cookie", &self.cookie)
            .header("Origin", &origin)
            .header("Referer", &referer)
            .header("Accept", "*/*")
            .header("x-lsc-bizid", "16")
            .header("x-lsc-terminal", "web")
            .header("x-lsc-version", "1")
            .header("x-lgw-os-type", "3")
            .header("x-lgw-terminal-type", "2")
            .send()
            .await
            .map_err(|e| format!("refresh http: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("refresh HTTP {status}: {body}"));
        }

        let mut new_sl: Option<String> = None;
        for header in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(h) = header.to_str()
                && let Some(v) = parse_set_cookie_value(h, "sl_session")
            {
                new_sl = Some(v);
                break;
            }
        }
        let new_sl = new_sl.ok_or("refresh ok but response had no sl_session set-cookie")?;

        self.cookie = replace_cookie_value(&self.cookie, "sl_session", &new_sl);
        Ok(())
    }

    /// Parse the `sl_session` JWT `exp` claim from the current cookie.
    /// Returns `None` when the cookie has no recognisable `sl_session` JWT.
    ///
    /// JWT payloads are public by design (only the signature needs the
    /// server's key), so this is a pure local decode.
    pub fn session_expires_at(&self) -> Option<SystemTime> {
        extract_sl_session_exp(&self.cookie)
    }

    /// `true` when the session is past `exp`, or within `buffer` of it.
    /// Unknown / unparseable cookies are treated as "needs refresh" so callers
    /// err on the side of re-harvesting.
    pub fn needs_refresh(&self, buffer: Duration) -> bool {
        match self.session_expires_at() {
            None => true,
            Some(exp) => exp <= SystemTime::now() + buffer,
        }
    }

    /// Replace the in-flight cookie with a fresh one (e.g. after a cron job
    /// re-harvested from Chrome's cookie DB). Rebuilds the CSRF header.
    pub fn reload_cookie(&mut self, new_cookie: String) -> Result<(), String> {
        let csrf = extract_csrf(&new_cookie)
            .ok_or_else(|| "reload: cookie missing bv_csrf_token".to_string())?;
        self.cookie = new_cookie;
        self.csrf = csrf;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use reqwest::Client;
    use std::time::UNIX_EPOCH;

    const GOOD_TOKEN: &str = "abcdefgh-1234-5678-9abc-def012345678";

    fn fake_jwt(exp: u64) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{exp},"sub":"x"}}"#).as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(b"sig");
        format!("{header}.{payload}.{sig}")
    }

    fn make_client(cookie: String, security_host: Option<String>) -> MinutesWebClient {
        MinutesWebClient {
            base: "https://x".into(),
            cookie,
            csrf: GOOD_TOKEN.into(),
            security_host,
            http: Client::new(),
        }
    }

    #[test]
    fn session_expires_at_parses_future() {
        let client = make_client(format!("sl_session={}", fake_jwt(2_000_000_000)), None);
        let exp = client.session_expires_at().unwrap();
        assert_eq!(exp, UNIX_EPOCH + Duration::from_secs(2_000_000_000));
    }

    #[test]
    fn session_expires_at_missing() {
        let client = make_client("a=b".into(), None);
        assert!(client.session_expires_at().is_none());
    }

    #[test]
    fn needs_refresh_future_ok() {
        let future = (SystemTime::now() + Duration::from_secs(3600))
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let client = make_client(format!("sl_session={}", fake_jwt(future)), None);
        assert!(!client.needs_refresh(Duration::from_secs(60)));
        assert!(client.needs_refresh(Duration::from_secs(7200)));
    }

    #[test]
    fn needs_refresh_past_true() {
        let client = make_client(format!("sl_session={}", fake_jwt(1_000_000_000)), None);
        assert!(client.needs_refresh(Duration::from_secs(0)));
    }

    #[test]
    fn needs_refresh_unknown_true() {
        let client = make_client("no_jwt=1".into(), None);
        assert!(client.needs_refresh(Duration::from_secs(0)));
    }

    #[test]
    fn reload_cookie_rebuilds_csrf() {
        let mut client = make_client(format!("bv_csrf_token={GOOD_TOKEN}"), None);
        let new_token = "11112222-3333-4444-5555-666677778888";
        let new = format!("foo=bar; bv_csrf_token={new_token}; baz=qux");
        client.reload_cookie(new.clone()).unwrap();
        assert_eq!(client.cookie, new);
        assert_eq!(client.csrf, new_token);
    }

    #[test]
    fn reload_cookie_rejects_missing_csrf() {
        let mut client = make_client(format!("bv_csrf_token={GOOD_TOKEN}"), None);
        assert!(client.reload_cookie("no_csrf=here".into()).is_err());
    }

    #[test]
    fn refresh_requires_security_host() {
        let mut client = make_client(format!("bv_csrf_token={GOOD_TOKEN}"), None);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt.block_on(client.refresh()).unwrap_err();
        assert!(err.contains("security_host"));
    }

    #[test]
    fn with_security_host_sets_field() {
        let client = make_client(format!("bv_csrf_token={GOOD_TOKEN}"), None)
            .with_security_host("https://internal-api-security-jp.larksuite.com");
        assert_eq!(
            client.security_host.as_deref(),
            Some("https://internal-api-security-jp.larksuite.com")
        );
    }
}
