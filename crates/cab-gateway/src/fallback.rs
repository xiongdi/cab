use axum::response::Response;
use cab_core::CabError;
use reqwest::Client;

use crate::router::ResolvedModel;

/// Request info needed for fallback execution.
pub struct ProxyRequest {
    pub body: bytes::Bytes,
    pub headers: axum::http::HeaderMap,
    pub stream: bool,
    pub path_suffix: String,
    /// Model id embedded in upstream URL (Gemini native API).
    pub url_model: Option<String>,
}

fn is_gemini_path(path_suffix: &str) -> bool {
    path_suffix.contains("generateContent")
}

fn is_messages_path(path_suffix: &str) -> bool {
    path_suffix == "v1/messages" || path_suffix == "messages"
}

fn is_responses_path(path_suffix: &str) -> bool {
    path_suffix == "responses"
}

fn target_model_name(resolved: &ResolvedModel, fallback: &str) -> String {
    resolved
        .model
        .links
        .as_ref()
        .and_then(|links| links.get("native_model_id"))
        .and_then(|native| native.as_str())
        .filter(|native| !native.trim().is_empty())
        .map(|native| native.to_string())
        .unwrap_or_else(|| {
            if let Some(pos) = resolved.model.name.find('/') {
                resolved.model.name[pos + 1..].to_string()
            } else {
                fallback.to_string()
            }
        })
}

fn build_upstream_url(
    endpoint: &cab_core::types::ProviderEndpoint,
    request: &ProxyRequest,
    resolved: &ResolvedModel,
) -> String {
    let base = endpoint.url.trim_end_matches('/').to_string();
    match endpoint.protocol.as_str() {
        "anthropic" => {
            if base.ends_with("/v1/messages") || base.ends_with("/messages") {
                base
            } else if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        "openai-responses" => {
            if base.ends_with("/v1/responses") || base.ends_with("/responses") {
                base
            } else if base.ends_with("/v1") {
                format!("{base}/responses")
            } else {
                format!("{base}/v1/responses")
            }
        }
        "gemini" => {
            let model_name = request
                .url_model
                .as_deref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| target_model_name(resolved, &resolved.model.name));
            let action = if request.stream || request.path_suffix.contains("streamGenerateContent")
            {
                "streamGenerateContent?alt=sse"
            } else {
                "generateContent"
            };
            if base.contains(":generateContent") || base.contains(":streamGenerateContent") {
                base
            } else if base.ends_with("/models") {
                format!("{base}/{model_name}:{action}")
            } else {
                format!("{base}/models/{model_name}:{action}")
            }
        }
        _ => {
            if base.ends_with("/v1/chat/completions") || base.ends_with("/chat/completions") {
                base
            } else if base.ends_with("/v1") {
                format!("{base}/chat/completions")
            } else {
                format!("{base}/v1/chat/completions")
            }
        }
    }
}

/// Try each model in order, cycling through its endpoint candidates on 5xx errors.
pub async fn execute_with_fallback(
    client: &Client,
    primary: &ResolvedModel,
    fallbacks: &[ResolvedModel],
    request: &ProxyRequest,
) -> Result<(Response, String, String), CabError> {
    // Try primary first, then fallbacks
    let all_models = std::iter::once(primary).chain(fallbacks.iter());

    let mut last_error = CabError::Proxy("No models available".to_string());

    for resolved in all_models {
        // If no endpoints match the model's protocol, skip this model
        if resolved.endpoint_candidates.is_empty() {
            tracing::warn!(
                "No endpoint matches model {} protocol {}",
                resolved.model.name,
                resolved.model_protocol
            );
            last_error = CabError::Proxy(format!(
                "no endpoint matches model protocol {} for model {}",
                resolved.model_protocol, resolved.model.name
            ));
            continue;
        }

        // Try each endpoint in priority order
        let mut endpoint_error = None;
        for endpoint in &resolved.endpoint_candidates {
            let upstream_url = build_upstream_url(endpoint, request, resolved);

            tracing::info!(
                "Trying model {} via {} [{}] at {}",
                resolved.model.name,
                resolved.provider_name,
                endpoint.protocol,
                upstream_url
            );

            // Rewrite model in JSON request body to match the target routed model
            let is_messages_path = is_messages_path(&request.path_suffix);
            let is_responses_path = is_responses_path(&request.path_suffix);
            let is_gemini_path = is_gemini_path(&request.path_suffix);
            let needs_responses_shim = endpoint.protocol != "openai-responses" && is_responses_path;
            let needs_gemini_stream_shim =
                endpoint.protocol != "gemini" && is_gemini_path && request.stream;

            let mut rewritten_body = request.body.clone();
            if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&request.body) {
                let mut converted_val = if endpoint.protocol == "anthropic"
                    && request.path_suffix == "chat/completions"
                {
                    crate::protocol::openai_to_anthropic(&json_val)
                } else if endpoint.protocol == "gemini" && request.path_suffix == "chat/completions"
                {
                    crate::protocol::openai_to_gemini(&json_val)
                } else if endpoint.protocol != "gemini" && is_gemini_path {
                    crate::protocol::gemini_to_openai_chat_request(&json_val)
                } else if endpoint.protocol != "anthropic" && is_messages_path {
                    crate::protocol::anthropic_to_openai_chat_request(&json_val)
                } else if endpoint.protocol != "openai-responses" && is_responses_path {
                    crate::protocol::responses_to_chat_request(&json_val)
                } else {
                    json_val.clone()
                };

                if let Some(obj) = converted_val.as_object_mut() {
                    let target_model_name = target_model_name(resolved, &resolved.model.name);

                    // Native Gemini uses model in URL; converted Gemini→OpenAI bodies need `model`.
                    if !(endpoint.protocol == "gemini" && is_gemini_path) {
                        obj.insert(
                            "model".to_string(),
                            serde_json::Value::String(target_model_name),
                        );
                    }
                    if needs_responses_shim || needs_gemini_stream_shim {
                        obj.insert("stream".to_string(), serde_json::Value::Bool(false));
                    }
                }
                if let Ok(new_body_bytes) = serde_json::to_vec(&converted_val) {
                    rewritten_body = bytes::Bytes::from(new_body_bytes);
                }
            }

            let upstream_stream = if needs_responses_shim || needs_gemini_stream_shim {
                false
            } else {
                request.stream
            };

            match crate::proxy::proxy_request(
                client,
                &upstream_url,
                &resolved.provider_api_key,
                &endpoint.protocol,
                rewritten_body,
                &request.headers,
                upstream_stream,
            )
            .await
            {
                Ok(response) => {
                    let mut final_response = response;

                    if endpoint.protocol == "anthropic"
                        && request.path_suffix == "chat/completions"
                        && !request.stream
                    {
                        let (parts, body) = final_response.into_parts();
                        final_response = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
                            Ok(body_bytes) => {
                                if let Ok(anthropic_json) =
                                    serde_json::from_slice::<serde_json::Value>(&body_bytes)
                                {
                                    let openai_json =
                                        crate::protocol::anthropic_to_openai(&anthropic_json);
                                    match serde_json::to_vec(&openai_json) {
                                        Ok(new_body_bytes) => Response::from_parts(
                                            parts,
                                            axum::body::Body::from(new_body_bytes),
                                        ),
                                        Err(_) => Response::from_parts(
                                            parts,
                                            axum::body::Body::from(body_bytes),
                                        ),
                                    }
                                } else {
                                    Response::from_parts(parts, axum::body::Body::from(body_bytes))
                                }
                            }
                            Err(_) => Response::from_parts(parts, axum::body::Body::empty()),
                        };
                    } else if endpoint.protocol != "anthropic"
                        && is_messages_path
                        && !request.stream
                    {
                        let (parts, body) = final_response.into_parts();
                        final_response = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
                            Ok(body_bytes) => {
                                if let Ok(openai_json) =
                                    serde_json::from_slice::<serde_json::Value>(&body_bytes)
                                {
                                    let anthropic_json =
                                        crate::protocol::openai_chat_to_anthropic_messages(
                                            &openai_json,
                                        );
                                    match serde_json::to_vec(&anthropic_json) {
                                        Ok(new_body_bytes) => Response::from_parts(
                                            parts,
                                            axum::body::Body::from(new_body_bytes),
                                        ),
                                        Err(_) => Response::from_parts(
                                            parts,
                                            axum::body::Body::from(body_bytes),
                                        ),
                                    }
                                } else {
                                    Response::from_parts(parts, axum::body::Body::from(body_bytes))
                                }
                            }
                            Err(_) => Response::from_parts(parts, axum::body::Body::empty()),
                        };
                    } else if endpoint.protocol == "gemini"
                        && request.path_suffix == "chat/completions"
                        && !request.stream
                    {
                        let (parts, body) = final_response.into_parts();
                        final_response = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
                            Ok(body_bytes) => {
                                if let Ok(gemini_json) =
                                    serde_json::from_slice::<serde_json::Value>(&body_bytes)
                                {
                                    let openai_json = crate::protocol::gemini_to_openai(
                                        &gemini_json,
                                        &resolved.model.name,
                                    );
                                    match serde_json::to_vec(&openai_json) {
                                        Ok(new_body_bytes) => Response::from_parts(
                                            parts,
                                            axum::body::Body::from(new_body_bytes),
                                        ),
                                        Err(_) => Response::from_parts(
                                            parts,
                                            axum::body::Body::from(body_bytes),
                                        ),
                                    }
                                } else {
                                    Response::from_parts(parts, axum::body::Body::from(body_bytes))
                                }
                            }
                            Err(_) => Response::from_parts(parts, axum::body::Body::empty()),
                        };
                    } else if endpoint.protocol != "gemini" && is_gemini_path && !request.stream {
                        let (parts, body) = final_response.into_parts();
                        final_response = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
                            Ok(body_bytes) => {
                                if let Ok(openai_json) =
                                    serde_json::from_slice::<serde_json::Value>(&body_bytes)
                                {
                                    let gemini_json = crate::protocol::openai_chat_to_gemini(
                                        &openai_json,
                                        &resolved.model.name,
                                    );
                                    match serde_json::to_vec(&gemini_json) {
                                        Ok(new_body_bytes) => Response::from_parts(
                                            parts,
                                            axum::body::Body::from(new_body_bytes),
                                        ),
                                        Err(_) => Response::from_parts(
                                            parts,
                                            axum::body::Body::from(body_bytes),
                                        ),
                                    }
                                } else {
                                    Response::from_parts(parts, axum::body::Body::from(body_bytes))
                                }
                            }
                            Err(_) => Response::from_parts(parts, axum::body::Body::empty()),
                        };
                    } else if endpoint.protocol != "openai-responses" && is_responses_path {
                        let (parts, body) = final_response.into_parts();
                        final_response = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
                            Ok(body_bytes) => {
                                if let Ok(openai_json) =
                                    serde_json::from_slice::<serde_json::Value>(&body_bytes)
                                {
                                    let responses_json = crate::protocol::chat_to_responses(
                                        &openai_json,
                                        &resolved.model.name,
                                    );
                                    if request.stream {
                                        let sse = crate::protocol::responses_to_sse_stream(
                                            &responses_json,
                                        );
                                        let mut response = Response::from_parts(
                                            parts,
                                            axum::body::Body::from(sse),
                                        );
                                        response.headers_mut().insert(
                                            axum::http::header::CONTENT_TYPE,
                                            "text/event-stream".parse().unwrap(),
                                        );
                                        response
                                    } else {
                                        match serde_json::to_vec(&responses_json) {
                                            Ok(new_body_bytes) => Response::from_parts(
                                                parts,
                                                axum::body::Body::from(new_body_bytes),
                                            ),
                                            Err(_) => Response::from_parts(
                                                parts,
                                                axum::body::Body::from(body_bytes),
                                            ),
                                        }
                                    }
                                } else {
                                    Response::from_parts(parts, axum::body::Body::from(body_bytes))
                                }
                            }
                            Err(_) => Response::from_parts(parts, axum::body::Body::empty()),
                        };
                    }
                    return Ok((
                        final_response,
                        resolved.provider_name.clone(),
                        resolved.model.name.clone(),
                    ));
                }
                Err(CabError::ProviderError { status, body }) if status >= 500 => {
                    tracing::warn!(
                        "Provider {} endpoint {} returned {status}, trying next endpoint: {body}",
                        resolved.provider_name,
                        endpoint.url
                    );
                    endpoint_error = Some(CabError::ProviderError { status, body });
                }
                Err(CabError::Proxy(msg)) => {
                    tracing::warn!(
                        "Provider {} endpoint {} proxy error, trying next endpoint: {msg}",
                        resolved.provider_name,
                        endpoint.url
                    );
                    endpoint_error = Some(CabError::Proxy(msg));
                }
                Err(e) => {
                    // Non-retriable error — return immediately
                    return Err(e);
                }
            }
        }

        // All endpoints for this model failed; record the last error and try next model
        if let Some(e) = endpoint_error {
            last_error = e;
        }
    }

    Err(last_error)
}
