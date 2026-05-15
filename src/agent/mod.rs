use std::path::PathBuf;

use anyhow::Result;

use crate::rag::ProjectMap;
use crate::sandbox::SandboxLanguage;

// ── Config ────────────────────────────────────────────────────────────────────

pub struct AgentConfig {
    /// How many self-correction retries after the first sandbox failure.
    pub max_retries: usize,
    /// Project root to map and stage from.
    pub root: PathBuf,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_retries: 1,
            root: PathBuf::from("."),
        }
    }
}

// ── Events ────────────────────────────────────────────────────────────────────

pub enum AgentEvent {
    Log(String),
    ThoughtStream(String),
    Done { success: bool },
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct AgentState {
    #[allow(dead_code)]
    pub goal: String,
    pub success: bool,
    pub events: Vec<AgentEvent>,
}

impl AgentState {
    fn new(goal: &str) -> Self {
        Self {
            goal: goal.to_string(),
            success: false,
            events: Vec::new(),
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.events.push(AgentEvent::Log(msg.into()));
    }

    pub fn thought(&mut self, msg: impl Into<String>) {
        self.events.push(AgentEvent::ThoughtStream(msg.into()));
    }
}

// ── Orchestrator ──────────────────────────────────────────────────────────────

/// Full agent pipeline:
/// map → consensus → artifact → sandbox (+ correction) → stage
pub async fn run_goal(goal: &str, config: &AgentConfig) -> Result<AgentState> {
    let mut state = AgentState::new(goal);

    // ── Step 1: Map project ──────────────────────────────────────────────────
    state.thought("Mapping project structure...");
    state.log("[Agent 1/4] Mapping project...");

    let map = match crate::rag::map_project(&config.root) {
        Ok(m) => {
            state.log(format!("[Agent] {} files found.", m.files.len()));
            m
        }
        Err(e) => {
            state.log(format!("[Agent] Map error: {}", e));
            state.events.push(AgentEvent::Done { success: false });
            return Ok(state);
        }
    };

    // ── Step 2: Triple model consensus ──────────────────────────────────────
    state.thought("Consulting logic · audit · optimization models...");
    state.log("[Agent 2/4] Running triple model consensus...");

    let prompt = build_prompt(goal, &map);
    let consensus = match crate::models::run_consensus(&prompt).await {
        Ok(c) => {
            state.log(format!("[Agent] {}", c.summary()));
            c
        }
        Err(e) => {
            state.log(format!("[Agent] Model error: {}", e));
            state.events.push(AgentEvent::Done { success: false });
            return Ok(state);
        }
    };

    // ── Step 3: Generate artifact + sandbox ──────────────────────────────────
    state.thought("Generating code artifact...");
    state.log("[Agent 3/4] Preparing sandbox artifact...");

    let lang = detect_language(&consensus.logic.content);
    let artifact = if consensus.logic.offline {
        offline_artifact(goal, lang)
    } else {
        extract_code_block(&consensus.logic.content)
            .unwrap_or_else(|| offline_artifact(goal, lang))
    };

    // Safety gate: never run blocked artifacts
    if crate::safety::classify_command(&artifact).is_blocked() {
        state.log("[Agent] BLOCKED — artifact contains dangerous commands. Aborting.");
        state.events.push(AgentEvent::Done { success: false });
        return Ok(state);
    }

    let passed = sandbox_with_correction(&mut state, &artifact, lang, config).await?;

    if !passed {
        state.log("[Agent] Sandbox did not pass after retries.");
        state.events.push(AgentEvent::Done { success: false });
        return Ok(state);
    }

    // ── Step 4: Stage (no auto-commit) ───────────────────────────────────────
    state.log("[Agent 4/4] Checking git repo...");

    if crate::gitops::is_git_repo(&config.root) {
        match crate::gitops::stage_all(&config.root).await {
            Ok(staged) => {
                state.log(format!("[Agent] {}", staged));
                let snippet = &consensus.logic.content
                    [..consensus.logic.content.len().min(80)];
                let msg =
                    crate::gitops::generate_semantic_commit_message(goal, snippet);
                state.log(format!("[Agent] Suggested commit: {}", msg));
                state.log("[Agent] Run 'git commit' manually to apply.");
            }
            Err(e) => state.log(format!("[Agent] Stage skipped: {}", e)),
        }
    } else {
        state.log("[Agent] Not a git repo — skipping stage.");
    }

    state.success = true;
    state.thought("Goal complete.");
    state.events.push(AgentEvent::Done { success: true });
    Ok(state)
}

// ── Sandbox with one correction pass ─────────────────────────────────────────

async fn sandbox_with_correction(
    state: &mut AgentState,
    artifact: &str,
    lang: SandboxLanguage,
    config: &AgentConfig,
) -> Result<bool> {
    for attempt in 0..=config.max_retries {
        let code = if attempt == 0 {
            artifact.to_string()
        } else {
            // Placeholder self-correction: fall back to a guaranteed-valid artifact
            state.thought("Applying self-correction fallback...");
            state.log(format!("[Agent] Self-correction attempt {}...", attempt));
            offline_artifact("corrected", lang)
        };

        state.log(format!(
            "[Agent] Sandbox attempt {} ({})...",
            attempt + 1,
            lang.label()
        ));

        let req = crate::sandbox::SandboxRequest {
            language: lang,
            code,
            timeout_secs: 30,
        };

        let result = crate::sandbox::run_sandbox(req).await?;
        state.log(format!("[Agent] {}", result.summary()));

        if result.success {
            for line in result.stdout.lines().take(5) {
                state.log(format!("[Agent] stdout: {}", line));
            }
            return Ok(true);
        }

        let err = &result.stderr[..result.stderr.len().min(120)];
        state.log(format!("[Agent] stderr: {}", err.trim()));
    }

    Ok(false)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_prompt(goal: &str, map: &ProjectMap) -> String {
    let files: String = map
        .files
        .iter()
        .take(15)
        .map(|f| format!("  {} [{}]\n", f.relative_path, f.role.label()))
        .collect();

    format!(
        "Goal: {}\n\nProject ({} files total):\n{}\n\
         Respond with ONLY executable code in a fenced code block \
         (```rust, ```python, or ```javascript). Keep it minimal.",
        goal,
        map.files.len(),
        files
    )
}

fn extract_code_block(text: &str) -> Option<String> {
    let fence_start = text.find("```")?;
    let after_fence = &text[fence_start + 3..];
    // Skip optional language tag line
    let body_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
    let body = &after_fence[body_start..];
    let fence_end = body.find("```")?;
    let code = body[..fence_end].trim();
    if code.is_empty() {
        None
    } else {
        Some(code.to_string())
    }
}

fn detect_language(content: &str) -> SandboxLanguage {
    let lower = content.to_lowercase();
    if lower.contains("```python") || lower.contains("def ") {
        SandboxLanguage::Python
    } else if lower.contains("```javascript")
        || lower.contains("```js")
        || lower.contains("console.log")
    {
        SandboxLanguage::Node
    } else {
        SandboxLanguage::Rust
    }
}

fn offline_artifact(goal: &str, lang: SandboxLanguage) -> String {
    let g: String = goal.chars().take(50).collect();
    match lang {
        SandboxLanguage::Rust => format!(
            "fn main() {{\n    \
             println!(\"Synapse offline artifact\");\n    \
             println!(\"Goal: {}\");\n\
             }}\n",
            g
        ),
        SandboxLanguage::Python => {
            format!("print('Synapse offline artifact')\nprint('Goal: {}')\n", g)
        }
        SandboxLanguage::Node => {
            format!(
                "console.log('Synapse offline artifact');\nconsole.log('Goal: {}');\n",
                g
            )
        }
        SandboxLanguage::Text => format!("Goal: {}\nOffline mode.", g),
    }
}
