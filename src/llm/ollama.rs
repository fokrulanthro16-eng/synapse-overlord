#![allow(dead_code)]

//! OpenAI-compatible HTTP client.
//!
//! Used for both Groq (cloud) and Ollama (local) — they share the same
//! `/v1/chat/completions` wire format.  Auth header is added only when
//! `api_key` is `Some`; Ollama does not require authentication.

use std::time::Duration;

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use super::router::{LlmRequest, LlmResponse};

const REQUEST_TIMEOUT_SECS: u64 = 60;

// ── Minimal deserialization structs ───────────────────────────────────────────

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
    model: String,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ApiChoice {
    message: ApiMessage,
}

#[derive(Deserialize)]
struct ApiMessage {
    content: String,
}

#[derive(Deserialize)]
struct ApiUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Send a chat-completion request to any OpenAI-compatible endpoint.
///
/// - `base_url`    — e.g. `"https://api.groq.com/openai/v1"` or
///                   `"http://localhost:11434/v1"`
/// - `api_key`     — `None` for Ollama; `Some(key)` for Groq / other cloud APIs
/// - `model`       — model name string passed directly to the API
/// - `req`         — conversation messages + generation parameters
/// - `backend_label` — human-readable tag stored in `LlmResponse::backend`
pub async fn call_openai_compatible(
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    req: LlmRequest,
    backend_label: &str,
) -> Result<LlmResponse> {
    let messages: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
        .collect();

    let body = json!({
        "model":       model,
        "messages":    messages,
        "max_tokens":  req.max_tokens,
        "temperature": req.temperature,
    });

    let client = Client::new();
    let mut builder = client
        .post(format!("{}/chat/completions", base_url))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS));

    if let Some(key) = api_key {
        builder = builder.header("Authorization", format!("Bearer {}", key));
    }

    let response = builder.send().await.map_err(|e| {
        anyhow!(
            "Failed to connect to LLM backend '{}': {}",
            backend_label,
            e
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        // Truncate to avoid leaking large payloads or API keys echoed in errors
        let snippet = &body_text[..body_text.len().min(300)];
        return Err(anyhow!(
            "LLM API error {} (backend={}): {}",
            status,
            backend_label,
            snippet
        ));
    }

    let parsed: ApiResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse LLM response from '{}': {}", backend_label, e))?;

    let content = parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_else(|| "(empty response)".to_string());

    let (input_tokens, output_tokens) = parsed
        .usage
        .map(|u| (u.prompt_tokens, u.completion_tokens))
        .unwrap_or((None, None));

    Ok(LlmResponse {
        content,
        model: parsed.model,
        backend: backend_label.to_string(),
        input_tokens,
        output_tokens,
    })
}
