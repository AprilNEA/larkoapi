//! Cookie / JWT / URL helpers used by the unofficial Minutes client.
//!
//! Most items are `pub(super)` — consumed by sibling modules. The one
//! externally-exported helper is [`infer_security_host_from_base`], which
//! callers need to resolve the compliance-ping host for `refresh()`.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Best-effort derivation of the security/compliance host from a tenant
/// base URL. Matches the Lark International pattern
/// `https://<tenant>.<region>.larksuite.com` → `https://internal-api-security-<region>.larksuite.com`.
///
/// Returns `None` when the host doesn't match a known pattern; in that case
/// capture the actual endpoint from your browser and pass it explicitly.
pub fn infer_security_host_from_base(base: &str) -> Option<String> {
    let rest = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .unwrap_or(base);
    let host = rest.trim_end_matches('/').split('/').next()?;
    let parts: Vec<&str> = host.split('.').collect();
    // expect: [tenant, region, larksuite, com]
    if parts.len() == 4 && parts[2] == "larksuite" && parts[3] == "com" {
        let region = parts[1];
        return Some(format!(
            "https://internal-api-security-{region}.larksuite.com"
        ));
    }
    // Fallback shape: [tenant, larksuite, com] — no region suffix.
    if parts.len() == 3 && parts[1] == "larksuite" && parts[2] == "com" {
        return Some("https://internal-api-security.larksuite.com".to_string());
    }
    None
}

pub(super) fn extract_csrf(cookie: &str) -> Option<String> {
    let key = "bv_csrf_token=";
    let start = cookie.find(key)? + key.len();
    let rest = &cookie[start..];
    let end = rest.find(';').unwrap_or(rest.len());
    let token = rest[..end].trim().to_string();
    if token.len() == 36 { Some(token) } else { None }
}

pub(super) fn extract_sl_session_exp(cookie: &str) -> Option<SystemTime> {
    const KEY: &str = "sl_session=";
    let start = cookie.find(KEY)? + KEY.len();
    let rest = &cookie[start..];
    let end = rest.find(';').unwrap_or(rest.len());
    let jwt = rest[..end].trim();

    let mut parts = jwt.split('.');
    let (_hdr, payload, _sig) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }

    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let exp = v.get("exp")?.as_u64()?;
    Some(UNIX_EPOCH + Duration::from_secs(exp))
}

/// Returns the value for a single `Set-Cookie` header if its first
/// key-value pair matches `name`.
pub(super) fn parse_set_cookie_value(set_cookie: &str, name: &str) -> Option<String> {
    let first = set_cookie.split(';').next()?.trim();
    let (k, v) = first.split_once('=')?;
    if k.trim() == name {
        Some(v.trim().to_string())
    } else {
        None
    }
}

/// Replace (or append) `name=new_value` in a serialized Cookie header string.
pub(super) fn replace_cookie_value(cookie: &str, name: &str, new_value: &str) -> String {
    let mut pieces: Vec<String> = Vec::new();
    let mut found = false;
    for piece in cookie.split(';') {
        let trimmed = piece.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((k, _)) = trimmed.split_once('=')
            && k.trim() == name
        {
            pieces.push(format!("{name}={new_value}"));
            found = true;
            continue;
        }
        pieces.push(trimmed.to_string());
    }
    if !found {
        pieces.push(format!("{name}={new_value}"));
    }
    pieces.join("; ")
}

/// The backend expects Python-style capitalised `True` / `False` on the
/// subtitle export endpoint, because the reference client relies on
/// `requests`' default `str(bool)` serialization.
pub(super) fn bool_cn(b: bool) -> &'static str {
    if b { "True" } else { "False" }
}

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_TOKEN: &str = "abcdefgh-1234-5678-9abc-def012345678"; // 36 chars

    #[test]
    fn csrf_found_middle() {
        let c = format!("foo=1; bv_csrf_token={GOOD_TOKEN}; bar=2");
        assert_eq!(extract_csrf(&c).unwrap(), GOOD_TOKEN);
    }

    #[test]
    fn csrf_found_at_end() {
        let c = format!("foo=1; bv_csrf_token={GOOD_TOKEN}");
        assert_eq!(extract_csrf(&c).unwrap(), GOOD_TOKEN);
    }

    #[test]
    fn csrf_missing() {
        assert!(extract_csrf("foo=1; bar=2").is_none());
    }

    #[test]
    fn csrf_wrong_length_rejected() {
        assert!(extract_csrf("bv_csrf_token=too_short;").is_none());
    }

    #[test]
    fn bool_cn_encoding() {
        assert_eq!(bool_cn(true), "True");
        assert_eq!(bool_cn(false), "False");
    }

    #[test]
    fn parse_set_cookie_matches_name() {
        let h = "sl_session=eyJhbGc.xxx; max-age=43200; domain=larksuite.com; path=/; HttpOnly";
        assert_eq!(
            parse_set_cookie_value(h, "sl_session"),
            Some("eyJhbGc.xxx".into())
        );
        assert_eq!(parse_set_cookie_value(h, "other"), None);
    }

    #[test]
    fn replace_cookie_updates_existing() {
        let c = "a=1; sl_session=OLD; b=2";
        assert_eq!(
            replace_cookie_value(c, "sl_session", "NEW"),
            "a=1; sl_session=NEW; b=2"
        );
    }

    #[test]
    fn replace_cookie_appends_when_missing() {
        let c = "a=1; b=2";
        assert_eq!(
            replace_cookie_value(c, "sl_session", "NEW"),
            "a=1; b=2; sl_session=NEW"
        );
    }

    #[test]
    fn infer_host_regional() {
        assert_eq!(
            infer_security_host_from_base("https://djp85thjlit4.jp.larksuite.com"),
            Some("https://internal-api-security-jp.larksuite.com".to_string())
        );
        assert_eq!(
            infer_security_host_from_base("https://tenant.sg.larksuite.com/"),
            Some("https://internal-api-security-sg.larksuite.com".to_string())
        );
    }

    #[test]
    fn infer_host_no_region() {
        assert_eq!(
            infer_security_host_from_base("https://meetings.larksuite.com"),
            Some("https://internal-api-security.larksuite.com".to_string())
        );
    }

    #[test]
    fn infer_host_unknown() {
        assert_eq!(
            infer_security_host_from_base("https://meetings.feishu.cn"),
            None
        );
    }

    #[test]
    fn sl_session_exp_roundtrips() {
        let exp = 2_000_000_000_u64;
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{exp}}}"#).as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(b"sig");
        let cookie = format!("a=b; sl_session={header}.{payload}.{sig}; c=d");
        let got = extract_sl_session_exp(&cookie).unwrap();
        assert_eq!(got, UNIX_EPOCH + Duration::from_secs(exp));
    }

    #[test]
    fn sl_session_exp_missing_returns_none() {
        assert!(extract_sl_session_exp("a=b; c=d").is_none());
    }
}
