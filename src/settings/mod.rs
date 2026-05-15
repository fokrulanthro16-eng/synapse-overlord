use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ── Sub-structs ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectEntry {
    pub name: String,
    pub path: String,
    pub added_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DbConnectionConfig {
    pub id: String,
    pub name: String,
    pub connector_type: String,
    pub path: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    // Password is never stored
}

// ── Settings struct ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // AI
    pub ai_mode: String,
    pub logic_model: String,
    pub audit_model: String,
    pub optimize_model: String,
    // Safety
    pub safety_block_destructive: bool,
    pub safety_needs_approval: bool,
    // API key (bool only — key is never stored here)
    pub api_key_set: bool,
    // IDE
    pub ide_connector: String,
    pub ide_workspace_path: String,
    // Profile
    pub profile_username: String,
    pub profile_display_name: String,
    // Projects
    pub projects: Vec<ProjectEntry>,
    pub active_project: String,
    // Database
    pub db_sqlite_path: String,
    pub db_connections: Vec<DbConnectionConfig>,
    pub active_db_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ai_mode: "cloud".to_string(),
            logic_model: "llama-3.3-70b-versatile".to_string(),
            audit_model: "llama-3.1-8b-instant".to_string(),
            optimize_model: "llama-3.1-8b-instant".to_string(),
            safety_block_destructive: true,
            safety_needs_approval: true,
            api_key_set: false,
            ide_connector: "none".to_string(),
            ide_workspace_path: String::new(),
            profile_username: String::new(),
            profile_display_name: String::new(),
            projects: Vec::new(),
            active_project: String::new(),
            db_sqlite_path: String::new(),
            db_connections: Vec::new(),
            active_db_id: String::new(),
        }
    }
}

// ── File helpers ──────────────────────────────────────────────────────────────

fn settings_path(root: &Path) -> PathBuf {
    root.join(".synapse").join("settings.json")
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Load / save ───────────────────────────────────────────────────────────────

pub fn load(root: &Path) -> Result<Settings> {
    let path = settings_path(root);
    let mut s = if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        serde_json::from_str(&text).unwrap_or_default()
    } else {
        Settings::default()
    };
    s.api_key_set = api_key_exists();
    Ok(s)
}

pub fn save(root: &Path, settings: &Settings) -> Result<()> {
    let dir = root.join(".synapse");
    std::fs::create_dir_all(&dir)?;
    let text = serde_json::to_string_pretty(settings)?;
    std::fs::write(dir.join("settings.json"), text)?;
    Ok(())
}

// ── Project management ────────────────────────────────────────────────────────

pub fn add_project(root: &Path, entry: ProjectEntry) -> Result<Settings> {
    let mut s = load(root)?;
    s.projects.retain(|p| p.path != entry.path);
    s.projects.push(entry);
    save(root, &s)?;
    Ok(s)
}

pub fn remove_project(root: &Path, path: &str) -> Result<Settings> {
    let mut s = load(root)?;
    s.projects.retain(|p| p.path != path);
    if s.active_project == path {
        s.active_project = String::new();
    }
    save(root, &s)?;
    Ok(s)
}

pub fn switch_project(root: &Path, path: &str) -> Result<Settings> {
    let mut s = load(root)?;
    if s.projects.iter().any(|p| p.path == path) {
        s.active_project = path.to_string();
        save(root, &s)?;
    }
    Ok(s)
}

// ── DB connection management ──────────────────────────────────────────────────

pub fn save_db_connection(root: &Path, conn: DbConnectionConfig) -> Result<Settings> {
    let mut s = load(root)?;
    if let Some(existing) = s.db_connections.iter_mut().find(|c| c.id == conn.id) {
        *existing = conn;
    } else {
        s.db_connections.push(conn);
    }
    save(root, &s)?;
    Ok(s)
}

pub fn remove_db_connection(root: &Path, id: &str) -> Result<Settings> {
    let mut s = load(root)?;
    s.db_connections.retain(|c| c.id != id);
    if s.active_db_id == id {
        s.active_db_id = String::new();
    }
    save(root, &s)?;
    Ok(s)
}

// ── .env helpers ──────────────────────────────────────────────────────────────

pub fn update_env_var(key: &str, value: &str) -> Result<()> {
    let env_path = Path::new(".env");
    let existing = if env_path.exists() {
        std::fs::read_to_string(env_path)?
    } else {
        String::new()
    };

    let prefix = format!("{}=", key);
    let new_line = format!("{}={}", key, value);
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    let mut found = false;

    for line in &mut lines {
        if line.starts_with(&prefix) {
            *line = new_line.clone();
            found = true;
            break;
        }
    }
    if !found {
        lines.push(new_line);
    }

    let mut content = lines.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    std::fs::write(env_path, content)?;

    // SAFETY: single-threaded at call site; no concurrent env reads during save
    unsafe { std::env::set_var(key, value); }
    Ok(())
}

pub fn api_key_exists() -> bool {
    std::env::var("GROQ_API_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}
