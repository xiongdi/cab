//! Canonical intermediate representation for protocol conversion.

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrBlock {
    Text {
        text: String,
    },
    Thinking {
        text: String,
        signature: Option<String>,
    },
    Image {
        media_type: String,
        source: IrImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Vec<IrBlock>,
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrImageSource {
    Base64(String),
    Url(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrMessage {
    pub role: String,
    pub blocks: Vec<IrBlock>,
}

/// Parse an image `data:` URL or http(s) URL into an `IrBlock::Image`. Returns
/// `None` for non-image URLs (e.g. plain text, `data:text/...`).
fn image_block_from_url(url: &str) -> Option<IrBlock> {
    if let Some(rest) = url.strip_prefix("data:image/")
        && let Some((subtype, b64)) = rest.split_once(';')
        && let Some(data) = b64.strip_prefix("base64,")
    {
        return Some(IrBlock::Image {
            media_type: format!("image/{}", subtype.trim()),
            source: IrImageSource::Base64(data.to_string()),
        });
    } else if url.starts_with("http://") || url.starts_with("https://") {
        return Some(IrBlock::Image {
            media_type: "image/jpeg".to_string(),
            source: IrImageSource::Url(url.to_string()),
        });
    }
    None
}

/// Build tool-result content blocks from a `function_call_output.output` value.
///
/// Codex's `view_image` returns the image as an array of
/// `{"type":"input_image","image_url":"data:image/png;base64,..."}` parts (and
/// may mix in `output_text`/`input_text` parts); OpenAI/Anthropic send a plain
/// string. We keep text parts and extract any embedded images so the image can
/// be forwarded to a vision-capable upstream model as a real image block.
fn tool_result_blocks_from_output(output: &Value) -> Vec<IrBlock> {
    match output {
        Value::String(s) if !s.is_empty() => vec![IrBlock::Text { text: s.clone() }],
        Value::Array(items) => {
            let mut blocks = Vec::new();
            for item in items {
                if let Some(kind) = item.get("type").and_then(|t| t.as_str()) {
                    if kind == "input_image" || kind == "output_image" {
                        if let Some(url) = item.get("image_url").and_then(|u| u.as_str())
                            && let Some(img) = image_block_from_url(url)
                        {
                            blocks.push(img);
                            continue;
                        }
                    } else if (kind == "output_text" || kind == "input_text")
                        && let Some(t) = item.get("text").and_then(|t| t.as_str())
                        && !t.is_empty()
                    {
                        blocks.push(IrBlock::Text {
                            text: t.to_string(),
                        });
                        continue;
                    }
                }
                // Fallback: serialize anything else as text.
                let text = match item {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if !text.is_empty() && text != "null" {
                    blocks.push(IrBlock::Text { text });
                }
            }
            if blocks.is_empty() {
                blocks.push(IrBlock::Text {
                    text: String::new(),
                });
            }
            blocks
        }
        other => {
            let text = other.to_string();
            if text.is_empty() || text == "null" {
                vec![IrBlock::Text {
                    text: String::new(),
                }]
            } else {
                vec![IrBlock::Text { text }]
            }
        }
    }
}

/// Render tool-result content blocks into an Anthropic `tool_result.content`
/// array. Only `Text` and `Image` blocks are valid here.
fn tool_result_content_to_anthropic(blocks: &[IrBlock]) -> Value {
    if blocks.is_empty() {
        return Value::String(String::new());
    }
    Value::Array(
        blocks
            .iter()
            .map(|b| match b {
                IrBlock::Text { text } => serde_json::json!({"type": "text", "text": text}),
                IrBlock::Image { media_type, source } => match source {
                    IrImageSource::Base64(data) => serde_json::json!({
                        "type": "image",
                        "source": {"type": "base64", "media_type": media_type, "data": data}
                    }),
                    IrImageSource::Url(url) => serde_json::json!({
                        "type": "image",
                        "source": {"type": "url", "url": url}
                    }),
                },
                _ => serde_json::json!({"type": "text", "text": ""}),
            })
            .collect(),
    )
}

/// Render tool-result content blocks into an OpenAI Chat `tool` message content
/// array (text + `image_url` parts). This is the path used for OpenAI-compatible
/// upstreams (e.g. deepseek), where the model must receive a proper image part.
fn tool_result_content_to_openai_chat(blocks: &[IrBlock]) -> Value {
    if blocks.is_empty() {
        return Value::String(String::new());
    }
    Value::Array(
        blocks
            .iter()
            .map(|b| match b {
                IrBlock::Text { text } => serde_json::json!({"type": "text", "text": text}),
                IrBlock::Image { source, .. } => {
                    let url = match source {
                        IrImageSource::Url(u) => u.clone(),
                        IrImageSource::Base64(d) => format!("data:image/jpeg;base64,{d}"),
                    };
                    serde_json::json!({"type": "image_url", "image_url": {"url": url}})
                }
                _ => serde_json::json!({"type": "text", "text": ""}),
            })
            .collect(),
    )
}

/// Render tool-result content blocks into an OpenAI Responses
/// `function_call_output.output` value.
///
/// In standard OpenAI Responses API, `output` is a plain string.
/// Codex's multimodal `view_image` extension uses an array of
/// `input_image` (and optionally `input_text`) parts.
fn tool_result_content_to_responses(blocks: &[IrBlock]) -> Value {
    if blocks.is_empty() {
        return Value::String(String::new());
    }
    let has_images = blocks.iter().any(|b| matches!(b, IrBlock::Image { .. }));
    if !has_images {
        let mut text = String::new();
        for b in blocks {
            if let IrBlock::Text { text: t } = b {
                text.push_str(t);
            }
        }
        return Value::String(text);
    }
    Value::Array(
        blocks
            .iter()
            .map(|b| match b {
                IrBlock::Text { text } => serde_json::json!({"type": "input_text", "text": text}),
                IrBlock::Image { source, .. } => {
                    let url = match source {
                        IrImageSource::Url(u) => u.clone(),
                        IrImageSource::Base64(d) => format!("data:image/jpeg;base64,{d}"),
                    };
                    serde_json::json!({"type": "input_image", "image_url": url})
                }
                _ => serde_json::json!({"type": "input_text", "text": ""}),
            })
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IrToolChoice {
    #[default]
    Auto,
    None,
    Required,
    Any,
    Tool {
        name: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct IrRequest {
    pub model: Option<String>,
    pub max_tokens: Option<u64>,
    pub stream: bool,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub stop_sequences: Vec<String>,
    pub system: Vec<IrBlock>,
    pub messages: Vec<IrMessage>,
    pub tools: Vec<IrTool>,
    pub tool_choice: IrToolChoice,
    pub extensions: Map<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct IrUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

#[derive(Debug, Clone, Default)]
pub struct IrResponse {
    pub id: String,
    pub model: String,
    pub blocks: Vec<IrBlock>,
    pub stop_reason: String,
    pub usage: IrUsage,
}

fn text_from_value(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                block
                    .as_str()
                    .map(String::from)
                    .or_else(|| block.get("text").and_then(|t| t.as_str()).map(String::from))
                    .or_else(|| {
                        block
                            .get("content")
                            .and_then(|c| c.as_str())
                            .map(String::from)
                    })
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn content_to_string(content: &Value) -> String {
    let text = text_from_value(content);
    if !text.is_empty() {
        return text;
    }
    match content {
        Value::Null => String::new(),
        Value::Object(_) | Value::Array(_) => content.to_string(),
        other => other.to_string(),
    }
}

fn anthropic_blocks_from_content(content: &Value) -> Vec<IrBlock> {
    match content {
        Value::String(text) if !text.is_empty() => vec![IrBlock::Text { text: text.clone() }],
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(anthropic_block_from_value)
            .collect(),
        _ => Vec::new(),
    }
}

fn anthropic_block_from_value(block: &Value) -> Option<IrBlock> {
    match block.get("type").and_then(|t| t.as_str()) {
        Some("text") | None => block
            .get("text")
            .and_then(|t| t.as_str())
            .or_else(|| block.get("content").and_then(|c| c.as_str()))
            .map(|text| IrBlock::Text {
                text: text.to_string(),
            }),
        Some("thinking") => Some(IrBlock::Thinking {
            text: block
                .get("thinking")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .to_string(),
            signature: block
                .get("signature")
                .and_then(|s| s.as_str())
                .map(str::to_string),
        }),
        Some("image") => {
            let media_type = if let Some(m) = block
                .get("source")
                .and_then(|s| s.get("media_type"))
                .and_then(|m| m.as_str())
            {
                m.to_string()
            } else if let Some(m) = block.get("media_type").and_then(|m| m.as_str()) {
                m.to_string()
            } else {
                "image/jpeg".to_string()
            };
            let data = block
                .get("source")
                .and_then(|s| s.get("data"))
                .and_then(|d| d.as_str());
            let url = block
                .get("source")
                .and_then(|s| s.get("url"))
                .and_then(|u| u.as_str());
            let source = match (data, url) {
                (Some(data), _) => IrImageSource::Base64(data.to_string()),
                (_, Some(url)) => IrImageSource::Url(url.to_string()),
                _ => return None,
            };
            Some(IrBlock::Image { media_type, source })
        }
        Some("tool_use") => Some(IrBlock::ToolUse {
            id: block
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            name: block
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            input: block
                .get("input")
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new())),
        }),
        Some("tool_result") => Some(IrBlock::ToolResult {
            tool_use_id: block
                .get("tool_use_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            content: {
                let text = text_from_value(block.get("content").unwrap_or(&Value::Null));
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![IrBlock::Text { text }]
                }
            },
            is_error: block
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }),
        _ => None,
    }
}

fn openai_content_blocks(content: Option<&Value>) -> Vec<IrBlock> {
    match content {
        Some(Value::String(text)) if !text.is_empty() => vec![IrBlock::Text { text: text.clone() }],
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| match part.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    part.get("text")
                        .and_then(|t| t.as_str())
                        .map(|text| IrBlock::Text {
                            text: text.to_string(),
                        })
                }
                Some("image_url") => {
                    let url = part
                        .get("image_url")
                        .and_then(|i| i.get("url"))
                        .and_then(|u| u.as_str())?;
                    Some(IrBlock::Image {
                        media_type: "image/jpeg".to_string(),
                        source: IrImageSource::Url(url.to_string()),
                    })
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn openai_message_to_ir(msg: &Value) -> Vec<IrMessage> {
    let role = msg
        .get("role")
        .and_then(|r| r.as_str())
        .unwrap_or("user")
        .to_string();

    if role == "tool" {
        let content = text_from_value(msg.get("content").unwrap_or(&Value::Null));
        if content.is_empty() {
            return Vec::new();
        }
        return vec![IrMessage {
            role: "user".into(),
            blocks: vec![IrBlock::ToolResult {
                tool_use_id: msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                content: vec![IrBlock::Text { text: content }],
                is_error: false,
            }],
        }];
    }

    if role == "assistant" {
        let mut blocks = openai_content_blocks(msg.get("content"));
        if let Some(reasoning) = msg.get("reasoning_content").and_then(|v| v.as_str())
            && !reasoning.is_empty()
        {
            blocks.insert(
                0,
                IrBlock::Thinking {
                    text: reasoning.to_string(),
                    signature: None,
                },
            );
        }
        if let Some(Value::Array(tool_calls)) = msg.get("tool_calls") {
            for call in tool_calls {
                let args = call
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}");
                let input =
                    serde_json::from_str(args).unwrap_or_else(|_| Value::Object(Map::new()));
                blocks.push(IrBlock::ToolUse {
                    id: call
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    input,
                });
            }
        }
        if blocks.is_empty() {
            return Vec::new();
        }
        return vec![IrMessage {
            role: "assistant".into(),
            blocks,
        }];
    }

    let blocks = openai_content_blocks(msg.get("content"));
    if blocks.is_empty() {
        return Vec::new();
    }
    vec![IrMessage { role, blocks }]
}

fn responses_item_to_ir_messages(item: &Value) -> Vec<IrMessage> {
    match item.get("type").and_then(|t| t.as_str()) {
        Some("function_call") => {
            let args = item
                .get("arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let input = serde_json::from_str(args).unwrap_or_else(|_| Value::Object(Map::new()));
            vec![IrMessage {
                role: "assistant".into(),
                blocks: vec![IrBlock::ToolUse {
                    id: item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    input,
                }],
            }]
        }
        Some("function_call_output") => {
            let output = item.get("output").cloned().unwrap_or(Value::Null);
            vec![IrMessage {
                role: "user".into(),
                blocks: vec![IrBlock::ToolResult {
                    tool_use_id: item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    content: tool_result_blocks_from_output(&output),
                    is_error: false,
                }],
            }]
        }
        _ => {
            let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let role = match role {
                "developer" | "system" => "system",
                other => other,
            }
            .to_string();
            let content = item
                .get("content")
                .map(content_to_string)
                .unwrap_or_default();
            if content.trim().is_empty() {
                return Vec::new();
            }
            vec![IrMessage {
                role,
                blocks: vec![IrBlock::Text { text: content }],
            }]
        }
    }
}

fn tool_choice_from_openai(value: &Value) -> IrToolChoice {
    match value {
        Value::String(s) => match s.as_str() {
            "auto" => IrToolChoice::Auto,
            "none" => IrToolChoice::None,
            "required" => IrToolChoice::Required,
            _ => IrToolChoice::Auto,
        },
        Value::Object(obj) => match obj.get("type").and_then(|t| t.as_str()) {
            Some("function") => IrToolChoice::Tool {
                name: obj
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .or_else(|| obj.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            Some("any") => IrToolChoice::Any,
            Some("auto") => IrToolChoice::Auto,
            Some("tool") => IrToolChoice::Tool {
                name: obj
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            _ => IrToolChoice::Auto,
        },
        _ => IrToolChoice::Auto,
    }
}

fn tool_choice_from_anthropic(value: &Value) -> IrToolChoice {
    match value {
        Value::Object(obj) => match obj.get("type").and_then(|t| t.as_str()) {
            Some("any") => IrToolChoice::Any,
            Some("tool") => IrToolChoice::Tool {
                name: obj
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            _ => IrToolChoice::Auto,
        },
        _ => IrToolChoice::Auto,
    }
}

fn tools_from_openai(value: &Value) -> Vec<IrTool> {
    let Value::Array(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|tool| {
            let function = tool.get("function").unwrap_or(tool);
            let name = function
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default();
            // Tools without a name (e.g. Responses-style `external_web_access`)
            // cannot be expressed as an OpenAI `function`; drop them so a later
            // protocol conversion never emits `function.name: ""`.
            if name.is_empty() {
                return None;
            }
            Some(IrTool {
                name: name.to_string(),
                description: function
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(str::to_string),
                input_schema: function
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
                strict: tool.get("strict").and_then(|v| v.as_bool()),
            })
        })
        .collect()
}

fn tools_from_anthropic(value: &Value) -> Vec<IrTool> {
    let Value::Array(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|tool| {
            let name = tool
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default();
            // Skip nameless tools so conversions never emit an empty `name`.
            if name.is_empty() {
                return None;
            }
            Some(IrTool {
                name: name.to_string(),
                description: tool
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(str::to_string),
                input_schema: tool
                    .get("input_schema")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
                strict: None,
            })
        })
        .collect()
}

fn tools_from_responses(value: &Value) -> Vec<IrTool> {
    let Value::Array(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|tool| {
            let name = tool
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default();
            // Responses supports special tool types (e.g. `external_web_access`)
            // that carry no `name`; they cannot round-trip through the IR to
            // OpenAI `function` tools, so drop them instead of emitting "".
            if name.is_empty() {
                return None;
            }
            Some(IrTool {
                name: name.to_string(),
                description: tool
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(str::to_string),
                input_schema: tool
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
                strict: tool.get("strict").and_then(|v| v.as_bool()),
            })
        })
        .collect()
}

const KNOWN_REQUEST_KEYS: &[&str] = &[
    "model",
    "max_tokens",
    "max_output_tokens",
    "stream",
    "temperature",
    "top_p",
    "stop",
    "stop_sequences",
    "messages",
    "system",
    "tools",
    "tool_choice",
    "instructions",
    "input",
    "thinking",
    "metadata",
];

fn collect_extensions(body: &Value) -> Map<String, Value> {
    let mut extensions = Map::new();
    if let Some(obj) = body.as_object() {
        for (key, value) in obj {
            if !KNOWN_REQUEST_KEYS.contains(&key.as_str()) {
                extensions.insert(key.clone(), value.clone());
            }
        }
    }
    extensions
}

pub fn decode_anthropic_request(body: &Value) -> IrRequest {
    let mut ir = IrRequest {
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        max_tokens: body.get("max_tokens").and_then(|v| v.as_u64()),
        stream: body
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        temperature: body.get("temperature").and_then(|v| v.as_f64()),
        top_p: body.get("top_p").and_then(|v| v.as_f64()),
        stop_sequences: body
            .get("stop_sequences")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        system: body
            .get("system")
            .map(anthropic_blocks_from_content)
            .unwrap_or_default(),
        messages: body
            .get("messages")
            .and_then(|v| v.as_array())
            .map(|msgs| {
                msgs.iter()
                    .map(|msg| IrMessage {
                        role: msg
                            .get("role")
                            .and_then(|r| r.as_str())
                            .unwrap_or("user")
                            .to_string(),
                        blocks: msg
                            .get("content")
                            .map(anthropic_blocks_from_content)
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        tools: body
            .get("tools")
            .map(tools_from_anthropic)
            .unwrap_or_default(),
        tool_choice: body
            .get("tool_choice")
            .map(tool_choice_from_anthropic)
            .unwrap_or_default(),
        extensions: collect_extensions(body),
    };
    if body.get("thinking").is_some()
        && let Some(thinking) = body.get("thinking")
    {
        ir.extensions.insert("thinking".into(), thinking.clone());
    }
    ir
}

pub fn decode_openai_chat_request(body: &Value) -> IrRequest {
    let mut system = Vec::new();
    let mut messages = Vec::new();
    if let Some(Value::Array(msgs)) = body.get("messages") {
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            if role == "system" || role == "developer" {
                system.extend(openai_content_blocks(msg.get("content")));
            } else {
                messages.extend(openai_message_to_ir(msg));
            }
        }
    }
    IrRequest {
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        max_tokens: body.get("max_tokens").and_then(|v| v.as_u64()),
        stream: body
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        temperature: body.get("temperature").and_then(|v| v.as_f64()),
        top_p: body.get("top_p").and_then(|v| v.as_f64()),
        stop_sequences: body
            .get("stop")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        system,
        messages,
        tools: body.get("tools").map(tools_from_openai).unwrap_or_default(),
        tool_choice: body
            .get("tool_choice")
            .map(tool_choice_from_openai)
            .unwrap_or_default(),
        extensions: collect_extensions(body),
    }
}

pub fn decode_responses_request(body: &Value) -> IrRequest {
    let mut system = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        system.push(IrBlock::Text {
            text: text_from_value(instructions),
        });
    }
    let mut messages = Vec::new();
    match body.get("input") {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            messages.push(IrMessage {
                role: "user".into(),
                blocks: vec![IrBlock::Text { text: text.clone() }],
            });
        }
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(text) = item.as_str() {
                    if !text.trim().is_empty() {
                        messages.push(IrMessage {
                            role: "user".into(),
                            blocks: vec![IrBlock::Text {
                                text: text.to_string(),
                            }],
                        });
                    }
                } else {
                    messages.extend(responses_item_to_ir_messages(item));
                }
            }
        }
        _ => {}
    }
    IrRequest {
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        max_tokens: body
            .get("max_output_tokens")
            .or_else(|| body.get("max_tokens"))
            .and_then(|v| v.as_u64()),
        stream: body
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        temperature: body.get("temperature").and_then(|v| v.as_f64()),
        top_p: body.get("top_p").and_then(|v| v.as_f64()),
        stop_sequences: Vec::new(),
        system,
        messages,
        tools: body
            .get("tools")
            .map(tools_from_responses)
            .unwrap_or_default(),
        tool_choice: body
            .get("tool_choice")
            .map(tool_choice_from_openai)
            .unwrap_or_default(),
        extensions: collect_extensions(body),
    }
}

fn ir_block_to_anthropic_value(block: &IrBlock) -> Value {
    match block {
        IrBlock::Text { text } => serde_json::json!({"type": "text", "text": text}),
        IrBlock::Thinking { text, signature } => {
            let mut obj = serde_json::json!({"type": "thinking", "thinking": text});
            // Claude Code drops thinking blocks that lack a signature when
            // replaying the conversation. Providers that emit reasoning without
            // Anthropic signatures (OpenAI-compat / Console Go) need a synthetic
            // one so the agent round-trips CoT into later turns.
            let sig = signature
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("cab_{}", uuid::Uuid::new_v4().simple()));
            obj["signature"] = Value::String(sig);
            obj
        }
        IrBlock::Image { media_type, source } => match source {
            IrImageSource::Base64(data) => serde_json::json!({
                "type": "image",
                "source": {"type": "base64", "media_type": media_type, "data": data}
            }),
            IrImageSource::Url(url) => serde_json::json!({
                "type": "image",
                "source": {"type": "url", "url": url}
            }),
        },
        IrBlock::ToolUse { id, name, input } => serde_json::json!({
            "type": "tool_use", "id": id, "name": name, "input": input
        }),
        IrBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => serde_json::json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": tool_result_content_to_anthropic(content),
            "is_error": is_error
        }),
    }
}

fn ir_messages_to_anthropic(ir: &IrRequest) -> Vec<IrMessage> {
    let mut out = ir.messages.clone();
    for msg in &mut out {
        if msg.role == "tool" {
            msg.role = "user".into();
        }
    }
    out
}

pub fn encode_anthropic_request(ir: &IrRequest) -> Value {
    let mut obj = Map::new();
    if let Some(model) = &ir.model {
        obj.insert("model".into(), Value::String(model.clone()));
    }
    obj.insert(
        "max_tokens".into(),
        Value::Number((ir.max_tokens.unwrap_or(4096)).into()),
    );
    obj.insert("stream".into(), Value::Bool(ir.stream));
    if let Some(temp) = ir.temperature {
        obj.insert("temperature".into(), serde_json::json!(temp));
    }
    if let Some(top_p) = ir.top_p {
        obj.insert("top_p".into(), serde_json::json!(top_p));
    }
    if !ir.stop_sequences.is_empty() {
        obj.insert(
            "stop_sequences".into(),
            Value::Array(
                ir.stop_sequences
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    if !ir.system.is_empty() {
        let text_only: Vec<&str> = ir
            .system
            .iter()
            .filter_map(|b| match b {
                IrBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        if text_only.len() == ir.system.len() && !text_only.is_empty() {
            obj.insert("system".into(), Value::String(text_only.join("\n\n")));
        } else {
            let blocks: Vec<Value> = ir.system.iter().map(ir_block_to_anthropic_value).collect();
            obj.insert("system".into(), Value::Array(blocks));
        }
    }
    let messages: Vec<Value> = ir_messages_to_anthropic(ir)
        .into_iter()
        .filter_map(|msg| {
            let blocks: Vec<Value> = msg.blocks.iter().map(ir_block_to_anthropic_value).collect();
            if blocks.is_empty() {
                return None;
            }
            let content = if blocks.len() == 1 && msg.blocks.len() == 1 {
                match &msg.blocks[0] {
                    IrBlock::Text { text } => Value::String(text.clone()),
                    _ => Value::Array(blocks),
                }
            } else {
                Value::Array(blocks)
            };
            Some(serde_json::json!({"role": msg.role, "content": content}))
        })
        .collect();
    obj.insert("messages".into(), Value::Array(messages));
    if !ir.tools.is_empty() {
        obj.insert(
            "tools".into(),
            Value::Array(
                ir.tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "description": t.description,
                            "input_schema": t.input_schema,
                        })
                    })
                    .collect(),
            ),
        );
    }
    obj.insert(
        "tool_choice".into(),
        encode_anthropic_tool_choice(&ir.tool_choice),
    );
    for (k, v) in &ir.extensions {
        obj.insert(k.clone(), v.clone());
    }
    Value::Object(obj)
}

fn encode_anthropic_tool_choice(choice: &IrToolChoice) -> Value {
    match choice {
        IrToolChoice::Auto => serde_json::json!({"type": "auto"}),
        IrToolChoice::None => serde_json::json!({"type": "auto"}),
        IrToolChoice::Required | IrToolChoice::Any => serde_json::json!({"type": "any"}),
        IrToolChoice::Tool { name } => serde_json::json!({"type": "tool", "name": name}),
    }
}

fn ir_message_to_openai_messages(msg: &IrMessage) -> Vec<Value> {
    let mut out = Vec::new();
    let mut text_parts = Vec::new();
    let mut reasoning_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_results = Vec::new();

    for block in &msg.blocks {
        match block {
            IrBlock::Text { text } => text_parts.push(text.clone()),
            IrBlock::Thinking { text, .. } => reasoning_parts.push(text.clone()),
            IrBlock::Image { source, .. } => {
                let url = match source {
                    IrImageSource::Url(u) => u.clone(),
                    IrImageSource::Base64(d) => format!("data:image/jpeg;base64,{d}"),
                };
                text_parts.push(format!("[image:{url}]"));
            }
            IrBlock::ToolUse { id, name, input } => tool_calls.push(serde_json::json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".into()),
                }
            })),
            IrBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => tool_results.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tool_use_id,
                "content": tool_result_content_to_openai_chat(content),
            })),
        }
    }

    if msg.role == "assistant" && !tool_calls.is_empty() {
        let mut assistant = serde_json::json!({
            "role": "assistant",
            "tool_calls": tool_calls,
        });
        if !text_parts.is_empty() {
            assistant["content"] = Value::String(text_parts.join(""));
        }
        // DeepSeek-style thinking models require `reasoning_content` on every
        // assistant tool-call turn when replaying history. Claude Code (and
        // similar agents) often strip unsigned thinking blocks, so we always
        // emit the field — real text when available, otherwise empty string.
        assistant["reasoning_content"] = Value::String(reasoning_parts.join(""));
        out.push(assistant);
    } else if msg.role == "assistant" {
        let mut assistant = serde_json::json!({
            "role": "assistant",
            "content": text_parts.join(""),
        });
        if !reasoning_parts.is_empty() {
            assistant["reasoning_content"] = Value::String(reasoning_parts.join(""));
        }
        out.push(assistant);
    } else if !tool_results.is_empty() {
        out.extend(tool_results);
    } else if !text_parts.is_empty() || !msg.blocks.is_empty() {
        let cleaned = strip_claude_total_tokens(&text_parts.join(""));
        if !cleaned.is_empty() {
            out.push(serde_json::json!({"role": msg.role, "content": cleaned}));
        }
    }
    out
}

fn encode_openai_tool_choice(choice: &IrToolChoice) -> Value {
    match choice {
        IrToolChoice::Auto => Value::String("auto".into()),
        IrToolChoice::None => Value::String("none".into()),
        IrToolChoice::Required | IrToolChoice::Any => Value::String("required".into()),
        IrToolChoice::Tool { name } => {
            serde_json::json!({"type": "function", "function": {"name": name}})
        }
    }
}

/// Anthropic Messages-exclusive request body extensions that should not be
/// passed to OpenAI Responses or Chat Completions endpoints.
pub fn is_anthropic_exclusive_extension(key: &str) -> bool {
    matches!(
        key,
        "thinking"
            | "context_management"
            | "output_config"
            | "betas"
            | "anthropic_beta"
            | "anthropic_version"
            | "top_k"
            | "container"
    )
}

pub fn encode_openai_chat_request(ir: &IrRequest) -> Value {
    let mut obj = Map::new();
    if let Some(model) = &ir.model {
        obj.insert("model".into(), Value::String(model.clone()));
    }
    if let Some(max_tokens) = ir.max_tokens {
        obj.insert("max_tokens".into(), Value::Number(max_tokens.into()));
    }
    obj.insert("stream".into(), Value::Bool(ir.stream));
    if let Some(temp) = ir.temperature {
        obj.insert("temperature".into(), serde_json::json!(temp));
    }
    if let Some(top_p) = ir.top_p {
        obj.insert("top_p".into(), serde_json::json!(top_p));
    }
    if !ir.stop_sequences.is_empty() {
        obj.insert(
            "stop".into(),
            Value::Array(
                ir.stop_sequences
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    let mut messages: Vec<Value> = Vec::new();
    for block in &ir.system {
        if let IrBlock::Text { text } = block {
            messages.push(serde_json::json!({"role": "system", "content": text}));
        }
    }
    // OpenAI Chat Completions requires every `tool_calls` in one turn to be
    // bundled into a single assistant message, followed by all `tool` responses.
    // A Responses request expresses parallel calls as consecutive top-level
    // `function_call` items, which decode to one assistant message each — so
    // merge adjacent assistant-with-tool_calls messages here. Failing to do so
    // yields `assistant(tool_calls:[A]) assistant(tool_calls:[B]) tool(A) tool(B)`,
    // which upstream rejects with "insufficient tool messages following
    // tool_calls message".
    let mut pending_assistant: Option<Value> = None;
    let flush_pending = |messages: &mut Vec<Value>, pending: &mut Option<Value>| {
        if let Some(assistant) = pending.take() {
            messages.push(assistant);
        }
    };
    for msg in &ir.messages {
        let converted = ir_message_to_openai_messages(msg);
        for item in converted {
            let is_mergeable_tool_call_turn = item
                .get("role")
                .and_then(|r| r.as_str())
                .map(|r| r == "assistant")
                .unwrap_or(false)
                && item.get("content").is_none()
                && item.get("tool_calls").is_some();
            if is_mergeable_tool_call_turn {
                if pending_assistant.is_some() {
                    if let Some(pending) = pending_assistant.as_mut()
                        && let Some(Value::Array(calls)) = pending.get_mut("tool_calls")
                        && let Some(Value::Array(more)) = item.get("tool_calls").cloned()
                    {
                        calls.extend(more);
                    }
                    continue;
                }
                pending_assistant = Some(item);
            } else {
                flush_pending(&mut messages, &mut pending_assistant);
                messages.push(item);
            }
        }
    }
    flush_pending(&mut messages, &mut pending_assistant);
    if messages.is_empty() {
        messages.push(serde_json::json!({"role": "user", "content": " "}));
    }
    obj.insert("messages".into(), Value::Array(messages));
    if !ir.tools.is_empty() {
        obj.insert(
            "tools".into(),
            Value::Array(
                ir.tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.input_schema,
                            }
                        })
                    })
                    .collect(),
            ),
        );
    }
    obj.insert(
        "tool_choice".into(),
        encode_openai_tool_choice(&ir.tool_choice),
    );
    for (k, v) in &ir.extensions {
        if k == "output_config" {
            if let Some(format) = v.get("format").and_then(|f| f.as_object()) {
                let fmt_type = format.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if fmt_type == "json_schema" {
                    let mut schema_obj = serde_json::Map::new();
                    let name = format
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("response_schema");
                    schema_obj.insert("name".into(), Value::String(name.into()));
                    if let Some(schema) = format.get("schema") {
                        schema_obj.insert("schema".into(), schema.clone());
                    }
                    obj.insert(
                        "response_format".into(),
                        serde_json::json!({
                            "type": "json_schema",
                            "json_schema": schema_obj
                        }),
                    );
                } else if fmt_type == "json_object" {
                    obj.insert(
                        "response_format".into(),
                        serde_json::json!({"type": "json_object"}),
                    );
                }
            }
            continue;
        }
        // Anthropic `thinking` and other Anthropic-exclusive extensions are not Chat fields.
        if is_anthropic_exclusive_extension(k) {
            continue;
        }
        obj.insert(k.clone(), v.clone());
    }
    Value::Object(obj)
}

/// Strip Claude Code ephemeral `<total_tokens>... tokens left</total_tokens>` tags.
/// When Claude Code inserts these between turns, they act as ephemeral token-counter
/// probes. Passing them into upstream models as user or system turns breaks the
/// function_call -> function_call_output continuity, causing models to halt early
/// instead of completing multi-turn tool loops.
pub fn strip_claude_total_tokens(text: &str) -> String {
    if !text.contains("<total_tokens>") {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<total_tokens>") {
        result.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("</total_tokens>") {
            rest = &rest[start + end + "</total_tokens>".len()..];
        } else {
            rest = "";
            break;
        }
    }
    result.push_str(rest);
    result.trim().to_string()
}

fn ir_message_to_responses_items(msg: &IrMessage) -> Vec<Value> {
    let mut items = Vec::new();
    let mut text = String::new();
    for block in &msg.blocks {
        match block {
            IrBlock::Text { text: t } => text.push_str(t),
            IrBlock::Thinking { .. } => {}
            IrBlock::ToolUse { id, name, input } => {
                let cleaned_text = strip_claude_total_tokens(&text);
                if !cleaned_text.is_empty() {
                    items.push(serde_json::json!({"role": "assistant", "content": cleaned_text}));
                }
                text.clear();
                items.push(serde_json::json!({
                    "type": "function_call",
                    "call_id": id,
                    "name": name,
                    "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".into()),
                }));
            }
            IrBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                items.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": tool_use_id,
                    "output": tool_result_content_to_responses(content),
                }));
            }
            IrBlock::Image { .. } => {}
        }
    }
    let cleaned_text = strip_claude_total_tokens(&text);
    if !cleaned_text.is_empty() {
        items.push(serde_json::json!({"role": msg.role, "content": cleaned_text}));
    }
    items
}

fn encode_responses_tool_choice(choice: &IrToolChoice) -> Value {
    match choice {
        IrToolChoice::Auto => Value::String("auto".into()),
        IrToolChoice::None => Value::String("none".into()),
        IrToolChoice::Required | IrToolChoice::Any => Value::String("required".into()),
        IrToolChoice::Tool { name } => {
            serde_json::json!({"type": "function", "name": name})
        }
    }
}

pub fn encode_responses_request(ir: &IrRequest) -> Value {
    let mut obj = Map::new();
    if let Some(model) = &ir.model {
        obj.insert("model".into(), Value::String(model.clone()));
    }
    if let Some(max_tokens) = ir.max_tokens {
        obj.insert("max_output_tokens".into(), Value::Number(max_tokens.into()));
    }
    obj.insert("stream".into(), Value::Bool(ir.stream));
    if let Some(temp) = ir.temperature {
        obj.insert("temperature".into(), serde_json::json!(temp));
    }
    let instructions: String = ir
        .system
        .iter()
        .filter_map(|b| match b {
            IrBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if !instructions.is_empty() {
        obj.insert("instructions".into(), Value::String(instructions));
    }
    let mut input = Vec::new();
    for msg in &ir.messages {
        input.extend(ir_message_to_responses_items(msg));
    }
    if input.is_empty() {
        input.push(serde_json::json!({"role": "user", "content": " "}));
    }
    obj.insert("input".into(), Value::Array(input));
    if !ir.tools.is_empty() {
        obj.insert(
            "tools".into(),
            Value::Array(
                ir.tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "type": "function",
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        })
                    })
                    .collect(),
            ),
        );
    }
    obj.insert(
        "tool_choice".into(),
        encode_responses_tool_choice(&ir.tool_choice),
    );
    for (k, v) in &ir.extensions {
        // Convert Anthropic `thinking` to OpenAI Responses `reasoning.effort`.
        // The Responses API does not understand `thinking` — passing it through
        // verbatim causes upstream 400 errors on reseller gateways.
        if k == "thinking" {
            if let Some(effort) = reasoning_effort_from_thinking(v) {
                obj.insert("reasoning".into(), serde_json::json!({"effort": effort}));
            }
            continue;
        }
        if k == "output_config" {
            if let Some(effort) = v.get("effort").and_then(|e| e.as_str())
                && !obj.contains_key("reasoning")
            {
                obj.insert("reasoning".into(), serde_json::json!({"effort": effort}));
            }
            if let Some(format) = v.get("format").and_then(|f| f.as_object()) {
                let mut fmt = format.clone();
                if fmt.get("type").and_then(|t| t.as_str()) == Some("json_schema")
                    && fmt.get("name").is_none()
                {
                    fmt.insert("name".into(), Value::String("response_schema".into()));
                }
                obj.insert("text".into(), serde_json::json!({"format": fmt}));
            }
            continue;
        }
        if is_anthropic_exclusive_extension(k) {
            continue;
        }
        obj.insert(k.clone(), v.clone());
    }
    Value::Object(obj)
}

/// Map Anthropic Messages `thinking` to Responses `reasoning.effort`.
///
/// Official Messages types: `enabled` + `budget_tokens`, `adaptive` (Claude 4.6+),
/// `disabled`. Official Responses effort: `low` | `medium` | `high` | `max`.
fn reasoning_effort_from_thinking(thinking: &Value) -> Option<&'static str> {
    let thinking_type = thinking
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("enabled");
    match thinking_type {
        "disabled" | "none" => None,
        "adaptive" => Some("medium"),
        "enabled" => {
            let budget = thinking
                .get("budget_tokens")
                .and_then(|b| b.as_u64())
                .unwrap_or(4096);
            Some(match budget {
                0 => return None,
                b if b <= 2048 => "low",
                b if b <= 8192 => "medium",
                b if b <= 32768 => "high",
                _ => "max",
            })
        }
        _ => Some("medium"),
    }
}

pub fn decode_anthropic_response(body: &Value) -> IrResponse {
    let blocks = body
        .get("content")
        .map(anthropic_blocks_from_content)
        .unwrap_or_default();
    IrResponse {
        id: body
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("msg-converted")
            .to_string(),
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        blocks,
        stop_reason: body
            .get("stop_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("end_turn")
            .to_string(),
        usage: IrUsage {
            input_tokens: body
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: body
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_read_tokens: body
                .get("usage")
                .and_then(|u| u.get("cache_read_input_tokens"))
                .or_else(|| body.get("usage").and_then(|u| u.get("cache_read_tokens")))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_creation_tokens: body
                .get("usage")
                .and_then(|u| u.get("cache_creation_input_tokens"))
                .or_else(|| {
                    body.get("usage")
                        .and_then(|u| u.get("cache_creation_tokens"))
                })
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        },
    }
}

pub fn decode_openai_chat_response(body: &Value) -> IrResponse {
    let message = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"));
    let mut blocks = openai_content_blocks(message.and_then(|m| m.get("content")));
    if let Some(reasoning) = message
        .and_then(|m| m.get("reasoning_content"))
        .and_then(|v| v.as_str())
        && !reasoning.is_empty()
    {
        blocks.insert(
            0,
            IrBlock::Thinking {
                text: reasoning.to_string(),
                signature: None,
            },
        );
    }
    if let Some(Value::Array(tool_calls)) = message.and_then(|m| m.get("tool_calls")) {
        for call in tool_calls {
            let args = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            blocks.push(IrBlock::ToolUse {
                id: call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                name: call
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string(),
                input: serde_json::from_str(args).unwrap_or_else(|_| Value::Object(Map::new())),
            });
        }
    }
    let finish_reason = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
        .unwrap_or("stop");
    IrResponse {
        id: body
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("chatcmpl-converted")
            .to_string(),
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        blocks,
        stop_reason: finish_reason.to_string(),
        usage: IrUsage {
            input_tokens: body
                .get("usage")
                .and_then(|u| u.get("prompt_tokens"))
                .or_else(|| body.get("usage").and_then(|u| u.get("input_tokens")))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: body
                .get("usage")
                .and_then(|u| u.get("completion_tokens"))
                .or_else(|| body.get("usage").and_then(|u| u.get("output_tokens")))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_read_tokens: body
                .get("usage")
                .and_then(|u| u.get("cache_read_input_tokens"))
                .or_else(|| body.get("usage").and_then(|u| u.get("cache_read_tokens")))
                .or_else(|| {
                    body.get("usage")
                        .and_then(|u| u.get("prompt_tokens_details"))
                        .and_then(|d| d.get("cached_tokens"))
                })
                .or_else(|| {
                    body.get("usage")
                        .and_then(|u| u.get("prompt_cache_hit_tokens"))
                })
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_creation_tokens: body
                .get("usage")
                .and_then(|u| u.get("cache_creation_input_tokens"))
                .or_else(|| {
                    body.get("usage")
                        .and_then(|u| u.get("cache_creation_tokens"))
                })
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        },
    }
}

pub fn decode_responses_response(body: &Value) -> IrResponse {
    let mut blocks = Vec::new();
    let mut stop_reason = "end_turn".to_string();
    if let Some(items) = body.get("output").and_then(|v| v.as_array()) {
        for item in items {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("function_call") => {
                    stop_reason = "tool_use".into();
                    let args = item
                        .get("arguments")
                        .and_then(|a| a.as_str())
                        .unwrap_or("{}");
                    blocks.push(IrBlock::ToolUse {
                        id: item
                            .get("call_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        name: item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        input: serde_json::from_str(args)
                            .unwrap_or_else(|_| Value::Object(Map::new())),
                    });
                }
                Some("reasoning") => {
                    let text = reasoning_text_from_responses_item(item);
                    if !text.is_empty() {
                        blocks.push(IrBlock::Thinking {
                            text,
                            signature: None,
                        });
                    }
                }
                Some("message") | None => {
                    let text = item.get("content").map(text_from_value).unwrap_or_default();
                    if !text.is_empty() {
                        blocks.push(IrBlock::Text { text });
                    }
                }
                _ => {}
            }
        }
    }
    if blocks.is_empty() {
        let text = body
            .get("output_text")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();
        if !text.is_empty() {
            blocks.push(IrBlock::Text { text });
        }
    }
    IrResponse {
        id: body
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("resp-converted")
            .to_string(),
        model: body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        blocks,
        stop_reason,
        usage: IrUsage {
            input_tokens: body
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: body
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_read_tokens: body
                .get("usage")
                .and_then(|u| u.get("cache_read_input_tokens"))
                .or_else(|| body.get("usage").and_then(|u| u.get("cache_read_tokens")))
                .or_else(|| {
                    body.get("usage")
                        .and_then(|u| u.get("input_tokens_details"))
                        .and_then(|d| d.get("cached_tokens"))
                })
                .or_else(|| {
                    body.get("usage")
                        .and_then(|u| u.get("prompt_cache_hit_tokens"))
                })
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_creation_tokens: body
                .get("usage")
                .and_then(|u| u.get("cache_creation_input_tokens"))
                .or_else(|| {
                    body.get("usage")
                        .and_then(|u| u.get("cache_creation_tokens"))
                })
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        },
    }
}

fn reasoning_text_from_responses_item(item: &Value) -> String {
    if let Some(summary) = item.get("summary").and_then(|v| v.as_array()) {
        let joined = summary
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(|t| t.as_str())
                    .or_else(|| part.get("summary_text").and_then(|t| t.as_str()))
            })
            .collect::<Vec<_>>()
            .join("");
        if !joined.is_empty() {
            return joined;
        }
    }
    item.get("content")
        .map(text_from_value)
        .or_else(|| {
            item.get("text")
                .and_then(|t| t.as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default()
}

fn anthropic_stop_reason(stop: &str) -> &'static str {
    match stop {
        "length" | "max_tokens" => "max_tokens",
        "tool_calls" | "tool_use" => "tool_use",
        "stop_sequence" => "stop_sequence",
        _ => "end_turn",
    }
}

fn openai_finish_reason(stop: &str) -> &'static str {
    match stop {
        "max_tokens" | "length" => "length",
        "tool_use" | "tool_calls" => "tool_calls",
        _ => "stop",
    }
}

pub fn encode_anthropic_response(ir: &IrResponse) -> Value {
    let content: Vec<Value> = ir.blocks.iter().map(ir_block_to_anthropic_value).collect();
    let mut usage = serde_json::json!({
        "input_tokens": ir.usage.input_tokens,
        "output_tokens": ir.usage.output_tokens,
    });
    if ir.usage.cache_read_tokens > 0 || ir.usage.cache_creation_tokens > 0 {
        usage["cache_read_input_tokens"] = serde_json::Value::from(ir.usage.cache_read_tokens);
        usage["cache_creation_input_tokens"] =
            serde_json::Value::from(ir.usage.cache_creation_tokens);
    }
    serde_json::json!({
        "id": ir.id,
        "type": "message",
        "role": "assistant",
        "model": ir.model,
        "content": content,
        "stop_reason": anthropic_stop_reason(&ir.stop_reason),
        "usage": usage,
    })
}

pub fn encode_openai_chat_response(ir: &IrResponse) -> Value {
    if ir.blocks.is_empty() {
        return serde_json::json!({
            "id": ir.id,
            "object": "chat.completion",
            "created": 0,
            "model": ir.model,
            "choices": [],
        });
    }
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut tool_calls = Vec::new();
    for block in &ir.blocks {
        match block {
            IrBlock::Text { text: t } => text.push_str(t),
            IrBlock::Thinking { text: t, .. } => reasoning.push_str(t),
            IrBlock::ToolUse { id, name, input } => tool_calls.push(serde_json::json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".into()),
                }
            })),
            _ => {}
        }
    }
    let mut message = serde_json::json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { Value::String(text.clone()) }});
    if text.is_empty() {
        message.as_object_mut().unwrap().remove("content");
    }
    if !reasoning.is_empty() {
        message["reasoning_content"] = Value::String(reasoning);
    }
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    let total = ir.usage.input_tokens + ir.usage.output_tokens;
    let mut usage = serde_json::json!({
        "prompt_tokens": ir.usage.input_tokens,
        "completion_tokens": ir.usage.output_tokens,
        "total_tokens": total,
    });
    if ir.usage.cache_read_tokens > 0 || ir.usage.cache_creation_tokens > 0 {
        usage["cache_read_input_tokens"] = serde_json::Value::from(ir.usage.cache_read_tokens);
        usage["cache_creation_input_tokens"] =
            serde_json::Value::from(ir.usage.cache_creation_tokens);
    }
    serde_json::json!({
        "id": ir.id,
        "object": "chat.completion",
        "created": 0,
        "model": ir.model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": openai_finish_reason(&ir.stop_reason),
        }],
        "usage": usage,
    })
}

pub fn encode_responses_response(ir: &IrResponse, model_fallback: &str) -> Value {
    let mut output = Vec::new();
    let mut all_text = String::new();
    let mut text = String::new();
    let flush_text = |output: &mut Vec<Value>, text: &mut String| {
        if text.is_empty() {
            return;
        }
        output.push(serde_json::json!({
            "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
            "type": "message",
            "role": "assistant",
            "status": "completed",
            "content": [{"type": "output_text", "text": text.clone()}],
        }));
        text.clear();
    };
    for block in &ir.blocks {
        match block {
            IrBlock::Text { text: t } => {
                text.push_str(t);
                all_text.push_str(t);
            }
            IrBlock::Thinking { text: t, .. } => {
                flush_text(&mut output, &mut text);
                all_text.push_str(t);
                output.push(serde_json::json!({
                    "id": format!("rs_{}", uuid::Uuid::new_v4().simple()),
                    "type": "reasoning",
                    "summary": [{"type": "summary_text", "text": t}],
                }));
            }
            IrBlock::ToolUse { id, name, input } => {
                flush_text(&mut output, &mut text);
                output.push(serde_json::json!({
                    "id": format!("fc_{}", uuid::Uuid::new_v4().simple()),
                    "type": "function_call",
                    "call_id": id,
                    "name": name,
                    "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".into()),
                    "status": "completed",
                }));
            }
            _ => {}
        }
    }
    flush_text(&mut output, &mut text);
    if output.is_empty() {
        output.push(serde_json::json!({
            "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
            "type": "message",
            "role": "assistant",
            "status": "completed",
            "content": [{"type": "output_text", "text": ""}],
        }));
    }
    let model = if ir.model.is_empty() || ir.model == "unknown" {
        model_fallback
    } else {
        &ir.model
    };
    let total = ir.usage.input_tokens + ir.usage.output_tokens;
    let mut usage = serde_json::json!({
        "input_tokens": ir.usage.input_tokens,
        "output_tokens": ir.usage.output_tokens,
        "total_tokens": total,
    });
    if ir.usage.cache_read_tokens > 0 || ir.usage.cache_creation_tokens > 0 {
        usage["cache_read_input_tokens"] = serde_json::Value::from(ir.usage.cache_read_tokens);
        usage["cache_creation_input_tokens"] =
            serde_json::Value::from(ir.usage.cache_creation_tokens);
    }
    serde_json::json!({
        "id": ir.id,
        "object": "response",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "output": output,
        "output_text": all_text,
        "usage": usage,
    })
}
