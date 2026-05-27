#![allow(dead_code)]

use anyhow::{anyhow, Result};

// ── LLM backend selection ─────────────────────────────────────────────────────

/// Which backend to send requests to.
#[derive(Debug, Clone)]
pub enum LlmBackend {
    /// Groq cloud API (OpenAI-compatible).
    /// Reads `GROQ_API_KEY` and `SYNAPSE_LOGIC_MODEL` from env.
    Groq,
    /// Local Ollama instance.
    /// Reads `OLLAMA_BASE_URL` (default `http://localhost:11434`) and
    /// `OLLAMA_MODEL` (default `hermes3`) from env.
    Ollama { model: String, base_url: String },
}

impl LlmBackend {
    /// Construct a backend from environment variables.
    /// `LLM_BACKEND=ollama` selects Ollama; anything else (or absent) selects Groq.
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        match std::env::var("LLM_BACKEND")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "ollama" => {
                let model = std::env::var("OLLAMA_MODEL")
                    .unwrap_or_else(|_| "hermes3".to_string());
                let base_url = std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string());
                LlmBackend::Ollama { model, base_url }
            }
            _ => LlmBackend::Groq,
        }
    }

    /// Human-readable label, used in logs.
    pub fn label(&self) -> &str {
        match self {
            LlmBackend::Groq => "groq",
            LlmBackend::Ollama { .. } => "ollama",
        }
    }
}

// ── Message types ─────────────────────────────────────────────────────────────

/// Role of a message in a conversation turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmRole {
    System,
    User,
    Assistant,
    /// Tool-result turn (fed back after a tool call).
    Tool,
}

impl LlmRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmRole::System => "system",
            LlmRole::User => "user",
            LlmRole::Assistant => "assistant",
            // Hermes chatml maps tool responses to "tool" turns.
            // Ollama ≥0.1.24 forwards this correctly for models that support it.
            LlmRole::Tool => "tool",
        }
    }
}

/// A single message in a multi-turn conversation.
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

impl LlmMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: LlmRole::System, content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: LlmRole::User, content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: LlmRole::Assistant, content: content.into() }
    }
    pub fn tool(content: impl Into<String>) -> Self {
        Self { role: LlmRole::Tool, content: content.into() }
    }
}

// ── Request / Response ────────────────────────────────────────────────────────

/// Parameters for a single completion request.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub messages: Vec<LlmMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl LlmRequest {
    pub fn new(messages: Vec<LlmMessage>) -> Self {
        Self {
            messages,
            max_tokens: 1024,
            temperature: 0.3,
        }
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }
}

/// The decoded result of a completion request.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// The assistant's response text.
    pub content: String,
    /// Model name as reported by the API.
    pub model: String,
    /// Which backend produced this response.
    pub backend: String,
    /// Token counts, if the API returned them.
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Route a request to the appropriate backend and return the response.
///
/// Falls back gracefully:
/// - Groq:  returns an error if `GROQ_API_KEY` is not set.
/// - Ollama: returns an error if the local server is unreachable.
pub async fn route_request(backend: &LlmBackend, req: LlmRequest) -> Result<LlmResponse> {
    match backend {
        LlmBackend::Groq => {
            dotenvy::dotenv().ok();
            let api_key = std::env::var("GROQ_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty())
                .ok_or_else(|| {
                    anyhow!(
                        "GROQ_API_KEY is not set. \
                         Set LLM_BACKEND=ollama to use a local model instead."
                    )
                })?;
            let model = std::env::var("SYNAPSE_LOGIC_MODEL")
                .unwrap_or_else(|_| "llama-3.3-70b-versatile".to_string());
            super::ollama::call_openai_compatible(
                "https://api.groq.com/openai/v1",
                Some(&api_key),
                &model,
                req,
                "groq",
            )
            .await
        }
        LlmBackend::Ollama { model, base_url } => {
            let url = format!("{}/v1", base_url);
            super::ollama::call_openai_compatible(&url, None, model, req, "ollama").await
        }
    }
}
