use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use prost::Message;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite;
use tracing::{error, info, warn};

pub mod pbbp2 {
    include!(concat!(env!("OUT_DIR"), "/pbbp2.rs"));
}

const ENDPOINT_PATH: &str = "/callback/ws/endpoint";
const METHOD_CONTROL: i32 = 0;
const METHOD_DATA: i32 = 1;

/// Trait for handling Lark WebSocket events.
///
/// Implementors receive parsed event JSON and may return an optional
/// card callback response (for in-place card updates within 3s).
#[async_trait]
pub trait WsEventHandler: Send + Sync + 'static {
    async fn handle_event(&self, event: &Value) -> Option<Value>;
}

#[derive(Deserialize)]
struct EndpointResponse {
    code: i64,
    msg: String,
    data: Option<EndpointData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EndpointData {
    #[serde(rename = "URL")]
    url: String,
    client_config: Option<ClientConfig>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
struct ClientConfig {
    #[serde(default = "default_reconnect_count")]
    reconnect_count: i64,
    #[serde(default = "default_reconnect_interval")]
    reconnect_interval: u64,
    #[serde(default = "default_reconnect_nonce")]
    #[allow(dead_code)]
    reconnect_nonce: u64,
    #[serde(default = "default_ping_interval")]
    ping_interval: u64,
}

fn default_reconnect_count() -> i64 {
    120
}
fn default_reconnect_interval() -> u64 {
    3
}
fn default_reconnect_nonce() -> u64 {
    30
}
fn default_ping_interval() -> u64 {
    120
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            reconnect_count: default_reconnect_count(),
            reconnect_interval: default_reconnect_interval(),
            reconnect_nonce: default_reconnect_nonce(),
            ping_interval: default_ping_interval(),
        }
    }
}

/// Run the Lark/Feishu WebSocket long connection client.
///
/// Connects to the Feishu WebSocket endpoint, handles ping/pong, message
/// fragmentation, deduplication, and dispatches events to the provided handler.
/// Automatically reconnects on disconnection.
pub async fn run_ws_client(
    base_url: &str,
    app_id: &str,
    app_secret: &str,
    handler: Arc<dyn WsEventHandler>,
    http: reqwest::Client,
) {
    let mut config = ClientConfig::default();
    let mut attempts: i64 = 0;

    loop {
        if config.reconnect_count >= 0 && attempts > config.reconnect_count {
            error!(
                "exceeded max reconnect attempts ({})",
                config.reconnect_count
            );
            return;
        }

        if attempts > 0 {
            let delay = config.reconnect_interval.max(1).min(10);
            info!("reconnecting in {delay}s (attempt {attempts})");
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }

        // Step 1: Get WSS URL
        let endpoint = format!("{base_url}{ENDPOINT_PATH}");
        let wss_url = match get_ws_endpoint(&http, &endpoint, app_id, app_secret).await {
            Ok((url, cfg)) => {
                config = cfg;
                attempts = 0;
                url
            }
            Err(e) => {
                error!("failed to get ws endpoint: {e}");
                attempts += 1;
                continue;
            }
        };

        // Extract service_id from URL
        let service_id = extract_query_param(&wss_url, "service_id")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);

        info!("connecting to feishu websocket");

        // Step 2: Connect WebSocket
        let ws_stream = match tokio_tungstenite::connect_async(&wss_url).await {
            Ok((stream, _)) => {
                info!("feishu websocket connected");
                stream
            }
            Err(e) => {
                error!("websocket connect failed: {e}");
                attempts += 1;
                continue;
            }
        };

        // Step 3: Message loop
        let result = ws_message_loop(ws_stream, service_id, &config, &handler).await;
        if let Err(e) = result {
            warn!("websocket disconnected: {e}");
        }
        attempts += 1;
    }
}

async fn get_ws_endpoint(
    http: &reqwest::Client,
    endpoint: &str,
    app_id: &str,
    app_secret: &str,
) -> Result<(String, ClientConfig), String> {
    let resp = http
        .post(endpoint)
        .header("locale", "zh")
        .json(&json!({
            "AppID": app_id,
            "AppSecret": app_secret,
        }))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let body: EndpointResponse = resp
        .json()
        .await
        .map_err(|e| format!("parse failed: {e}"))?;

    if body.code != 0 {
        return Err(format!("endpoint error {}: {}", body.code, body.msg));
    }

    let data = body.data.ok_or("missing data in response")?;
    let config = data.client_config.unwrap_or_default();
    Ok((data.url, config))
}

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn ws_message_loop(
    ws_stream: WsStream,
    service_id: i32,
    config: &ClientConfig,
    handler: &Arc<dyn WsEventHandler>,
) -> Result<(), String> {
    let (mut write, mut read): (
        futures_util::stream::SplitSink<WsStream, tungstenite::Message>,
        futures_util::stream::SplitStream<WsStream>,
    ) = ws_stream.split();
    let ping_interval = Duration::from_secs(config.ping_interval);
    let mut last_ping = Instant::now();

    // Fragment cache: message_id -> (total, collected fragments)
    let mut fragments: HashMap<String, (usize, Vec<(usize, Vec<u8>)>)> = HashMap::new();

    // Dedup: track recent event_ids to skip duplicates
    let mut seen_events: HashMap<String, Instant> = HashMap::new();

    loop {
        // Check if ping needed
        if last_ping.elapsed() >= ping_interval {
            let ping_frame = build_ping_frame(service_id);
            let data = encode_frame(&ping_frame);
            write
                .send(tungstenite::Message::Binary(data.into()))
                .await
                .map_err(|e| format!("ping send failed: {e}"))?;
            last_ping = Instant::now();
        }

        let msg = tokio::select! {
            msg = read.next() => msg,
            _ = tokio::time::sleep(ping_interval) => {
                continue;
            }
        };

        let msg = match msg {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(format!("read error: {e}")),
            None => return Err("connection closed".into()),
        };

        let data = match msg {
            tungstenite::Message::Binary(data) => data,
            tungstenite::Message::Close(_) => return Err("server closed connection".into()),
            _ => continue,
        };

        let frame = match pbbp2::Frame::decode(data.as_ref()) {
            Ok(f) => f,
            Err(e) => {
                warn!("failed to decode frame: {e}");
                continue;
            }
        };

        let headers = frame_headers(&frame);
        let frame_type = headers.get("type").map(|s| s.as_str()).unwrap_or("");

        match (frame.method, frame_type) {
            (METHOD_CONTROL, "pong") => {
                if let Some(ref payload) = frame.payload {
                    if !payload.is_empty() {
                        if let Ok(new_config) = serde_json::from_slice::<ClientConfig>(payload) {
                            info!(
                                "config updated from pong: ping_interval={}s",
                                new_config.ping_interval
                            );
                        }
                    }
                }
            }
            (METHOD_DATA, "event") | (METHOD_DATA, "card") => {
                let message_id = headers.get("message_id").cloned().unwrap_or_default();
                let sum: usize = headers.get("sum").and_then(|s| s.parse().ok()).unwrap_or(1);
                let seq: usize = headers.get("seq").and_then(|s| s.parse().ok()).unwrap_or(0);

                let payload = if sum <= 1 {
                    frame.payload.clone().unwrap_or_default()
                } else {
                    let entry = fragments
                        .entry(message_id.clone())
                        .or_insert_with(|| (sum, Vec::new()));
                    entry
                        .1
                        .push((seq, frame.payload.clone().unwrap_or_default()));

                    if entry.1.len() >= sum {
                        let mut parts = fragments.remove(&message_id).unwrap().1;
                        parts.sort_by_key(|(s, _)| *s);
                        parts.into_iter().flat_map(|(_, d)| d).collect()
                    } else {
                        continue;
                    }
                };

                // Parse payload JSON
                let event: Value = match serde_json::from_slice(&payload) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("failed to parse event payload: {e}");
                        continue;
                    }
                };

                // Dedup by event_id + event_type
                let event_id = event
                    .pointer("/header/event_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let event_type = event
                    .pointer("/header/event_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let dedup_key = format!("{event_id}:{event_type}");
                if !event_id.is_empty() {
                    if seen_events.contains_key(&dedup_key) {
                        tracing::debug!(event_id, event_type, "duplicate event, skipping");
                        let ack = build_ack_frame(&frame, "0");
                        let _ = write
                            .send(tungstenite::Message::Binary(encode_frame(&ack).into()))
                            .await;
                        continue;
                    }
                    seen_events.insert(dedup_key, Instant::now());
                    // Prune old entries (keep last 5 minutes)
                    seen_events.retain(|_, t| t.elapsed() < Duration::from_secs(300));
                }

                // Process event
                let start = Instant::now();
                let card_response = handler.handle_event(&event).await;
                let biz_rt = start.elapsed().as_millis().to_string();

                // Send ACK (with optional card response payload)
                let ack = if let Some(resp) = card_response {
                    build_ack_frame_with_response(&frame, &biz_rt, &resp)
                } else {
                    build_ack_frame(&frame, &biz_rt)
                };
                let ack_data = encode_frame(&ack);
                if let Err(e) = write
                    .send(tungstenite::Message::Binary(ack_data.into()))
                    .await
                {
                    warn!("failed to send ack: {e}");
                }
            }
            _ => {
                if frame.method == METHOD_DATA {
                    tracing::debug!(
                        frame_type,
                        method = frame.method,
                        "unhandled data frame type"
                    );
                }
            }
        }

        // Clean stale fragments
        fragments.retain(|_, _| true);
    }
}

fn build_ping_frame(service_id: i32) -> pbbp2::Frame {
    pbbp2::Frame {
        seq_id: 0,
        log_id: 0,
        service: service_id,
        method: METHOD_CONTROL,
        headers: vec![pbbp2::Header {
            key: "type".into(),
            value: "ping".into(),
        }],
        payload_encoding: None,
        payload_type: None,
        payload: None,
        log_id_new: None,
    }
}

fn build_ack_frame(original: &pbbp2::Frame, biz_rt: &str) -> pbbp2::Frame {
    let mut headers = original.headers.clone();
    headers.push(pbbp2::Header {
        key: "biz_rt".into(),
        value: biz_rt.into(),
    });

    let ack_payload = serde_json::to_vec(&json!({
        "code": 200,
        "headers": null,
        "data": null
    }))
    .unwrap_or_default();

    pbbp2::Frame {
        seq_id: original.seq_id,
        log_id: original.log_id,
        service: original.service,
        method: original.method,
        headers,
        payload_encoding: original.payload_encoding.clone(),
        payload_type: original.payload_type.clone(),
        payload: Some(ack_payload),
        log_id_new: original.log_id_new.clone(),
    }
}

fn build_ack_frame_with_response(
    original: &pbbp2::Frame,
    biz_rt: &str,
    response: &serde_json::Value,
) -> pbbp2::Frame {
    let mut headers = original.headers.clone();
    headers.push(pbbp2::Header {
        key: "biz_rt".into(),
        value: biz_rt.into(),
    });

    // Encode response data as base64 (matching Go SDK's []byte JSON encoding)
    let response_json = serde_json::to_vec(response).unwrap_or_default();
    let response_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &response_json);
    let ack_payload = serde_json::to_vec(&json!({
        "code": 200,
        "headers": null,
        "data": response_b64
    }))
    .unwrap_or_default();

    pbbp2::Frame {
        seq_id: original.seq_id,
        log_id: original.log_id,
        service: original.service,
        method: original.method,
        headers,
        payload_encoding: original.payload_encoding.clone(),
        payload_type: original.payload_type.clone(),
        payload: Some(ack_payload),
        log_id_new: original.log_id_new.clone(),
    }
}

fn encode_frame(frame: &pbbp2::Frame) -> Vec<u8> {
    let mut buf = Vec::with_capacity(frame.encoded_len());
    frame.encode(&mut buf).expect("failed to encode frame");
    buf
}

fn frame_headers(frame: &pbbp2::Frame) -> HashMap<String, String> {
    frame
        .headers
        .iter()
        .map(|h| (h.key.clone(), h.value.clone()))
        .collect()
}

fn extract_query_param(url_str: &str, key: &str) -> Option<String> {
    let url = url::Url::parse(url_str).ok()?;
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.to_string())
}
