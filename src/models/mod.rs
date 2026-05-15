#![allow(dead_code)]

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const DEFAULT_LOGIC_MODEL: &str = "llama-3.3-70b-versatile";
const DEFAULT_AUDIT_MODEL: &str = "llama-3.3-70b-versatile";
const DEFAULT_OPTIMIZE_MODEL: &str = "llama-3.1-8b-instant";
const REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Copy)]
pub enum ModelRole {
    Logic,
    Audit,
    Optimization,
}

impl ModelRole {
    fn system_prompt(self) -> &'static str {
        match self {
            ModelRole::Logic => {
                "You are a software architect and implementation expert. \
                 Analyze the user's goal and produce a clear, concise implementation \
                 plan or code solution. Be practical and specific."
            }
            ModelRole::Audit => {
                "You are a security and correctness auditor. Review the following for \
                 bugs, security issues, missing edge cases, and risky operations. \
                 Flag anything requiring user approval before execution."
            }
            ModelRole::Optimization => {
                "You are a performance optimization expert focused on memory efficiency \
                 and clean async design. Suggest the most impactful improvements only. \
                 Be brief and specific."
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ModelRole::Logic => "logic",
            ModelRole::Audit => "audit",
            ModelRole::Optimization => "optimization",
        }
    }
}

pub struct ModelConfig {
    pub api_key: Option<String>,
    pub logic_model: String,
    pub audit_model: String,
    pub optimize_model: String,
}

impl ModelConfig {
    pub fn from_env() -> Self {
        // Load .env if present; silently ignore missing file
        dotenvy::dotenv().ok();

        let api_key = std::env::var("GROQ_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty());

        let logic_model = std::env::var("SYNAPSE_LOGIC_MODEL")
            .unwrap_or_else(|_| DEFAULT_LOGIC_MODEL.to_string());
        let audit_model = std::env::var("SYNAPSE_AUDIT_MODEL")
            .unwrap_or_else(|_| DEFAULT_AUDIT_MODEL.to_string());
        let optimize_model = std::env::var("SYNAPSE_OPTIMIZE_MODEL")
            .unwrap_or_else(|_| DEFAULT_OPTIMIZE_MODEL.to_string());

        Self {
            api_key,
            logic_model,
            audit_model,
            optimize_model,
        }
    }

    pub fn is_online(&self) -> bool {
        self.api_key.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub role: ModelRole,
    pub model_name: String,
    pub content: String,
    pub offline: bool,
}

#[derive(Debug)]
pub struct ConsensusResult {
    pub logic: ModelResponse,
    pub audit: ModelResponse,
    pub optimize: ModelResponse,
}

impl ConsensusResult {
    pub fn summary(&self) -> String {
        let mode = if self.logic.offline { "OFFLINE" } else { "ONLINE" };
        format!(
            "[{}] logic={} chars  audit={} chars  optimize={} chars",
            mode,
            self.logic.content.len(),
            self.audit.content.len(),
            self.optimize.content.len(),
        )
    }
}

// Minimal structs for deserialising the OpenAI-compatible Groq response
#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
}

#[derive(Deserialize)]
struct ApiChoice {
    message: ApiMessage,
}

#[derive(Deserialize)]
struct ApiMessage {
    content: String,
}

async fn call_model(
    client: &Client,
    api_key: &str,
    model: &str,
    role: ModelRole,
    prompt: &str,
) -> Result<ModelResponse> {
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": role.system_prompt() },
            { "role": "user",   "content": prompt },
        ],
        "max_tokens": 1024,
        "temperature": 0.3,
    });

    let response = client
        .post(GROQ_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        // Read body for context but never echo the key back
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Groq API error {}: {}", status, text));
    }

    let parsed: ApiResponse = response.json().await?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_else(|| "(empty response)".to_string());

    Ok(ModelResponse {
        role,
        model_name: model.to_string(),
        content,
        offline: false,
    })
}

fn offline_response(role: ModelRole, model_name: &str) -> ModelResponse {
    let content = match role {
        ModelRole::Logic => {
            "OFFLINE — no GROQ_API_KEY found.\n\
             Fallback plan: map project structure, identify target file, \
             apply minimal required change, write a sandbox test."
                .to_string()
        }
        ModelRole::Audit => {
            "OFFLINE — audit skipped.\n\
             Reminder: never execute destructive host commands without explicit approval. \
             Verify sandbox isolation before running generated code."
                .to_string()
        }
        ModelRole::Optimization => {
            "OFFLINE — optimization skipped.\n\
             Reminder: avoid loading entire files into memory; \
             cap log buffers; prefer streaming over bulk reads."
                .to_string()
        }
    };
    ModelResponse {
        role,
        model_name: model_name.to_string(),
        content,
        offline: true,
    }
}

/// Call all three model roles against `prompt` and return their responses.
/// Falls back to deterministic offline responses when GROQ_API_KEY is absent.
pub async fn run_consensus(prompt: &str) -> Result<ConsensusResult> {
    let config = ModelConfig::from_env();

    if !config.is_online() {
        return Ok(ConsensusResult {
            logic: offline_response(ModelRole::Logic, &config.logic_model),
            audit: offline_response(ModelRole::Audit, &config.audit_model),
            optimize: offline_response(ModelRole::Optimization, &config.optimize_model),
        });
    }

    let api_key = config.api_key.as_deref().unwrap();
    let client = Client::new();

    let logic =
        call_model(&client, api_key, &config.logic_model, ModelRole::Logic, prompt).await?;
    let audit =
        call_model(&client, api_key, &config.audit_model, ModelRole::Audit, prompt).await?;
    let optimize = call_model(
        &client,
        api_key,
        &config.optimize_model,
        ModelRole::Optimization,
        prompt,
    )
    .await?;

    Ok(ConsensusResult {
        logic,
        audit,
        optimize,
    })
}
