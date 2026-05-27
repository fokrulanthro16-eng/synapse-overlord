#![allow(unused_imports)] // Phase 3 will consume LlmResponse via route handlers

/// LLM abstraction layer for Synapse-Overlord.
///
/// Provides a unified `route_request` entry-point that dispatches to either:
/// - Groq  (cloud, OpenAI-compatible API)
/// - Ollama (local, same wire format)
///
/// The Hermes sub-module handles prompt building and tool-call parsing
/// for the Hermes-2-Pro chatml tool-use format.
pub mod hermes;
pub mod ollama;
pub mod router;

pub use router::{LlmBackend, LlmMessage, LlmRequest, LlmResponse, LlmRole, route_request};
