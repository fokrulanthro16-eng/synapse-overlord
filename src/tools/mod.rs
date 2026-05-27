#![allow(dead_code, unused_imports)]

//! Tool registry for the Hermes architect agent.
//!
//! Every tool exposed to the LLM lives here:
//!   - Shared type definitions (`ToolCallRequest`, `ToolDefinition`, `ToolResult`)
//!   - `available_tools()` — read-only tools for analysis
//!   - `available_tools_with_patch()` — above + `propose_patch` for Phase 3
//!   - `dispatch()` — read-only tool dispatcher
//!   - `dispatch_with_patch()` — dispatcher that also handles `propose_patch`

pub mod cargo_check;
pub mod diff;
pub mod file_tools;
pub mod patch_tool;

pub use cargo_check::CargoCheckResult;
pub use file_tools::{FileListArgs, FileReadArgs, FileSearchArgs};
pub use patch_tool::{PatchProposal, PatchStatus, PatchStore};

use std::path::Path;

use serde::{Deserialize, Serialize};

// ── Shared types ──────────────────────────────────────────────────────────────

/// A parsed tool-call request from the LLM.
/// Matches the JSON shape inside `<tool_call>` tags:
/// `{"name": "tool_name", "arguments": {...}}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Schema descriptor for one tool, injected into the Hermes system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the `arguments` object.
    pub parameters: serde_json::Value,
}

/// The result of executing a tool call.
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(name: impl Into<String>, output: impl Into<String>) -> Self {
        Self { tool_name: name.into(), success: true, output: output.into(), error: None }
    }

    pub fn err(name: impl Into<String>, error: impl Into<String>) -> Self {
        let e = error.into();
        Self { tool_name: name.into(), success: false, output: String::new(), error: Some(e) }
    }

    /// Returns the text that should be fed back to the LLM as the tool result.
    pub fn display(&self) -> String {
        if self.success {
            self.output.clone()
        } else {
            format!("ERROR: {}", self.error.as_deref().unwrap_or("unknown error"))
        }
    }
}

// ── Tool registry ─────────────────────────────────────────────────────────────

/// Returns the canonical list of tools available to the Hermes architect.
/// This list is serialised into the system prompt by `llm::hermes::build_system_prompt`.
pub fn available_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "file_read",
            description:
                "Read the contents of a file within the project root. \
                 Returns numbered lines up to `max_lines` (default 200, hard-cap 500). \
                 Use this to inspect source files, configs, or docs. \
                 Path must be relative to the project root (e.g. 'src/main.rs').",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path from project root."
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Lines to return (default 200, max 500).",
                        "default": 200
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "file_list",
            description:
                "List the files and subdirectories at a path within the project root. \
                 Hidden files and build artefacts (target/, node_modules/, .git/) \
                 are excluded automatically. Use '.' for the project root.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative directory path (default '.').",
                        "default": "."
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "file_search",
            description:
                "Search for a text pattern (case-insensitive, plain text) \
                 across all source files in the project. \
                 Returns matching lines with file path and line number. \
                 Useful for finding function definitions, usages, or constants.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Case-insensitive text to search for."
                    },
                    "path": {
                        "type": "string",
                        "description": "Limit search to this subdirectory (default '.').",
                        "default": "."
                    }
                },
                "required": ["pattern"]
            }),
        },
    ]
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Execute a tool call inside the given project `root`.
///
/// All path arguments are validated by `file_tools::safe_resolve` before
/// any I/O takes place — path-traversal attempts are rejected with an error.
///
/// Returns a `ToolResult` (never panics).  The caller feeds
/// `result.display()` back to the LLM as the tool-response content.
pub async fn dispatch(call: &ToolCallRequest, root: &Path) -> ToolResult {
    match call.name.as_str() {
        // ── file_read ──────────────────────────────────────────────────────────
        "file_read" => {
            let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
                Some(p) => p.to_string(),
                None => return ToolResult::err("file_read", "Missing required argument: 'path'"),
            };
            let max_lines = call
                .arguments
                .get("max_lines")
                .and_then(|v| v.as_u64())
                .map(|n| n.min(500) as usize)
                .unwrap_or(200);

            match file_tools::read_file(root, FileReadArgs { path, max_lines }) {
                Ok(output) => ToolResult::ok("file_read", output),
                Err(e)     => ToolResult::err("file_read", e.to_string()),
            }
        }

        // ── file_list ─────────────────────────────────────────────────────────
        "file_list" => {
            let path = call
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();

            match file_tools::list_directory(root, FileListArgs { path }) {
                Ok(output) => ToolResult::ok("file_list", output),
                Err(e)     => ToolResult::err("file_list", e.to_string()),
            }
        }

        // ── file_search ───────────────────────────────────────────────────────
        "file_search" => {
            let pattern = match call.arguments.get("pattern").and_then(|v| v.as_str()) {
                Some(p) => p.to_string(),
                None => return ToolResult::err("file_search", "Missing required argument: 'pattern'"),
            };
            let path = call
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();

            match file_tools::search_files(root, FileSearchArgs { pattern, path }) {
                Ok(output) => ToolResult::ok("file_search", output),
                Err(e)     => ToolResult::err("file_search", e.to_string()),
            }
        }

        // ── unknown ───────────────────────────────────────────────────────────
        unknown => ToolResult::err(
            unknown,
            format!(
                "Unknown tool '{}'. Available: file_read, file_list, file_search",
                unknown
            ),
        ),
    }
}

// ── Phase 3: patch-capable registry ──────────────────────────────────────────

/// Returns the full tool list for the patch-proposal mode of the architect.
/// Includes the three read-only tools plus `propose_patch`.
pub fn available_tools_with_patch() -> Vec<ToolDefinition> {
    let mut tools = available_tools();
    tools.push(ToolDefinition {
        name: "propose_patch",
        description:
            "Propose a file modification for human review and approval. \
             Only call this when you have a concrete, well-reasoned patch ready. \
             Provide COMPLETE new file content — not a partial snippet. \
             The human must approve before anything is written to disk. \
             Path must be relative to the project root (e.g. 'src/main.rs').",
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path from project root."
                },
                "description": {
                    "type": "string",
                    "description": "Brief explanation of what this change does and why."
                },
                "new_content": {
                    "type": "string",
                    "description": "Complete new content for the file (not a diff or partial snippet)."
                }
            },
            "required": ["path", "description", "new_content"]
        }),
    });
    tools
}

// ── Proposal input limits ─────────────────────────────────────────────────────

/// Maximum byte length of `new_content` in a propose_patch call.
/// Guards against memory exhaustion from runaway LLM output.
const MAX_CONTENT_BYTES: usize = 256 * 1024; // 256 KB

/// Maximum byte length of the `description` field.
const MAX_DESCRIPTION_BYTES: usize = 2_048;

/// Maximum byte length of the target `path` argument.
const MAX_PATH_BYTES: usize = 512;

/// Like `dispatch()` but also handles the `propose_patch` tool.
///
/// SAFETY: `propose_patch` validates the path via `safe_resolve_new` before
/// storing anything.  No bytes are written to disk here — that only happens
/// in `patch_store.approve()`, which is gated by a separate HTTP call.
pub async fn dispatch_with_patch(
    call:        &ToolCallRequest,
    root:        &Path,
    patch_store: &PatchStore,
) -> ToolResult {
    match call.name.as_str() {
        // ── Delegate all read-only tools to the existing dispatcher ───────────
        "file_read" | "file_list" | "file_search" => dispatch(call, root).await,

        // ── propose_patch ─────────────────────────────────────────────────────
        "propose_patch" => {
            // ── Extract and validate arguments ────────────────────────────────

            let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
                Some(p) => p.trim().to_string(),
                None => return ToolResult::err("propose_patch", "Missing required argument: 'path'"),
            };
            if path.is_empty() {
                return ToolResult::err("propose_patch", "'path' must not be empty");
            }
            if path.len() > MAX_PATH_BYTES {
                return ToolResult::err(
                    "propose_patch",
                    format!("'path' exceeds maximum length ({} bytes)", MAX_PATH_BYTES),
                );
            }

            let description = match call.arguments.get("description").and_then(|v| v.as_str()) {
                Some(d) => d.trim().to_string(),
                None => return ToolResult::err("propose_patch", "Missing required argument: 'description'"),
            };
            if description.is_empty() {
                return ToolResult::err("propose_patch", "'description' must not be empty");
            }
            if description.len() > MAX_DESCRIPTION_BYTES {
                return ToolResult::err(
                    "propose_patch",
                    format!(
                        "'description' exceeds {} byte limit (got {} bytes)",
                        MAX_DESCRIPTION_BYTES,
                        description.len()
                    ),
                );
            }

            let new_content = match call.arguments.get("new_content").and_then(|v| v.as_str()) {
                Some(c) => c.to_string(),
                None => return ToolResult::err("propose_patch", "Missing required argument: 'new_content'"),
            };
            if new_content.len() > MAX_CONTENT_BYTES {
                return ToolResult::err(
                    "propose_patch",
                    format!(
                        "'new_content' exceeds {} byte limit (got {} bytes). \
                         Split the change into smaller, focused patches.",
                        MAX_CONTENT_BYTES,
                        new_content.len()
                    ),
                );
            }

            // Read original content (optional — needed for diff preview only)
            let original_content = file_tools::safe_resolve(root, &path)
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default();

            // Generate diff preview (no I/O — pure string operation)
            let diff_text = diff::generate_unified_diff(
                &original_content,
                &new_content,
                &format!("a/{}", path),
                &format!("b/{}", path),
            );

            // Store proposal — path validated via safe_resolve_new inside propose()
            match patch_store.propose(root, &path, &description, &new_content) {
                Err(e) => ToolResult::err("propose_patch", format!("Failed to store proposal: {}", e)),
                Ok(patch_id) => {
                    let preview: String = diff_text
                        .lines()
                        .take(30)
                        .collect::<Vec<_>>()
                        .join("\n");

                    ToolResult::ok(
                        "propose_patch",
                        format!(
                            "PATCH_ID:{id}\npath: {path}\ndescription: {desc}\n\nDiff preview:\n{preview}",
                            id      = patch_id,
                            path    = path,
                            desc    = description,
                            preview = preview,
                        ),
                    )
                }
            }
        }

        // ── unknown ───────────────────────────────────────────────────────────
        unknown => ToolResult::err(
            unknown,
            format!(
                "Unknown tool '{}'. Available: file_read, file_list, file_search, propose_patch",
                unknown
            ),
        ),
    }
}
