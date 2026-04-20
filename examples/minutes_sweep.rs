//! Live smoke test for the unofficial Minutes web client.
//!
//! Cookie source — pick one, in priority order:
//!   1. `LARK_MINUTES_COOKIE_FILE` — path to a mode-0600 file whose contents
//!      are the raw Cookie header. Preferred: the cookie never touches env
//!      vars or shell history.
//!   2. `LARK_MINUTES_COOKIE` — raw cookie string. Handy for one-off runs.
//!      Quote with SINGLE quotes in the shell so embedded `"` stay literal:
//!          export LARK_MINUTES_COOKIE='name="value"; other=x'
//!
//! Other env:
//!   `LARK_MINUTES_BASE`  — `https://meetings.feishu.cn` (default) or
//!                          `https://meetings.larksuite.com`
//!   `LARK_MINUTES_LIMIT` — rows to print (default 5)
//!
//! ```sh
//! # Preferred file-based setup:
//! umask 077
//! pbpaste > ~/.lark-minutes-cookie   # or: cat > ~/.lark-minutes-cookie
//! chmod 600 ~/.lark-minutes-cookie
//! export LARK_MINUTES_COOKIE_FILE=~/.lark-minutes-cookie
//!
//! cargo run --example minutes_sweep --features minutes-unofficial
//! ```
//!
//! Exits 0 on success, prints: page size, each record one per line, media URL
//! for the first record, and a 500-char SRT preview.

#[cfg(feature = "minutes-unofficial")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(run())
}

#[cfg(not(feature = "minutes-unofficial"))]
fn main() {
    eprintln!(
        "this example requires the `minutes-unofficial` feature:\n  cargo run --example minutes_sweep --features minutes-unofficial"
    );
    std::process::exit(2);
}

#[cfg(feature = "minutes-unofficial")]
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    use larkoapi::minutes::{
        FEISHU_BASE, MinutesWebClient, SpaceName, SubtitleOptions, infer_security_host_from_base,
    };

    let cookie = load_cookie()?;
    let base = std::env::var("LARK_MINUTES_BASE").unwrap_or_else(|_| FEISHU_BASE.to_string());
    let limit: usize = std::env::var("LARK_MINUTES_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let http = reqwest::Client::new();
    let mut client = MinutesWebClient::new(base.clone(), cookie, http)?;

    // Demonstrate the refresh path if we can resolve the security host.
    let security_host = std::env::var("LARK_MINUTES_SECURITY_HOST")
        .ok()
        .or_else(|| infer_security_host_from_base(&base));
    if let Some(host) = security_host {
        client = client.with_security_host(host.clone());
        if let Some(exp) = client.session_expires_at() {
            let secs = exp
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            println!("sl_session before refresh: exp={secs} (unix)");
        }
        match client.refresh().await {
            Ok(()) => {
                if let Some(exp) = client.session_expires_at() {
                    let secs = exp
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    println!("sl_session after refresh:  exp={secs} (unix) via {host}");
                }
            }
            Err(e) => println!("refresh FAILED (continuing): {e}"),
        }
    } else {
        println!("(security_host not resolvable; skipping refresh demo)");
    }

    let page = client.list_page(SpaceName::Personal, 20, None).await?;
    println!(
        "page: {} rows, has_more={}",
        page.items.len(),
        page.has_more
    );
    for (i, r) in page.items.iter().take(limit).enumerate() {
        let duration_ms = r.stop_time.saturating_sub(r.start_time);
        println!(
            "  [{i}] token={} kind={} duration={}ms topic={:?}",
            r.object_token, r.object_type, duration_ms, r.topic
        );
    }

    // Try each record in turn — longer meetings are more likely to have a
    // transcript populated. Stops after the first non-empty SRT.
    let mut transcript_printed = false;
    for r in page.items.iter().take(limit) {
        println!(
            "\n--- {} ({}ms) ---",
            r.object_token,
            r.stop_time.saturating_sub(r.start_time)
        );
        match client.get_media_url(&r.object_token).await {
            Ok(url) => println!("  media URL: {} chars", url.len()),
            Err(e) => println!("  media URL FAILED: {e}"),
        }
        match client
            .export_subtitle(&r.object_token, &SubtitleOptions::default())
            .await
        {
            Ok(body) if body.is_empty() => {
                println!("  subtitle: empty (probably no transcript generated for this minute)")
            }
            Ok(body) => {
                let preview: String = body.chars().take(500).collect();
                println!("  subtitle: {} chars, first 500:\n{preview}", body.len());
                transcript_printed = true;
                break;
            }
            Err(e) => println!("  subtitle FAILED: {e}"),
        }
    }
    if !transcript_printed {
        println!("\nno non-empty transcripts in sampled rows");
    }

    Ok(())
}

#[cfg(feature = "minutes-unofficial")]
fn load_cookie() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(path) = std::env::var("LARK_MINUTES_COOKIE_FILE") {
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("read LARK_MINUTES_COOKIE_FILE={path}: {e}"))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(format!("{path} is empty").into());
        }
        #[cfg(unix)]
        warn_if_world_readable(&path);
        return Ok(trimmed.to_string());
    }
    if let Ok(s) = std::env::var("LARK_MINUTES_COOKIE") {
        if s.trim().is_empty() {
            return Err("LARK_MINUTES_COOKIE is empty".into());
        }
        return Ok(s);
    }
    Err("set LARK_MINUTES_COOKIE_FILE (preferred) or LARK_MINUTES_COOKIE".into())
}

#[cfg(all(feature = "minutes-unofficial", unix))]
fn warn_if_world_readable(path: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            eprintln!(
                "warning: {path} mode is {mode:o} — run `chmod 600 {path}` to restrict access"
            );
        }
    }
}
