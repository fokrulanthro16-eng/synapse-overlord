#![allow(dead_code)]

use std::path::Path;

use anyhow::{anyhow, Result};
use tokio::process::Command;

// ── Repo detection ────────────────────────────────────────────────────────────

/// Returns true if `root` contains a `.git` directory.
pub fn is_git_repo(root: &Path) -> bool {
    root.join(".git").is_dir()
}

// ── Read-only git ops ─────────────────────────────────────────────────────────

/// Runs `git status --porcelain` and returns the output.
/// Returns "No changes." when the working tree is clean.
pub async fn git_status(root: &Path) -> Result<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain"])
        .output()
        .await?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("git status failed: {}", err.trim()));
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Ok("No changes.".to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

// ── Staging (no commit) ───────────────────────────────────────────────────────

/// Stages all changes with `git add --all`.
/// Never calls `git commit` — the caller decides whether to commit.
pub async fn stage_all(root: &Path) -> Result<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["add", "--all"])
        .output()
        .await?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("git add --all failed: {}", err.trim()));
    }

    // git add --all is silent on success; follow up with a short status
    let status = Command::new("git")
        .current_dir(root)
        .args(["status", "--short"])
        .output()
        .await?;

    let text = String::from_utf8_lossy(&status.stdout);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Ok("Nothing to stage.".to_string())
    } else {
        Ok(format!("Staged:\n{}", trimmed))
    }
}

// ── Commit message generation ─────────────────────────────────────────────────

/// Generates a Conventional Commits message from `goal` and `summary`.
/// Purely deterministic — never calls git or any external process.
pub fn generate_semantic_commit_message(goal: &str, summary: &str) -> String {
    let commit_type = detect_type(goal);
    let desc = build_desc(goal, summary);
    format!("{}: {}", commit_type, desc)
}

fn detect_type(goal: &str) -> &'static str {
    let g = goal.to_lowercase();
    if g.contains("fix") || g.contains("bug") || g.contains("patch") || g.contains("repair") {
        "fix"
    } else if g.contains("test") || g.contains("spec") {
        "test"
    } else if g.contains("doc") || g.contains("readme") || g.contains("comment") {
        "docs"
    } else if g.contains("refactor") || g.contains("clean") || g.contains("simplify") {
        "refactor"
    } else if g.contains("optim") || g.contains("perf") || g.contains("faster") || g.contains("speed") {
        "perf"
    } else if g.contains("style") || g.contains("lint") {
        "style"
    } else if g.contains("chore") || g.contains("bump") || g.contains("upgrade") {
        "chore"
    } else {
        "feat"
    }
}

fn build_desc(goal: &str, summary: &str) -> String {
    // Prefer summary if it is short and non-empty; fall back to goal
    let raw = if !summary.trim().is_empty() && summary.trim().len() <= 60 {
        summary.trim()
    } else {
        goal.trim()
    };

    let mut desc = raw.to_lowercase();

    // Strip trailing punctuation
    desc = desc
        .trim_end_matches(|c| matches!(c, '.' | '!' | '?'))
        .to_string();

    // Cap at 60 chars on a word boundary (safe UTF-8 handling)
    if desc.len() > 60 {
        let mut end = 60;
        while !desc.is_char_boundary(end) {
            end -= 1;
        }
        desc = match desc[..end].rfind(' ') {
            Some(space) => desc[..space].trim_end().to_string(),
            None => desc[..end].to_string(),
        };
    }

    desc
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_types() {
        assert!(generate_semantic_commit_message("fix the null pointer bug", "").starts_with("fix:"));
        assert!(generate_semantic_commit_message("add sandbox engine", "").starts_with("feat:"));
        assert!(generate_semantic_commit_message("refactor the TUI layout", "").starts_with("refactor:"));
        assert!(generate_semantic_commit_message("update docs for RAG module", "").starts_with("docs:"));
        assert!(generate_semantic_commit_message("optimize memory usage", "").starts_with("perf:"));
        assert!(generate_semantic_commit_message("bump tokio version", "").starts_with("chore:"));
    }

    #[test]
    fn summary_preferred_when_short() {
        let msg = generate_semantic_commit_message("fix a thing", "repair timeout in sandbox");
        assert!(msg.contains("repair timeout in sandbox"));
    }

    #[test]
    fn long_desc_truncated() {
        let long = "a".repeat(80);
        let msg = generate_semantic_commit_message("add new feature", &long);
        // description part should be at most 60 chars
        let desc = msg.splitn(2, ": ").nth(1).unwrap_or("");
        assert!(desc.len() <= 60);
    }

    #[test]
    fn is_git_repo_detects_dot_git() {
        // The project itself is a git repo
        let cwd = std::path::PathBuf::from(".");
        assert!(is_git_repo(&cwd));
    }
}
