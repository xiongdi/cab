//! Vision-capability detection for request routing.
//!
//! Coding-agent requests can embed images (screenshots, diagrams, UI mockups)
//! in several wire formats depending on the client protocol:
//!
//! - Anthropic Messages: a content block `{"type": "image", "source": {...}}`
//!   with either a base64 `data` or a public `url`.
//! - OpenAI Chat Completions: a content part `{"type": "image_url",
//!   "image_url": {"url": ...}}`.
//! - OpenAI Responses: an input item whose `content` uses the image block above.
//! - Text-only fallback: some clients inline an image as a `data:image/...;base64`
//!   URI or a `http(s)://` image URL inside a plain string.
//!
//! When a request carries an image, the chosen model must support the `image`
//! input modality — otherwise the upstream rejects it (400) or the model
//! produces garbage. [`request_requires_vision`] detects the image;
//! [`model_supports_vision`] checks the catalog modality data (models.dev
//! `architecture.modalities.input`) with a slug-based fallback for models that
//! lack explicit modality data.

use crate::types::Model;

/// Whether a request body embeds an image anywhere in its input.
///
/// Checks the `messages` / `contents` / `input` arrays of the three supported
/// client protocols (Anthropic, OpenAI Chat, OpenAI Responses) and falls back to
/// a lightweight scan of the already-extracted request text for `data:image/`
/// URIs and image URLs. Returns `false` for bodies without messages.
///
/// The caller supplies `text` (the output of [`crate::routing`]'s text
/// extraction) so the body is only stringified once per request.
pub fn request_requires_vision(body: &serde_json::Value, text: &str) -> bool {
    for key in ["messages", "contents", "input"] {
        if let Some(array) = body.get(key).and_then(|v| v.as_array())
            && array.iter().any(contains_image)
        {
            return true;
        }
    }
    // Last resort: a data-URI or http(s) image URL embedded in plain text.
    text.contains(DATA_IMAGE_PREFIX) || looks_like_image_url(text)
}

const DATA_IMAGE_PREFIX: &str = "data:image/";

/// True when a content value (string, block array, or part object) contains an
/// image block of any supported shape.
fn contains_image(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => s.contains(DATA_IMAGE_PREFIX) || looks_like_image_url(s),
        serde_json::Value::Array(items) => items.iter().any(contains_image),
        serde_json::Value::Object(obj) => {
            if let Some(kind) = obj.get("type").and_then(|t| t.as_str())
                && (kind == "image" || kind == "image_url")
            {
                return true;
            }
            // Anthropic blocks carry the source object on the block itself.
            if obj.get("source").map(|s| !s.is_null()).unwrap_or(false) {
                return true;
            }
            if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                return text.contains(DATA_IMAGE_PREFIX) || looks_like_image_url(text);
            }
            // OpenAI Responses `function_call_output` items carry the tool result
            // in `output` (e.g. Codex's `view_image` returns the base64 image here),
            // not under `content`/`parts`. Treat it like any other string content.
            if let Some(out) = obj.get("output") {
                return contains_image(out);
            }
            // Recurse into nested content: OpenAI Responses `input` items are
            // `{"type": "message", "content": [parts]}`; Gemini contents items
            // nest parts under `parts`.
            if let Some(nested) = obj.get("content").or_else(|| obj.get("parts")) {
                return contains_image(nested);
            }
            false
        }
        _ => false,
    }
}

/// Cheap heuristic for image URLs embedded in plain text. We deliberately keep
/// this narrow to avoid flagging code snippets (`example.com/foo.png` paths in
/// source) as images — only URLs whose path clearly denotes an image file or a
/// well-known image host (screenshot domains, `imgur`, `images.unsplash.com`)
/// count.
fn looks_like_image_url(text: &str) -> bool {
    // Cheap case-insensitive scan before allocating a lowercase copy; most
    // request text has no URL at all.
    if !text
        .as_bytes()
        .windows(4)
        .any(|w| w.eq_ignore_ascii_case(b"http"))
    {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    let has_image_ext = [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".avif",
    ]
    .iter()
    .any(|ext| lower.contains(ext));
    if has_image_ext {
        return true;
    }
    [
        "imgur.com",
        "images.unsplash.com",
        "screenshot",
        "s3.amazonaws.com/screenshot",
    ]
    .iter()
    .any(|host| lower.contains(host))
}

/// Whether a model accepts image input, per models.dev `architecture.modalities.input`.
pub fn model_supports_vision(model: &Model) -> bool {
    if let Some(modalities) = model
        .architecture
        .as_ref()
        .and_then(|a| a.get("modalities"))
        .and_then(|m| m.get("input"))
        .and_then(|m| m.as_array())
    {
        let has_image = modalities.iter().any(|m| m.as_str() == Some("image"));
        // Explicit modality data is authoritative: text-only models report
        // `["text"]`, vision models include `"image"`.
        return has_image;
    }

    // Fallback: models without explicit modalities data are matched by slug
    // against well-known vision-capable families.
    let slug = model
        .canonical_slug
        .as_deref()
        .or(Some(model.name.as_str()))
        .unwrap_or_default()
        .to_ascii_lowercase();
    KNOWN_VISION_FAMILIES
        .iter()
        .any(|family| slug.contains(family))
}

/// Slug fragments that identify vision-capable model families. Only used when
/// the catalog provides no explicit `modalities` data for a model.
const KNOWN_VISION_FAMILIES: &[&str] = &[
    "claude-opus",
    "claude-sonnet",
    "claude-haiku",
    // Gemini is multimodal by default across the family.
    "gemini",
    "gpt-4o",
    "gpt-4.1",
    "gpt-5",
    "gpt-image",
    // Chinese vendors' vision (VL) / omni lines.
    "qwen-vl",
    "qwen2-vl",
    "qwen2.5-vl",
    "qwen3-vl",
    "qwen-omni",
    "glm-4v",
    "glm-5v",
    "glm-v",
    "step-1v",
    "kimi-latest",
    "moonshot-v1-8k-vision",
    "moonshot-v1-32k-vision",
    "moonshot-v1-128k-vision",
    "grok-4",
    "grok-4.1",
    "grok-3",
    // MiniMax M-series, Mistral vision
    "minimax-m",
    "mistral-vision",
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn requires_vision(body: &serde_json::Value) -> bool {
        let text = crate::routing::extract_request_text(body);
        request_requires_vision(body, &text)
    }

    fn model_with_modalities(input: &[&str]) -> Model {
        let mut model =
            crate::routing::tests::sample_model("test-model", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        model.architecture = Some(json!({
            "modalities": { "input": input, "output": ["text"] }
        }));
        model
    }

    fn text_only_model() -> Model {
        let mut model = crate::routing::tests::sample_model(
            "deepseek/deepseek-v4-flash",
            1.0,
            1.0,
            (50.0, 50.0, 40.0, 40.0),
        );
        model.canonical_slug = Some("deepseek/deepseek-v4-flash".into());
        model
    }

    #[test]
    fn detects_anthropic_image_block() {
        let body = json!({
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "what is this"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}
                ]
            }]
        });
        assert!(requires_vision(&body));
    }

    #[test]
    fn detects_openai_image_url_part() {
        let body = json!({
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "look"},
                    {"type": "image_url", "image_url": {"url": "https://example.com/a.png"}}
                ]
            }]
        });
        assert!(requires_vision(&body));
    }

    #[test]
    fn detects_data_uri_in_plain_string() {
        let body = json!({
            "messages": [{"role": "user", "content": "here: data:image/png;base64,AAABBB"}]
        });
        assert!(requires_vision(&body));
    }

    #[test]
    fn plain_text_without_images_is_not_vision() {
        let body = json!({
            "messages": [{"role": "user", "content": "refactor this Rust function"}]
        });
        assert!(!requires_vision(&body));
    }

    #[test]
    fn responses_api_input_image_detected() {
        let body = json!({
            "input": [{
                "role": "user",
                "content": [{"type": "image_url", "image_url": {"url": "https://x.com/foo.webp"}}]
            }]
        });
        assert!(requires_vision(&body));
    }

    #[test]
    fn detects_image_in_function_call_output() {
        // Codex's `view_image` returns the base64 image in a
        // `function_call_output` item's `output`, not under `content`/`parts`.
        let body = json!({
            "input": [{
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg="
            }]
        });
        assert!(requires_vision(&body));
    }

    #[test]
    fn code_snippet_urls_are_not_treated_as_images() {
        let body = json!({
            "messages": [{"role": "user", "content": "see example.com/api/path usage below"}]
        });
        assert!(!requires_vision(&body));
    }

    #[test]
    fn explicit_modalities_are_authoritative() {
        assert!(model_supports_vision(&model_with_modalities(&[
            "text", "image"
        ])));
        assert!(!model_supports_vision(&model_with_modalities(&["text"])));
    }

    #[test]
    fn slug_fallback_detects_vision_families() {
        assert!(!model_supports_vision(&text_only_model()));
        let mut claude = crate::routing::tests::sample_model(
            "anthropic/claude-opus-4-8",
            1.0,
            1.0,
            (50.0, 50.0, 40.0, 40.0),
        );
        claude.canonical_slug = Some("anthropic/claude-opus-4-8".into());
        assert!(model_supports_vision(&claude));
    }
}
