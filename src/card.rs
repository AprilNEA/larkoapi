use serde::Serialize;
use serde_json::json;

// --- Card data models ---

#[derive(Serialize)]
pub struct LarkMessage {
    pub msg_type: &'static str,
    pub card: LarkCard,
}

#[derive(Serialize, Clone)]
pub struct LarkCard {
    pub header: LarkHeader,
    pub elements: Vec<serde_json::Value>,
}

#[derive(Serialize, Clone)]
pub struct LarkHeader {
    pub template: String,
    pub title: LarkTitle,
}

#[derive(Serialize, Clone)]
pub struct LarkTitle {
    pub content: String,
    pub tag: &'static str,
}

// --- Card element helpers ---

/// Markdown text block.
pub fn md_block(content: &str) -> serde_json::Value {
    json!({
        "tag": "div",
        "text": {
            "tag": "lark_md",
            "content": content
        }
    })
}

/// Interactive button with a JSON value payload.
pub fn action_btn(text: &str, btn_type: &str, value: serde_json::Value) -> serde_json::Value {
    json!({
        "tag": "button",
        "text": { "tag": "plain_text", "content": text },
        "type": btn_type,
        "value": value
    })
}

/// Interactive button with a confirmation dialog.
pub fn action_btn_confirm(
    text: &str,
    btn_type: &str,
    value: serde_json::Value,
    title: &str,
    body: &str,
) -> serde_json::Value {
    json!({
        "tag": "button",
        "text": { "tag": "plain_text", "content": text },
        "type": btn_type,
        "value": value,
        "confirm": {
            "title": { "tag": "plain_text", "content": title },
            "text": { "tag": "plain_text", "content": body }
        }
    })
}

/// Key-value column for use inside `column_set`.
pub fn col_kv(label: &str, value: &str) -> serde_json::Value {
    json!({
        "tag": "column",
        "width": "weighted",
        "weight": 1,
        "vertical_align": "center",
        "elements": [
            { "tag": "div", "text": { "tag": "lark_md", "content": format!("**{label}**\n{value}") } }
        ]
    })
}

/// Short field for use inside a `fields_block`.
pub fn md_field(content: &str) -> serde_json::Value {
    json!({
        "is_short": true,
        "text": {
            "tag": "lark_md",
            "content": content
        }
    })
}

/// Fields block containing multiple short fields.
pub fn fields_block(fields: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "tag": "div",
        "fields": fields
    })
}

/// Format minutes into a human-readable duration string (e.g. "1h30min").
pub fn format_duration(minutes: i64) -> String {
    if minutes < 60 {
        format!("{minutes}min")
    } else {
        let h = minutes / 60;
        let m = minutes % 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h{m}min")
        }
    }
}
