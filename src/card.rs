use serde::Serialize;
use serde_json::{Value, json};

// ────────────────────────────── Card top-level ──────────────────────────────

#[derive(Serialize, Clone)]
pub struct LarkMessage {
    pub msg_type: &'static str,
    pub card: LarkCard,
}

#[derive(Serialize, Clone)]
pub struct LarkCard {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<CardConfig>,
    pub header: LarkHeader,
    pub elements: Vec<Value>,
}

#[derive(Serialize, Clone)]
pub struct CardConfig {
    /// Whether the updated card content is visible to everyone who received this card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_multi: Option<bool>,
    /// Card width mode: "default", "compact", or "fill".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_mode: Option<String>,
}

impl LarkCard {
    /// Build a new card with the given header template color and title.
    pub fn new(template: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            config: None,
            header: LarkHeader::new(template, title),
            elements: Vec::new(),
        }
    }

    /// Mark this card as a shared card — updates are visible to all recipients.
    pub fn shared(mut self) -> Self {
        self.config = Some(CardConfig {
            update_multi: Some(true),
            ..self.config.unwrap_or(CardConfig { update_multi: None, width_mode: None })
        });
        self
    }

    /// Append an element that implements `Into<Value>`.
    pub fn push(mut self, element: impl Into<Value>) -> Self {
        self.elements.push(element.into());
        self
    }

    /// Append multiple elements.
    pub fn extend(mut self, elements: impl IntoIterator<Item = impl Into<Value>>) -> Self {
        self.elements.extend(elements.into_iter().map(Into::into));
        self
    }

    /// Wrap this card into a `LarkMessage` (msg_type = "interactive").
    pub fn into_message(self) -> LarkMessage {
        LarkMessage {
            msg_type: "interactive",
            card: self,
        }
    }
}

impl From<LarkCard> for LarkMessage {
    fn from(card: LarkCard) -> Self {
        card.into_message()
    }
}

#[derive(Serialize, Clone)]
pub struct LarkHeader {
    pub template: String,
    pub title: LarkTitle,
}

impl LarkHeader {
    pub fn new(template: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            title: LarkTitle { content: content.into(), tag: "plain_text" },
        }
    }
}

#[derive(Serialize, Clone)]
pub struct LarkTitle {
    pub content: String,
    pub tag: &'static str,
}

// ─────────────────────────── Typed card elements ───────────────────────────

/// Markdown text block (`tag: "div"` with `lark_md` text).
#[derive(Serialize, Clone)]
pub struct MdBlock {
    tag: &'static str,
    text: MdText,
}

#[derive(Serialize, Clone)]
struct MdText {
    tag: &'static str,
    content: String,
}

impl MdBlock {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            tag: "div",
            text: MdText { tag: "lark_md", content: content.into() },
        }
    }
}

impl From<MdBlock> for Value {
    fn from(b: MdBlock) -> Self {
        serde_json::to_value(b).unwrap()
    }
}

/// Horizontal rule.
#[derive(Serialize, Clone, Copy)]
pub struct Hr {
    tag: &'static str,
}

impl Hr {
    pub const fn new() -> Self {
        Self { tag: "hr" }
    }
}

impl From<Hr> for Value {
    fn from(h: Hr) -> Self {
        serde_json::to_value(h).unwrap()
    }
}

/// Image element.
#[derive(Serialize, Clone)]
pub struct ImageElement {
    tag: &'static str,
    img_key: String,
    alt: PlainText,
    mode: &'static str,
}

#[derive(Serialize, Clone)]
struct PlainText {
    tag: &'static str,
    content: String,
}

impl ImageElement {
    pub fn new(img_key: impl Into<String>) -> Self {
        Self {
            tag: "img",
            img_key: img_key.into(),
            alt: PlainText { tag: "plain_text", content: "Image".into() },
            mode: "fit_horizontal",
        }
    }

    pub fn alt(mut self, alt: impl Into<String>) -> Self {
        self.alt.content = alt.into();
        self
    }
}

impl From<ImageElement> for Value {
    fn from(img: ImageElement) -> Self {
        serde_json::to_value(img).unwrap()
    }
}

/// Footer note element.
#[derive(Serialize, Clone)]
pub struct NoteElement {
    tag: &'static str,
    elements: Vec<Value>,
}

impl NoteElement {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            tag: "note",
            elements: vec![json!({"tag": "plain_text", "content": text.into()})],
        }
    }
}

impl From<NoteElement> for Value {
    fn from(n: NoteElement) -> Self {
        serde_json::to_value(n).unwrap()
    }
}

/// Action bar containing buttons.
#[derive(Serialize, Clone)]
pub struct ActionGroup {
    tag: &'static str,
    actions: Vec<Value>,
}

impl ActionGroup {
    pub fn new() -> Self {
        Self { tag: "action", actions: Vec::new() }
    }

    pub fn button(mut self, text: &str, btn_type: &str, value: Value) -> Self {
        self.actions.push(action_btn(text, btn_type, value));
        self
    }

    pub fn button_confirm(
        mut self,
        text: &str,
        btn_type: &str,
        value: Value,
        title: &str,
        body: &str,
    ) -> Self {
        self.actions.push(action_btn_confirm(text, btn_type, value, title, body));
        self
    }
}

impl From<ActionGroup> for Value {
    fn from(a: ActionGroup) -> Self {
        serde_json::to_value(a).unwrap()
    }
}

/// Column set (grid layout).
#[derive(Serialize, Clone)]
pub struct ColumnSet {
    tag: &'static str,
    flex_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    horizontal_spacing: Option<String>,
    columns: Vec<Value>,
}

impl ColumnSet {
    pub fn bisect() -> Self {
        Self {
            tag: "column_set",
            flex_mode: "bisect".into(),
            horizontal_spacing: None,
            columns: Vec::new(),
        }
    }

    pub fn spacing(mut self, spacing: impl Into<String>) -> Self {
        self.horizontal_spacing = Some(spacing.into());
        self
    }

    /// Add a key-value column.
    pub fn kv(mut self, label: &str, value: &str) -> Self {
        self.columns.push(col_kv(label, value));
        self
    }

    /// Add a raw column value.
    pub fn col(mut self, col: impl Into<Value>) -> Self {
        self.columns.push(col.into());
        self
    }
}

impl From<ColumnSet> for Value {
    fn from(cs: ColumnSet) -> Self {
        serde_json::to_value(cs).unwrap()
    }
}

/// A single column with multiple `lark_md` rows.
#[derive(Clone)]
pub struct Column {
    rows: Vec<String>,
}

impl Column {
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    pub fn row(mut self, md: impl Into<String>) -> Self {
        self.rows.push(md.into());
        self
    }
}

impl From<Column> for Value {
    fn from(c: Column) -> Self {
        let elements: Vec<Value> = c.rows.into_iter().map(|md| {
            json!({"tag": "div", "text": {"tag": "lark_md", "content": md}})
        }).collect();
        json!({
            "tag": "column",
            "width": "weighted",
            "weight": 1,
            "elements": elements
        })
    }
}

// ───────────────────── Legacy free-function helpers ─────────────────────
// Kept for backward compatibility; prefer the typed structs above.

/// Markdown text block (legacy helper — prefer `MdBlock::new`).
pub fn md_block(content: &str) -> Value {
    MdBlock::new(content).into()
}

/// Interactive button with a JSON value payload.
pub fn action_btn(text: &str, btn_type: &str, value: Value) -> Value {
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
    value: Value,
    title: &str,
    body: &str,
) -> Value {
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
pub fn col_kv(label: &str, value: &str) -> Value {
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
pub fn md_field(content: &str) -> Value {
    json!({
        "is_short": true,
        "text": {
            "tag": "lark_md",
            "content": content
        }
    })
}

/// Fields block containing multiple short fields.
pub fn fields_block(fields: Vec<Value>) -> Value {
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
