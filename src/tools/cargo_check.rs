#![allow(dead_code)]

//! Async `cargo check` runner with structured diagnostic output.
//!
//! Runs `cargo check --message-format=json` in the given project root and
//! parses the newline-delimited JSON messages into typed `DiagMessage` structs.
//!
//! **Safety**: `cargo check` is a read-only analysis pass — it never modifies
//! source files.  The command is pre-validated through `crate::safety::classify_command`
//! as a defence-in-depth measure.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;

const TIMEOUT_SECS: u64 = 60;
const MAX_STDERR_BYTES: usize = 32_768; // 32 KB

// ── Public types ───────────────────────────────────────────────────────────────

/// A single compiler diagnostic (error or warning).
#[derive(Debug, Clone)]
pub struct DiagMessage {
    /// `"error"` or `"warning"`.
    pub level: String,
    /// Human-readable message text.
    pub message: String,
    /// Source file (relative path as reported by rustc).
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// rustc error code, e.g. `"E0433"`.
    pub code: Option<String>,
}

impl DiagMessage {
    fn format(&self) -> String {
        match (&self.file, self.line) {
            (Some(f), Some(l)) => format!("  [{}] {}:{} — {}\n", self.level, f, l, self.message),
            (Some(f), None)    => format!("  [{}] {} — {}\n", self.level, f, self.message),
            _                  => format!("  [{}] {}\n", self.level, self.message),
        }
    }
}

/// The complete result of a `cargo check` run.
#[derive(Debug, Clone)]
pub struct CargoCheckResult {
    /// `true` if `cargo check` exited with code 0.
    pub success: bool,
    pub errors: Vec<DiagMessage>,
    pub warnings: Vec<DiagMessage>,
    pub duration_ms: u128,
    /// Raw stderr output (capped at `MAX_STDERR_BYTES`).
    pub raw_stderr: String,
}

impl CargoCheckResult {
    pub fn summary(&self) -> String {
        format!(
            "cargo check {} — {} error(s), {} warning(s) [{} ms]",
            if self.success { "PASS" } else { "FAIL" },
            self.errors.len(),
            self.warnings.len(),
            self.duration_ms,
        )
    }

    /// Format all diagnostics as a human-readable string for display or LLM feedback.
    pub fn format_diagnostics(&self) -> String {
        let mut out = String::new();
        for d in &self.errors   { out.push_str(&d.format()); }
        for d in &self.warnings { out.push_str(&d.format()); }
        out
    }
}

// ── Runner ─────────────────────────────────────────────────────────────────────

/// Run `cargo check` in `project_root` and return structured results.
///
/// Returns `Err` only for infrastructure failures (spawn failure, no Cargo.toml).
/// Compiler errors/warnings come back as `Ok(CargoCheckResult { success: false, … })`.
pub async fn run_cargo_check(project_root: &Path) -> Result<CargoCheckResult> {
    // Defence-in-depth: verify the safety classifier agrees
    if crate::safety::classify_command("cargo check").is_blocked() {
        return Err(anyhow!("[cargo_check] blocked by safety classifier — this is unexpected"));
    }

    // Verify we're in a Rust project
    if !project_root.join("Cargo.toml").exists() {
        return Err(anyhow!(
            "No Cargo.toml found at '{}' — not a Rust project",
            project_root.display()
        ));
    }

    let canonical = project_root
        .canonicalize()
        .map_err(|e| anyhow!("Cannot canonicalize project root: {}", e))?;

    let start = Instant::now();

    let outcome = timeout(
        Duration::from_secs(TIMEOUT_SECS),
        Command::new("cargo")
            .args(["check", "--message-format=json"])
            .current_dir(&canonical)
            .output(),
    )
    .await;

    let elapsed = start.elapsed().as_millis();

    let output = match outcome {
        Ok(Ok(o))  => o,
        Ok(Err(e)) => return Err(anyhow!("Failed to spawn `cargo check`: {}", e)),
        Err(_)     => {
            return Ok(CargoCheckResult {
                success: false,
                errors: vec![DiagMessage {
                    level:   "error".into(),
                    message: format!("cargo check timed out after {} seconds", TIMEOUT_SECS),
                    file: None, line: None, column: None, code: None,
                }],
                warnings:    vec![],
                duration_ms: elapsed,
                raw_stderr:  String::new(),
            });
        }
    };

    let stdout     = String::from_utf8_lossy(&output.stdout).into_owned();
    let raw_stderr = cap_string(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        MAX_STDERR_BYTES,
    );
    let (errors, warnings) = parse_json_messages(&stdout);

    Ok(CargoCheckResult {
        success: output.status.success(),
        errors,
        warnings,
        duration_ms: elapsed,
        raw_stderr,
    })
}

// ── NDJSON parser ──────────────────────────────────────────────────────────────

/// Minimal deserialization structs for `cargo check --message-format=json` output.
#[derive(Deserialize)]
struct CargoMsg {
    reason: String,
    #[serde(default)]
    message: Option<CompilerMessage>,
}

#[derive(Deserialize)]
struct CompilerMessage {
    level: String,
    message: String,
    #[serde(default)]
    code: Option<MsgCode>,
    #[serde(default)]
    spans: Vec<MsgSpan>,
}

#[derive(Deserialize)]
struct MsgCode {
    code: String,
}

#[derive(Deserialize)]
struct MsgSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    is_primary: bool,
}

fn parse_json_messages(stdout: &str) -> (Vec<DiagMessage>, Vec<DiagMessage>) {
    let mut errors   = Vec::new();
    let mut warnings = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        let msg: CargoMsg = match serde_json::from_str(line) {
            Ok(m)  => m,
            Err(_) => continue,
        };

        if msg.reason != "compiler-message" { continue; }

        let cm = match msg.message {
            Some(m) => m,
            None    => continue,
        };

        // Skip meta-messages like "aborting due to N previous errors"
        if cm.message.starts_with("aborting due to") { continue; }

        let primary = cm.spans.iter().find(|s| s.is_primary);
        let (file, line_num, col) = match primary {
            Some(s) => (Some(s.file_name.clone()), Some(s.line_start), Some(s.column_start)),
            None    => (None, None, None),
        };

        let diag = DiagMessage {
            level:   cm.level.clone(),
            message: cm.message,
            file,
            line:    line_num,
            column:  col,
            code:    cm.code.map(|c| c.code),
        };

        match cm.level.as_str() {
            "error"   => errors.push(diag),
            "warning" => warnings.push(diag),
            _         => {} // note / help — omitted for brevity
        }
    }

    (errors, warnings)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn cap_string(s: String, max: usize) -> String {
    if s.len() <= max { return s; }
    let mut end = max;
    while !s.is_char_boundary(end) { end -= 1; }
    format!("{}[…truncated]", &s[..end])
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_is_clean() {
        let (e, w) = parse_json_messages("");
        assert!(e.is_empty());
        assert!(w.is_empty());
    }

    #[test]
    fn parse_non_compiler_message_ignored() {
        let line = r#"{"reason":"build-script-executed","package_id":"foo 0.1.0"}"#;
        let (e, w) = parse_json_messages(line);
        assert!(e.is_empty());
        assert!(w.is_empty());
    }

    #[test]
    fn parse_compiler_error() {
        // Minimal valid compiler-message JSON (stripped of optional fields)
        let line = r#"{"reason":"compiler-message","package_id":"x","manifest_path":"Cargo.toml","target":{"kind":["bin"],"name":"x","src_path":"src/main.rs","edition":"2021","doc":true,"doctest":true,"test":true},"message":{"message":"unused variable","code":{"code":"unused_variables","explanation":null},"level":"warning","spans":[{"file_name":"src/main.rs","byte_start":0,"byte_end":1,"line_start":3,"line_end":3,"column_start":9,"column_end":10,"is_primary":true,"text":[],"label":null,"suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[],"rendered":"warning text"}}"#;
        let (e, w) = parse_json_messages(line);
        assert!(e.is_empty(), "expected no errors");
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(w[0].line, Some(3));
    }

    #[test]
    fn abort_message_skipped() {
        let line = r#"{"reason":"compiler-message","package_id":"x","manifest_path":"","target":{"kind":[],"name":"","src_path":"","edition":"","doc":false,"doctest":false,"test":false},"message":{"message":"aborting due to 2 previous errors","code":null,"level":"error","spans":[],"children":[],"rendered":""}}"#;
        let (e, _) = parse_json_messages(line);
        assert!(e.is_empty(), "abort message should be skipped");
    }
}
