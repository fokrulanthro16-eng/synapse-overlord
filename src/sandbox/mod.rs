#![allow(dead_code)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use tempfile::TempDir;
use tokio::process::Command;
use tokio::time::timeout;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 65_536; // 64 KB cap per stream

#[derive(Debug, Clone, Copy)]
pub enum SandboxLanguage {
    Rust,
    Python,
    Node,
    Text,
}

impl SandboxLanguage {
    pub fn label(self) -> &'static str {
        match self {
            SandboxLanguage::Rust => "rust",
            SandboxLanguage::Python => "python",
            SandboxLanguage::Node => "node",
            SandboxLanguage::Text => "text",
        }
    }
}

pub struct SandboxRequest {
    pub language: SandboxLanguage,
    pub code: String,
    pub timeout_secs: u64,
}

impl SandboxRequest {
    pub fn new(language: SandboxLanguage, code: impl Into<String>) -> Self {
        Self {
            language,
            code: code.into(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }
}

pub struct SandboxResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub language: SandboxLanguage,
    pub artifact_path: Option<PathBuf>,
}

impl SandboxResult {
    pub fn summary(&self) -> String {
        format!(
            "[{}] exit={} duration={}ms stdout={} chars stderr={} chars",
            self.language.label(),
            self.exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.duration_ms,
            self.stdout.len(),
            self.stderr.len(),
        )
    }
}

/// Run `request` in an isolated temp directory with a hard timeout.
pub async fn run_sandbox(request: SandboxRequest) -> Result<SandboxResult> {
    let start = Instant::now();
    let timeout_dur = Duration::from_secs(request.timeout_secs);
    let lang = request.language;

    let outcome = timeout(timeout_dur, execute(request)).await;
    let duration_ms = start.elapsed().as_millis();

    match outcome {
        Ok(Ok(mut result)) => {
            result.duration_ms = duration_ms;
            Ok(result)
        }
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => Ok(SandboxResult {
            success: false,
            stdout: String::new(),
            stderr: format!("Sandbox timed out after {} seconds.", timeout_dur.as_secs()),
            exit_code: None,
            duration_ms,
            language: lang,
            artifact_path: None,
        }),
    }
}

async fn execute(request: SandboxRequest) -> Result<SandboxResult> {
    match request.language {
        SandboxLanguage::Rust => run_rust(request).await,
        SandboxLanguage::Python => run_python(request).await,
        SandboxLanguage::Node => run_node(request).await,
        SandboxLanguage::Text => Ok(SandboxResult {
            success: true,
            stdout: request.code,
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 0,
            language: SandboxLanguage::Text,
            artifact_path: None,
        }),
    }
}

async fn run_rust(request: SandboxRequest) -> Result<SandboxResult> {
    let dir = TempDir::new()?;
    // EXE_SUFFIX is ".exe" on Windows, "" elsewhere
    let bin_name = format!("main{}", std::env::consts::EXE_SUFFIX);
    let bin_path = dir.path().join(&bin_name);

    tokio::fs::write(dir.path().join("main.rs"), request.code.as_bytes()).await?;

    // Step 1: compile
    let compile = Command::new("rustc")
        .current_dir(dir.path())
        .arg("main.rs")
        .arg("-o")
        .arg(&bin_name)
        .output()
        .await?;

    if !compile.status.success() {
        return Ok(SandboxResult {
            success: false,
            stdout: cap(compile.stdout),
            stderr: cap(compile.stderr),
            exit_code: compile.status.code(),
            duration_ms: 0,
            language: SandboxLanguage::Rust,
            artifact_path: None,
        });
    }

    // Step 2: execute the compiled binary
    let run = Command::new(&bin_path)
        .current_dir(dir.path())
        .output()
        .await?;

    Ok(SandboxResult {
        success: run.status.success(),
        stdout: cap(run.stdout),
        stderr: cap(run.stderr),
        exit_code: run.status.code(),
        duration_ms: 0,
        language: SandboxLanguage::Rust,
        artifact_path: None,
    })
}

async fn run_python(request: SandboxRequest) -> Result<SandboxResult> {
    let dir = TempDir::new()?;
    tokio::fs::write(dir.path().join("script.py"), request.code.as_bytes()).await?;

    // Try both common names; first one that can be spawned wins
    for cmd in ["python", "python3"] {
        match Command::new(cmd)
            .current_dir(dir.path())
            .arg("script.py")
            .output()
            .await
        {
            Ok(output) => {
                return Ok(SandboxResult {
                    success: output.status.success(),
                    stdout: cap(output.stdout),
                    stderr: cap(output.stderr),
                    exit_code: output.status.code(),
                    duration_ms: 0,
                    language: SandboxLanguage::Python,
                    artifact_path: None,
                });
            }
            Err(_) => continue,
        }
    }

    Ok(SandboxResult {
        success: false,
        stdout: String::new(),
        stderr: "Python not found. Ensure 'python' or 'python3' is in PATH.".to_string(),
        exit_code: None,
        duration_ms: 0,
        language: SandboxLanguage::Python,
        artifact_path: None,
    })
}

async fn run_node(request: SandboxRequest) -> Result<SandboxResult> {
    let dir = TempDir::new()?;
    tokio::fs::write(dir.path().join("script.js"), request.code.as_bytes()).await?;

    match Command::new("node")
        .current_dir(dir.path())
        .arg("script.js")
        .output()
        .await
    {
        Ok(output) => Ok(SandboxResult {
            success: output.status.success(),
            stdout: cap(output.stdout),
            stderr: cap(output.stderr),
            exit_code: output.status.code(),
            duration_ms: 0,
            language: SandboxLanguage::Node,
            artifact_path: None,
        }),
        Err(_) => Ok(SandboxResult {
            success: false,
            stdout: String::new(),
            stderr: "Node.js not found. Ensure 'node' is in PATH.".to_string(),
            exit_code: None,
            duration_ms: 0,
            language: SandboxLanguage::Node,
            artifact_path: None,
        }),
    }
}

/// Decode bytes as UTF-8 (lossy) and cap at MAX_OUTPUT_BYTES on a char boundary.
fn cap(bytes: Vec<u8>) -> String {
    let s = String::from_utf8_lossy(&bytes).into_owned();
    if s.len() <= MAX_OUTPUT_BYTES {
        return s;
    }
    // Walk back from the limit to find a valid char boundary
    let mut end = MAX_OUTPUT_BYTES;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}[...truncated]", &s[..end])
}
