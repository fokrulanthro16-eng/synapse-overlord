#![allow(dead_code)]

//! POST /api/architect/analyze — Read-only Hermes architect session.
//!
//! The handler:
//!   1. Maps the project via `crate::rag::map_project`.
//!   2. Builds a Hermes tool-use conversation (system prompt with tool schemas).
//!   3. Runs an LLM ↔ tool loop (max `max_tool_calls` iterations).
//!   4. Returns the final synthesis text + a log of tool calls made.
//!
//! SAFETY CONTRACT
//! ───────────────
//! • No writes to disk.
//! • No patch proposals.
//! • Available tools: file_read, file_list, file_search (all read-only).
//! • All path arguments validated by `safe_resolve` before any I/O.
//! • Tool call count hard-capped at MAX_TOOL_CALLS.
//! • The handler has no Axum `State<T>` extractor and is compatible
//!   with any `Router<S>` — WebState is never touched.

use std::path::{Path, PathBuf};
use std::time::Instant;

use axum::Json;
use serde::{Deserialize, Serialize};

use crate::llm::{
    hermes as llm_hermes, // alias — avoids confusion with this module's own name
    route_request,
    LlmBackend,
    LlmMessage,
    LlmRequest,
    LlmRole,
};
use crate::rag;
use crate::tools;

use tracing::{error, info, warn};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Absolute maximum iterations regardless of what the caller requests.
const MAX_TOOL_CALLS: usize = 10;

/// Characters of tool output kept in each `ToolCallRecord::result_preview`.
const PREVIEW_LEN: usize = 200;

// ── Request validation limits ─────────────────────────────────────────────────

/// Maximum character length of the `goal` field in any architect request.
const MAX_GOAL_LEN: usize = 8_192;

/// Maximum byte length of a patch_id path parameter.
const MAX_PATCH_ID_LEN: usize = 64;

// ── Wire types ────────────────────────────────────────────────────────────────

/// Request body for POST /api/architect/analyze.
#[derive(Deserialize)]
pub struct AnalyzeRequest {
    /// The goal or question for the architect agent.
    /// Example: "What is the purpose of src/agent/mod.rs?"
    pub goal: String,

    /// Maximum LLM ↔ tool iterations (default 5, hard-capped at 10).
    #[serde(default = "default_max_calls")]
    pub max_tool_calls: usize,
}

fn default_max_calls() -> usize { 5 }

/// Summary of a single tool call, included in the response for transparency.
#[derive(Serialize)]
pub struct ToolCallRecord {
    /// Tool name, e.g. "file_read".
    pub name: String,
    /// Whether the tool returned successfully.
    pub success: bool,
    /// First PREVIEW_LEN chars of the result (for dashboard display).
    pub result_preview: String,
}

/// Response body for POST /api/architect/analyze.
#[derive(Serialize)]
pub struct AnalyzeResponse {
    /// "success" | "partial" | "error"
    pub status: String,

    /// True when the agent produced a final synthesis text without errors.
    pub success: bool,

    /// Short summary — first non-bullet paragraph (≤ 400 chars).
    pub summary: String,

    /// The Hermes model's full analysis / synthesis text.
    pub analysis: String,

    /// Bullet-point suggestions extracted from the analysis (up to 8).
    pub suggestions: Vec<String>,

    /// Ordered log of tool calls the model made.
    pub tool_calls: Vec<ToolCallRecord>,

    /// Number of project files discovered (alias: files_mapped).
    pub files_analyzed: usize,

    /// Same as `files_analyzed` — preserved for backward compatibility.
    pub files_mapped: usize,

    /// Model name as reported by the API (e.g. "hermes3", "llama-3.3-70b-versatile").
    pub model: String,

    /// Backend label: "groq" or "ollama".
    pub backend: String,

    /// Wall-clock milliseconds for the entire request (alias: duration_ms).
    pub timing_ms: u128,

    /// Same as `timing_ms` — preserved for backward compatibility.
    pub duration_ms: u128,

    /// Present only on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// POST /api/architect/analyze
///
/// Stateless Axum handler — no `State<T>` extractor.
/// Compatible with any `Router<S>`; registered directly in `web/mod.rs`.
pub async fn analyze_handler(Json(req): Json<AnalyzeRequest>) -> Json<AnalyzeResponse> {
    let start = Instant::now();

    // ── Validate inputs ───────────────────────────────────────────────────────
    let goal = req.goal.trim().to_string();
    if goal.is_empty() {
        return Json(err_response("'goal' must not be empty", start.elapsed().as_millis()));
    }
    if goal.len() > MAX_GOAL_LEN {
        return Json(err_response(
            &format!("'goal' exceeds {} character limit", MAX_GOAL_LEN),
            start.elapsed().as_millis(),
        ));
    }

    // Clamp the caller's requested iteration count
    let max_calls = req.max_tool_calls.min(MAX_TOOL_CALLS).max(1);

    // ── 1. Resolve project root ───────────────────────────────────────────────
    let root = resolve_root();

    // ── 2. Map project ────────────────────────────────────────────────────────
    let map = match rag::map_project(&root) {
        Ok(m) => m,
        Err(e) => {
            return Json(err_response(
                &format!("Project mapping failed: {}", e),
                start.elapsed().as_millis(),
            ));
        }
    };

    let files_mapped = map.files.len();

    // ── 3. Build initial Hermes conversation ──────────────────────────────────
    let tool_defs  = tools::available_tools();
    let summary    = project_summary(&map);
    let messages   = llm_hermes::build_conversation(&goal, &summary, &tool_defs);
    let backend    = LlmBackend::from_env();

    // ── 4. Run the agentic loop ───────────────────────────────────────────────
    let AgentOutput { analysis, tool_calls, model, error } =
        run_agent_loop(messages, &root, &backend, max_calls).await;

    let dur         = start.elapsed().as_millis();
    let summary     = extract_summary(&analysis);
    let suggestions = extract_suggestions(&analysis);
    let status      = if error.is_none() && !analysis.is_empty() {
        "success"
    } else if !analysis.is_empty() {
        "partial"
    } else {
        "error"
    };

    Json(AnalyzeResponse {
        status:         status.to_string(),
        success:        error.is_none() && !analysis.is_empty(),
        summary,
        analysis,
        suggestions,
        tool_calls,
        files_analyzed: files_mapped,
        files_mapped,
        model,
        backend:        backend.label().to_string(),
        timing_ms:      dur,
        duration_ms:    dur,
        error,
    })
}

// ── Agentic loop ──────────────────────────────────────────────────────────────

struct AgentOutput {
    analysis:   String,
    tool_calls: Vec<ToolCallRecord>,
    model:      String,
    error:      Option<String>,
}

/// Core LLM ↔ tool loop.
///
/// Iteration structure:
///   call LLM
///   → if response contains no <tool_call> blocks: done, return text
///   → otherwise: dispatch each tool, push results into conversation, repeat
///
/// Exits when:
///   - The model produces a response with no tool calls (natural finish).
///   - `max_calls` iterations are exhausted (best-effort fallback used).
///   - An LLM network error occurs.
async fn run_agent_loop(
    mut messages: Vec<LlmMessage>,
    root: &Path,
    backend: &LlmBackend,
    max_calls: usize,
) -> AgentOutput {
    let mut records: Vec<ToolCallRecord> = Vec::new();
    let mut model_name  = String::new();
    let mut final_text  = String::new();
    let mut loop_error: Option<String> = None;

    'agent: for iter in 0..max_calls {
        // ── Call LLM ─────────────────────────────────────────────────────────
        let llm_req = LlmRequest::new(messages.clone())
            .with_max_tokens(1024)
            .with_temperature(0.3);

        let resp = match route_request(backend, llm_req).await {
            Ok(r)  => r,
            Err(e) => {
                loop_error = Some(format!("LLM error (iter {}): {}", iter + 1, e));
                break 'agent;
            }
        };

        if model_name.is_empty() {
            model_name = resp.model.clone();
        }

        let assistant_text = resp.content;

        // ── Parse tool calls ──────────────────────────────────────────────────
        let calls = llm_hermes::parse_tool_calls(&assistant_text);

        if calls.is_empty() {
            // Model produced a final synthesis answer — we're done
            final_text = assistant_text;
            break 'agent;
        }

        // Push the assistant turn (which contains the <tool_call> tags)
        messages.push(LlmMessage::assistant(assistant_text));

        // ── Dispatch tools ────────────────────────────────────────────────────
        for call in &calls {
            let result = tools::dispatch(call, root).await;

            // Truncated preview for the dashboard
            let preview: String = result.display().chars().take(PREVIEW_LEN).collect();
            records.push(ToolCallRecord {
                name:           call.name.clone(),
                success:        result.success,
                result_preview: preview,
            });

            // Push the tool-response turn into conversation history
            messages.push(llm_hermes::build_tool_result_message(
                &call.name,
                &result.display(),
            ));
        }
    }

    // ── Fallback: if the loop ran out of budget without a clean finish ─────
    if final_text.is_empty() && loop_error.is_none() {
        // Recover the last assistant message as a best-effort answer
        for msg in messages.iter().rev() {
            if msg.role == LlmRole::Assistant {
                final_text = msg.content.clone();
                break;
            }
        }
        if final_text.is_empty() {
            loop_error = Some(
                "Agent exhausted tool-call budget without producing a final response. \
                 Try increasing max_tool_calls or simplifying the goal."
                .into(),
            );
        }
    }

    AgentOutput {
        analysis:   final_text,
        tool_calls: records,
        model:      model_name,
        error:      loop_error,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Determine the project root.
///
/// Priority:
///   1. `active_project` from saved settings (if it's a valid directory).
///   2. Current directory (`.`).
fn resolve_root() -> PathBuf {
    let settings = crate::settings::load(Path::new(".")).unwrap_or_default();
    if !settings.active_project.is_empty()
        && Path::new(&settings.active_project).is_dir()
    {
        PathBuf::from(settings.active_project)
    } else {
        PathBuf::from(".")
    }
}

/// Format a compact project snapshot for the Hermes system prompt.
/// Shows the top 20 most-relevant files (sorted by `rag::role_priority`).
fn project_summary(map: &rag::ProjectMap) -> String {
    let top: String = map
        .files
        .iter()
        .take(20)
        .map(|f| {
            format!(
                "  {path}  [{role}]  {size} bytes\n",
                path = f.relative_path,
                role = f.role.label(),
                size = f.size_bytes,
            )
        })
        .collect();

    format!(
        "Root: {root}\nFiles: {total} ({skipped} skipped)\n\nTop files (by relevance):\n{top}",
        root    = map.root.display(),
        total   = map.files.len(),
        skipped = map.skipped_count,
        top     = top,
    )
}

/// Extract a short summary: the first two non-empty, non-bullet lines joined.
fn extract_summary(text: &str) -> String {
    text.lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with('-')
                && !t.starts_with('•')
                && !t.starts_with('*')
                && !t.starts_with('#')
        })
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(400)
        .collect()
}

/// Extract bullet-point / numbered suggestions from the analysis text (up to 8).
fn extract_suggestions(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim();
            // Bullet-point list
            if t.starts_with("- ") || t.starts_with("• ") || t.starts_with("* ") {
                let body = t
                    .trim_start_matches(|c: char| c == '-' || c == '•' || c == '*')
                    .trim();
                if body.len() > 10 { return Some(body.to_string()); }
            }
            // Numbered list: "1. " "2. " etc.
            let mut chars = t.chars();
            if chars.next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                if let Some(body) = t.splitn(2, ". ").nth(1) {
                    let body = body.trim();
                    if body.len() > 10 { return Some(body.to_string()); }
                }
            }
            None
        })
        .take(8)
        .collect()
}

/// Construct a failed AnalyzeResponse without going through the full pipeline.
fn err_response(msg: &str, duration_ms: u128) -> AnalyzeResponse {
    AnalyzeResponse {
        status:         "error".to_string(),
        success:        false,
        summary:        String::new(),
        analysis:       String::new(),
        suggestions:    vec![],
        tool_calls:     vec![],
        files_analyzed: 0,
        files_mapped:   0,
        model:          String::new(),
        backend:        String::new(),
        timing_ms:      duration_ms,
        duration_ms,
        error:          Some(msg.to_string()),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Phase 3 — Safe patch proposal workflow
//
// SAFETY CONTRACT
// ───────────────
// • propose_handler: reads files + stores proposals in memory only. No writes.
// • apply_handler:   backup → approve (write) → cargo check. One file at a time.
// • All path arguments re-validated by safe_resolve_new at every step.
// • No shell execution except `cargo check` (run by tools::cargo_check).
// • No file deletions. Backups are written to .synapse_backups/ (never removed).
// ══════════════════════════════════════════════════════════════════════════════

// ── Wire types ────────────────────────────────────────────────────────────────

/// Request body for POST /api/architect/propose.
#[derive(Deserialize)]
pub struct ProposeRequest {
    /// Instruction describing what change the architect should propose.
    pub goal: String,

    /// Maximum LLM ↔ tool iterations (default 5, hard-capped at 10).
    #[serde(default = "default_max_calls")]
    pub max_tool_calls: usize,
}

/// Summary of one proposed patch, returned for human review.
#[derive(Serialize)]
pub struct PatchInfo {
    pub patch_id:      String,
    pub relative_path: String,
    pub description:   String,
    /// First 30 lines of the unified diff preview (read-only string).
    pub diff_preview:  String,
}

/// Response body for POST /api/architect/propose.
#[derive(Serialize)]
pub struct ProposeResponse {
    pub success:      bool,
    pub analysis:     String,
    pub patches:      Vec<PatchInfo>,
    pub tool_calls:   Vec<ToolCallRecord>,
    pub files_mapped: usize,
    pub model:        String,
    pub backend:      String,
    pub duration_ms:  u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Cargo-check result included in the apply response.
#[derive(Serialize)]
pub struct CargoSummary {
    pub success:       bool,
    pub error_count:   usize,
    pub warning_count: usize,
    pub summary:       String,
    pub diagnostics:   String,
}

/// Response body for POST /api/architect/apply/:patch_id.
#[derive(Serialize)]
pub struct ApplyResponse {
    pub success:     bool,
    pub patch_id:    String,
    pub file_path:   String,
    pub backup_path: String,
    pub cargo:       CargoSummary,
    pub duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Propose handler ───────────────────────────────────────────────────────────

/// POST /api/architect/propose
///
/// Runs a Hermes agent loop with the `propose_patch` tool enabled.
/// The agent may read files and then propose one or more patches.
/// Nothing is written to disk — the caller must approve each patch via
/// `/api/architect/apply/:patch_id`.
pub async fn propose_handler(
    axum::Extension(patch_store): axum::Extension<crate::tools::PatchStore>,
    Json(req): Json<ProposeRequest>,
) -> Json<ProposeResponse> {
    let start = Instant::now();

    // ── Validate inputs ───────────────────────────────────────────────────────
    let goal = req.goal.trim().to_string();
    if goal.is_empty() {
        return Json(err_propose_response("'goal' must not be empty", start.elapsed().as_millis()));
    }
    if goal.len() > MAX_GOAL_LEN {
        return Json(err_propose_response(
            &format!("'goal' exceeds {} character limit", MAX_GOAL_LEN),
            start.elapsed().as_millis(),
        ));
    }

    let max_calls = req.max_tool_calls.min(MAX_TOOL_CALLS).max(1);
    let root      = resolve_root();

    // Map project
    let map = match rag::map_project(&root) {
        Ok(m)  => m,
        Err(e) => return Json(err_propose_response(
            &format!("Project mapping failed: {}", e),
            start.elapsed().as_millis(),
        )),
    };
    let files_mapped = map.files.len();

    // Build conversation with patch-capable tool schemas
    let tool_defs = tools::available_tools_with_patch();
    let summary   = project_summary(&map);
    let messages  = llm_hermes::build_conversation(&goal, &summary, &tool_defs);
    let backend   = LlmBackend::from_env();

    info!(
        goal        = %goal,
        max_calls   = max_calls,
        files       = files_mapped,
        backend     = %backend.label(),
        "propose: starting agent loop"
    );

    // Run agent loop with patch dispatch
    let ProposalOutput { analysis, patches, tool_calls, model, error } =
        run_proposal_loop(messages, &root, &backend, max_calls, &patch_store).await;

    if let Some(ref e) = error {
        error!(err = %e, "propose: agent loop error");
    } else {
        info!(
            patches     = patches.len(),
            tool_calls  = tool_calls.len(),
            model       = %model,
            "propose: agent loop complete"
        );
    }

    Json(ProposeResponse {
        success: error.is_none() && !analysis.is_empty(),
        analysis,
        patches,
        tool_calls,
        files_mapped,
        model,
        backend: backend.label().to_string(),
        duration_ms: start.elapsed().as_millis(),
        error,
    })
}

// ── Apply handler ─────────────────────────────────────────────────────────────

/// POST /api/architect/apply/:patch_id
///
/// Applies a previously proposed patch after human approval.
///
/// Steps:
///   1. Retrieve the patch from the in-memory store (errors if expired/missing).
///   2. Back up the original file (if it exists) to `.synapse_backups/`.
///   3. Call `patch_store.approve()` which re-validates the path and writes the file.
///   4. Run `cargo check` and return its output.
pub async fn apply_handler(
    axum::Extension(patch_store): axum::Extension<crate::tools::PatchStore>,
    axum::extract::Path(patch_id): axum::extract::Path<String>,
) -> Json<ApplyResponse> {
    let start = Instant::now();

    // ── Validate patch_id ─────────────────────────────────────────────────────
    if patch_id.is_empty() || patch_id.len() > MAX_PATCH_ID_LEN {
        return Json(err_apply_response(
            &patch_id,
            "Invalid patch_id — must be non-empty and ≤64 characters",
            start.elapsed().as_millis(),
        ));
    }
    if !patch_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Json(err_apply_response(
            &patch_id,
            "Invalid patch_id — must contain only alphanumeric characters and underscores",
            start.elapsed().as_millis(),
        ));
    }

    let root = resolve_root();
    info!(patch_id = %patch_id, "apply: looking up patch");

    // ── 1. Retrieve patch ─────────────────────────────────────────────────────
    let patch = match patch_store.get(&patch_id) {
        Some(p) => p,
        None    => {
            warn!(patch_id = %patch_id, "apply: patch not found or expired");
            return Json(err_apply_response(
                &patch_id,
                &format!("Patch '{}' not found or has expired.", patch_id),
                start.elapsed().as_millis(),
            ));
        }
    };

    let relative_path = patch.relative_path.clone();
    let abs_path      = root.join(&relative_path);

    info!(patch_id = %patch_id, file = %relative_path, "apply: patch found");

    // ── 2. Back up original file ──────────────────────────────────────────────
    let backup_path = if abs_path.exists() {
        match write_backup(&abs_path, &root) {
            Ok(p)  => {
                let s = p.to_string_lossy().into_owned();
                info!(patch_id = %patch_id, backup = %s, "apply: backup written");
                s
            }
            Err(e) => {
                error!(patch_id = %patch_id, err = %e, "apply: backup failed");
                return Json(err_apply_response(
                    &patch_id,
                    &format!("Backup failed: {}", e),
                    start.elapsed().as_millis(),
                ));
            }
        }
    } else {
        info!(patch_id = %patch_id, file = %relative_path, "apply: new file — no backup needed");
        "(new file — no backup needed)".to_string()
    };

    // ── 3. Approve — writes file to disk ─────────────────────────────────────
    if let Err(e) = patch_store.approve(&patch_id, &root) {
        error!(patch_id = %patch_id, err = %e, "apply: approve/write failed");
        return Json(err_apply_response(
            &patch_id,
            &format!("Apply failed: {}", e),
            start.elapsed().as_millis(),
        ));
    }
    info!(patch_id = %patch_id, file = %relative_path, "apply: file written to disk");

    // ── 4. Run cargo check ────────────────────────────────────────────────────
    info!(patch_id = %patch_id, "apply: running cargo check");
    let cargo = match tools::cargo_check::run_cargo_check(&root).await {
        Ok(result) => {
            if result.success {
                info!(
                    patch_id = %patch_id,
                    warnings = result.warnings.len(),
                    "apply: cargo check passed"
                );
            } else {
                warn!(
                    patch_id  = %patch_id,
                    errors    = result.errors.len(),
                    warnings  = result.warnings.len(),
                    "apply: cargo check found errors"
                );
            }
            CargoSummary {
                success:       result.success,
                error_count:   result.errors.len(),
                warning_count: result.warnings.len(),
                summary:       result.summary(),
                diagnostics:   result.format_diagnostics(),
            }
        }
        Err(e) => {
            error!(patch_id = %patch_id, err = %e, "apply: cargo check failed to run");
            CargoSummary {
                success:       false,
                error_count:   1,
                warning_count: 0,
                summary:       format!("cargo check failed to execute: {}", e),
                diagnostics:   String::new(),
            }
        }
    };

    Json(ApplyResponse {
        success: cargo.success,
        patch_id,
        file_path: relative_path,
        backup_path,
        cargo,
        duration_ms: start.elapsed().as_millis(),
        error: None,
    })
}

// ── Proposal agentic loop ─────────────────────────────────────────────────────

struct ProposalOutput {
    analysis:   String,
    patches:    Vec<PatchInfo>,
    tool_calls: Vec<ToolCallRecord>,
    model:      String,
    error:      Option<String>,
}

/// Core LLM ↔ tool loop for patch-proposal mode.
///
/// Identical in structure to `run_agent_loop` but calls `dispatch_with_patch`,
/// collects `PatchInfo` records from successful `propose_patch` results, and
/// uses a higher `max_tokens` budget (patches can be large).
async fn run_proposal_loop(
    mut messages: Vec<LlmMessage>,
    root:         &Path,
    backend:      &LlmBackend,
    max_calls:    usize,
    patch_store:  &crate::tools::PatchStore,
) -> ProposalOutput {
    let mut records:    Vec<ToolCallRecord> = Vec::new();
    let mut patches:    Vec<PatchInfo>      = Vec::new();
    let mut model_name  = String::new();
    let mut final_text  = String::new();
    let mut loop_error: Option<String>      = None;

    'agent: for iter in 0..max_calls {
        let llm_req = LlmRequest::new(messages.clone())
            .with_max_tokens(2048) // higher budget — patches can be large
            .with_temperature(0.2);

        let resp = match route_request(backend, llm_req).await {
            Ok(r)  => r,
            Err(e) => {
                loop_error = Some(format!("LLM error (iter {}): {}", iter + 1, e));
                break 'agent;
            }
        };

        if model_name.is_empty() {
            model_name = resp.model.clone();
        }

        let assistant_text = resp.content;
        let calls = llm_hermes::parse_tool_calls(&assistant_text);

        if calls.is_empty() {
            final_text = assistant_text;
            break 'agent;
        }

        messages.push(LlmMessage::assistant(assistant_text));

        for call in &calls {
            let result = tools::dispatch_with_patch(call, root, patch_store).await;

            let preview: String = result.display().chars().take(PREVIEW_LEN).collect();
            records.push(ToolCallRecord {
                name:           call.name.clone(),
                success:        result.success,
                result_preview: preview,
            });

            // Collect patch metadata when propose_patch succeeds
            if call.name == "propose_patch" && result.success {
                if let Some(info) = parse_patch_info(&result.display()) {
                    patches.push(info);
                }
            }

            messages.push(llm_hermes::build_tool_result_message(
                &call.name,
                &result.display(),
            ));
        }
    }

    // Fallback: recover the last assistant message as best-effort output
    if final_text.is_empty() && loop_error.is_none() {
        for msg in messages.iter().rev() {
            if msg.role == LlmRole::Assistant {
                final_text = msg.content.clone();
                break;
            }
        }
        if final_text.is_empty() {
            loop_error = Some(
                "Agent exhausted tool-call budget without producing a final response. \
                 Try increasing max_tool_calls or simplifying the goal."
                    .into(),
            );
        }
    }

    ProposalOutput {
        analysis: final_text,
        patches,
        tool_calls: records,
        model: model_name,
        error: loop_error,
    }
}

// ── Phase 3 helpers ───────────────────────────────────────────────────────────

/// Parse a `PatchInfo` from the structured text produced by `dispatch_with_patch`.
///
/// Expected format:
/// ```text
/// PATCH_ID:{id}
/// path: {relative_path}
/// description: {description}
///
/// Diff preview:
/// --- a/...
/// ...
/// ```
fn parse_patch_info(result: &str) -> Option<PatchInfo> {
    let mut patch_id      = String::new();
    let mut relative_path = String::new();
    let mut description   = String::new();
    let mut diff_lines: Vec<&str> = Vec::new();
    let mut in_diff = false;

    for line in result.lines() {
        if line.starts_with("PATCH_ID:") {
            patch_id = line.trim_start_matches("PATCH_ID:").trim().to_string();
        } else if line.starts_with("path: ") {
            relative_path = line.trim_start_matches("path: ").trim().to_string();
        } else if line.starts_with("description: ") {
            description = line.trim_start_matches("description: ").trim().to_string();
        } else if line == "Diff preview:" {
            in_diff = true;
        } else if in_diff {
            diff_lines.push(line);
        }
    }

    if patch_id.is_empty() {
        return None;
    }

    Some(PatchInfo {
        patch_id,
        relative_path,
        description,
        diff_preview: diff_lines.join("\n"),
    })
}

/// Create a timestamped backup of `file_path` inside `{root}/.synapse_backups/`.
///
/// Returns the absolute path to the backup file.
/// Backup filename format: `{flat_relative_path}.{unix_timestamp}.bak`
fn write_backup(
    file_path: &std::path::Path,
    root:      &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let rel = file_path.strip_prefix(root).map_err(|e| e.to_string())?;
    let backup_dir = root.join(".synapse_backups");
    std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Flatten e.g. "src/main.rs" → "src_main.rs"
    let flat = rel.to_string_lossy().replace(['/', '\\', ':'], "_");
    let backup_path = backup_dir.join(format!("{}.{}.bak", flat, ts));

    let content = std::fs::read(file_path).map_err(|e| e.to_string())?;
    std::fs::write(&backup_path, content).map_err(|e| e.to_string())?;

    Ok(backup_path)
}

fn err_propose_response(msg: &str, duration_ms: u128) -> ProposeResponse {
    ProposeResponse {
        success:      false,
        analysis:     String::new(),
        patches:      vec![],
        tool_calls:   vec![],
        files_mapped: 0,
        model:        String::new(),
        backend:      String::new(),
        duration_ms,
        error:        Some(msg.to_string()),
    }
}

fn err_apply_response(patch_id: &str, msg: &str, duration_ms: u128) -> ApplyResponse {
    ApplyResponse {
        success:     false,
        patch_id:    patch_id.to_string(),
        file_path:   String::new(),
        backup_path: String::new(),
        cargo: CargoSummary {
            success:       false,
            error_count:   0,
            warning_count: 0,
            summary:       String::new(),
            diagnostics:   String::new(),
        },
        duration_ms,
        error: Some(msg.to_string()),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Phase 3 continued — Reject + List endpoints
// ══════════════════════════════════════════════════════════════════════════════

/// Re-use the canonical TTL from patch_tool so both sides stay in sync.
use crate::tools::patch_tool::PATCH_TTL_SECS;

// ── Wire types ────────────────────────────────────────────────────────────────

/// Response body for POST /api/architect/reject/:patch_id.
#[derive(Serialize)]
pub struct RejectResponse {
    pub success:  bool,
    pub patch_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One pending patch as seen in the list endpoint.
#[derive(Serialize)]
pub struct PatchSummary {
    pub patch_id:        String,
    pub relative_path:   String,
    pub description:     String,
    /// Unix timestamp (seconds) when this patch was proposed.
    pub proposed_at:     u64,
    /// Seconds remaining before this patch expires (may be negative if rounding).
    pub expires_in_secs: i64,
}

/// Response body for GET /api/architect/patches.
#[derive(Serialize)]
pub struct ListPatchesResponse {
    pub count:   usize,
    pub patches: Vec<PatchSummary>,
}

// ── Reject handler ────────────────────────────────────────────────────────────

/// POST /api/architect/reject/:patch_id
///
/// Marks a pending patch as Rejected.  No disk I/O is performed.
/// The patch ID remains in the store (status = "rejected") for audit purposes
/// but can no longer be approved.
pub async fn reject_handler(
    axum::Extension(patch_store): axum::Extension<crate::tools::PatchStore>,
    axum::extract::Path(patch_id): axum::extract::Path<String>,
) -> Json<RejectResponse> {
    // ── Validate patch_id ─────────────────────────────────────────────────────
    if patch_id.is_empty() || patch_id.len() > MAX_PATCH_ID_LEN {
        return Json(RejectResponse {
            success:  false,
            patch_id,
            error:    Some("Invalid patch_id — must be non-empty and ≤64 characters".into()),
        });
    }
    if !patch_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Json(RejectResponse {
            success:  false,
            patch_id,
            error:    Some("Invalid patch_id — must contain only alphanumeric characters and underscores".into()),
        });
    }

    match patch_store.reject(&patch_id) {
        Ok(()) => {
            info!(patch_id = %patch_id, "reject: patch rejected by user");
            Json(RejectResponse {
                success:  true,
                patch_id,
                error:    None,
            })
        }
        Err(e) => {
            warn!(patch_id = %patch_id, err = %e, "reject: failed");
            Json(RejectResponse {
                success:  false,
                patch_id,
                error:    Some(e.to_string()),
            })
        }
    }
}

// ── List patches handler ──────────────────────────────────────────────────────

/// GET /api/architect/patches
///
/// Returns all currently pending (non-expired) patch proposals.
/// - Lazily marks expired patches during `list_pending()`.
/// - Runs `gc()` to evict approved/rejected/expired entries and bound memory.
pub async fn list_patches_handler(
    axum::Extension(patch_store): axum::Extension<crate::tools::PatchStore>,
) -> Json<ListPatchesResponse> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // list_pending lazily marks pending-but-expired entries as Expired.
    let pending = patch_store.list_pending();

    // gc() then removes all non-Pending entries (Approved, Rejected, Expired)
    // from the store, keeping memory bounded.
    let removed = patch_store.gc();
    if removed > 0 {
        info!(removed = removed, "list_patches: gc removed stale entries");
    }

    let patches: Vec<PatchSummary> = pending
        .into_iter()
        .map(|p| {
            let elapsed    = now.saturating_sub(p.proposed_at);
            let expires_in = (PATCH_TTL_SECS as i64) - (elapsed as i64);
            PatchSummary {
                patch_id:        p.id,
                relative_path:   p.relative_path,
                description:     p.description,
                proposed_at:     p.proposed_at,
                expires_in_secs: expires_in,
            }
        })
        .collect();

    let count = patches.len();
    info!(pending = count, "list_patches: returned pending patches");

    Json(ListPatchesResponse { count, patches })
}
