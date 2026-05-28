#![allow(dead_code)]

//! Hermes-2-Pro chatml prompt builder and tool-call parser.
//!
//! Hermes-2-Pro (NousResearch) uses a chatml variant where tool calls are
//! expressed as JSON inside `<tool_call>…</tool_call>` tags, and tool
//! results are returned inside `<tool_response>…</tool_response>` tags.
//!
//! This module is purely text-manipulation — no I/O, no async.

use crate::llm::router::{LlmMessage, LlmRole};
use crate::tools::{ToolCallRequest, ToolDefinition};

// ── Prompt construction ───────────────────────────────────────────────────────

/// Build the system prompt that:
/// 1. Gives Hermes its persona and role.
/// 2. Injects the tool schemas as JSON inside `<tools>` XML tags.
/// 3. Explains the exact output format required for tool calls.
/// 4. States the safety constraints.
pub fn build_system_prompt(tools: &[ToolDefinition]) -> String {
    let tool_schemas: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name":        t.name,
                    "description": t.description,
                    "parameters":  t.parameters,
                }
            })
        })
        .collect();

    let tools_json = serde_json::to_string_pretty(&tool_schemas)
        .unwrap_or_else(|_| "[]".to_string());

    format!(
        "You are Hermes, an expert software architect AI assistant embedded in \
Synapse-Overlord, a Rust project analysis and generation tool.\n\n\
You have access to the following read-only tools to inspect the codebase:\n\
<tools>\n{tools_json}\n</tools>\n\n\
## How to call a tool\n\
Output a JSON object inside <tool_call></tool_call> tags — nothing else on \
those lines:\n\n\
<tool_call>\n\
{{\"name\": \"tool_name\", \"arguments\": {{\"key\": \"value\"}}}}\n\
</tool_call>\n\n\
After receiving the tool result you will see it in a <tool_response> block. \
Continue calling tools as needed, then provide your final answer in plain text \
(no tags).\n\n\
## Safety rules\n\
- NEVER attempt to access files outside the project root.\n\
- NEVER propose shell commands — use the provided tools only.\n\
- ALWAYS explain your reasoning before and after tool calls.\n\
- When proposing code changes, describe the change clearly and wait for \
explicit user approval before anything is written to disk.",
        tools_json = tools_json,
    )
}

/// Build the initial user message for an architect session.
pub fn build_user_message(goal: &str, project_summary: &str) -> String {
    format!(
        "## Goal\n{goal}\n\n\
## Project snapshot\n{summary}\n\n\
Please analyse the project and help accomplish the goal. \
Use the available tools to inspect any files you need.",
        goal    = goal,
        summary = project_summary,
    )
}

/// Assemble the initial two-message conversation (system + first user turn).
pub fn build_conversation(
    goal: &str,
    project_summary: &str,
    tools: &[ToolDefinition],
) -> Vec<LlmMessage> {
    vec![
        LlmMessage {
            role:    LlmRole::System,
            content: build_system_prompt(tools),
        },
        LlmMessage {
            role:    LlmRole::User,
            content: build_user_message(goal, project_summary),
        },
    ]
}

// ── Tool-call parsing ─────────────────────────────────────────────────────────

/// Parse every `<tool_call>…</tool_call>` block in `response`.
///
/// - Uses slice scanning (no regex, no extra deps).
/// - Skips blocks whose inner JSON is malformed (logs a warning to stderr).
/// - Returns an empty `Vec` when no valid tool calls are present.
pub fn parse_tool_calls(response: &str) -> Vec<ToolCallRequest> {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    let mut calls = Vec::new();
    let mut cursor = response;

    loop {
        // Find the next opening tag
        let start = match cursor.find(OPEN) {
            Some(i) => i,
            None => break,
        };

        let after_open = &cursor[start + OPEN.len()..];

        // Find the matching closing tag
        let end = match after_open.find(CLOSE) {
            Some(i) => i,
            None => break, // unclosed tag — stop
        };

        let json_str = after_open[..end].trim();

        match serde_json::from_str::<ToolCallRequest>(json_str) {
            Ok(call) => calls.push(call),
            Err(e) => {
                eprintln!(
                    "[hermes] Skipping malformed tool_call JSON: {:?} — error: {}",
                    &json_str[..json_str.len().min(80)],
                    e
                );
            }
        }

        // Advance cursor past this closing tag
        cursor = &after_open[end + CLOSE.len()..];
    }

    calls
}

// ── Tool-result message builder ───────────────────────────────────────────────

/// Wrap a tool result in the Hermes `<tool_response>` chatml format and
/// return it as a `User`-role message ready to append to the conversation.
///
/// We use `User` (not `Tool`) because the agent loop drives tool calls via
/// text-based `<tool_call>` tags — not the OpenAI native `tool_calls` API.
/// Groq and other OpenAI-compatible APIs require a matching `tool_call_id`
/// on every `role: "tool"` message; since we never emit official tool_calls
/// objects there is no ID to attach, so the call would be rejected with 400.
/// Embedding the result as a user turn is semantically equivalent here.
pub fn build_tool_result_message(tool_name: &str, result: &str) -> LlmMessage {
    LlmMessage {
        role: LlmRole::User,
        content: format!(
            "<tool_response>\n\
<name>{tool_name}</name>\n\
<result>\n{result}\n</result>\n\
</tool_response>",
            tool_name = tool_name,
            result    = result,
        ),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_tool_call() {
        let text = r#"Let me read the file.
<tool_call>
{"name": "file_read", "arguments": {"path": "src/main.rs"}}
</tool_call>
Awaiting result."#;

        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "file_read");
        assert_eq!(calls[0].arguments["path"], "src/main.rs");
    }

    #[test]
    fn parses_multiple_tool_calls() {
        let text = r#"
<tool_call>
{"name": "file_list", "arguments": {"path": "src"}}
</tool_call>
<tool_call>
{"name": "file_read", "arguments": {"path": "Cargo.toml"}}
</tool_call>"#;
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "file_list");
        assert_eq!(calls[1].name, "file_read");
    }

    #[test]
    fn returns_empty_for_plain_text() {
        let calls = parse_tool_calls("I have finished the analysis.");
        assert!(calls.is_empty());
    }

    #[test]
    fn skips_malformed_json_silently() {
        let text = "<tool_call>\n{bad json here}\n</tool_call>";
        let calls = parse_tool_calls(text);
        assert!(calls.is_empty()); // malformed — skipped
    }

    #[test]
    fn ignores_unclosed_tag() {
        let text = "<tool_call>\n{\"name\":\"file_read\",\"arguments\":{\"path\":\"x\"}}";
        let calls = parse_tool_calls(text);
        assert!(calls.is_empty()); // no closing tag
    }

    #[test]
    fn tool_result_message_has_user_role() {
        let msg = build_tool_result_message("file_read", "fn main() {}");
        assert_eq!(msg.role, LlmRole::User);
        assert!(msg.content.contains("<name>file_read</name>"));
        assert!(msg.content.contains("fn main()"));
    }

    #[test]
    fn system_prompt_contains_tool_names() {
        use crate::tools::available_tools;
        let tools = available_tools();
        let prompt = build_system_prompt(&tools);
        assert!(prompt.contains("file_read"));
        assert!(prompt.contains("file_list"));
        assert!(prompt.contains("file_search"));
        assert!(prompt.contains("<tools>"));
        assert!(prompt.contains("Safety rules"));
    }
}
