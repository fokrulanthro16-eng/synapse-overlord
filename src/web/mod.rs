use std::sync::{Arc, Mutex};

use anyhow::Result;
use tracing::info;
use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::system::SystemMonitor;

// ── Shared state ──────────────────────────────────────────────────────────────

struct WebState {
    monitor: Mutex<SystemMonitor>,
    status: Mutex<String>,
    settings: Mutex<crate::settings::Settings>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run_web_server() -> Result<()> {
    // Structured logging: RUST_LOG=synapse_overlord=debug for verbose output.
    // Falls back to info-level if the env var is absent or unparseable.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("synapse_overlord=info")
            }),
        )
        .try_init();

    let settings =
        crate::settings::load(std::path::Path::new(".")).unwrap_or_default();

    let state = Arc::new(WebState {
        monitor: Mutex::new(SystemMonitor::new()),
        status: Mutex::new("Idle".to_string()),
        settings: Mutex::new(settings),
    });

    let app = Router::new()
        // Dashboard
        .route("/", get(dashboard))
        // Health + command (existing)
        .route("/api/health", get(health))
        .route("/api/command", post(command))
        // Settings (existing)
        .route("/api/settings", get(get_settings).post(post_settings))
        .route("/api/settings/api-keys", post(post_api_keys))
        // Profile
        .route("/api/profile/save", post(save_profile))
        // IDE
        .route("/api/ide/save", post(save_ide))
        // Projects
        .route("/api/projects", get(get_projects))
        .route("/api/projects/add", post(add_project))
        .route("/api/projects/switch", post(switch_project))
        .route("/api/projects/remove", post(remove_project))
        // Integrations status
        .route("/api/integrations/status", get(integrations_status))
        // SQLite (existing)
        .route("/api/database/test", post(db_test))
        .route("/api/database/sqlite/tables", post(db_tables))
        .route("/api/database/sqlite/schema", post(db_schema))
        .route("/api/database/sqlite/query", post(db_query))
        // Typed multi-connector
        .route("/api/database/connect", post(db_connect))
        .route("/api/database/connections/save", post(db_connection_save))
        .route("/api/database/connections/remove", post(db_connection_remove))
        // Generated projects
        .route("/api/generated-projects", get(list_generated_projects))
        .route("/project/{slug}", get(serve_project_page))
        .route("/generated/{slug}/{file}", get(serve_project_file))
        .route("/api/projects/download/{slug}", get(download_project_zip))
        .route("/api/projects/enhance", post(api_enhance_project))
        // ── Hermes Architect (Phase 2 — read-only analysis) ──────────────────
        .route("/api/architect/analyze", post(crate::routes::hermes::analyze_handler))
        // ── Hermes Architect (Phase 3 — safe patch proposal workflow) ─────────
        .route("/api/architect/propose",             post(crate::routes::hermes::propose_handler))
        .route("/api/architect/apply/{patch_id}",     post(crate::routes::hermes::apply_handler))
        .route("/api/architect/reject/{patch_id}",   post(crate::routes::hermes::reject_handler))
        .route("/api/architect/patches",             get(crate::routes::hermes::list_patches_handler))
        .layer(axum::Extension(crate::tools::PatchStore::new()))
        .with_state(state);

    // Bind first — only print the URL once the socket is confirmed open.
    // Use 127.0.0.1 so the port is localhost-only by default; change to
    // 0.0.0.0 if LAN access is needed.
    let listener = TcpListener::bind("127.0.0.1:3000").await.map_err(|e| {
        anyhow::anyhow!(
            "Cannot bind to 127.0.0.1:3000 — port may already be in use.\n\
             Kill the process holding port 3000 and retry.\n\
             Error: {}",
            e
        )
    })?;

    let addr = listener.local_addr()?;

    // println! is unconditional — always visible regardless of RUST_LOG level.
    println!();
    println!("  ╔══════════════════════════════════════════╗");
    println!("  ║      SYNAPSE-OVERLORD  ◈  web mode       ║");
    println!("  ╠══════════════════════════════════════════╣");
    println!("  ║  http://127.0.0.1:{}                   ║", addr.port());
    println!("  ║  Press Ctrl-C to stop                    ║");
    println!("  ╚══════════════════════════════════════════╝");
    println!();

    info!(%addr, "web server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

// ── GET / ─────────────────────────────────────────────────────────────────────

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

// ── Shared response types ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct SavedResponse {
    saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn ok() -> Json<SavedResponse> {
    Json(SavedResponse { saved: true, error: None })
}
fn err(msg: impl Into<String>) -> Json<SavedResponse> {
    Json(SavedResponse { saved: false, error: Some(msg.into()) })
}

// ── GET /api/health ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct HealthResponse {
    cpu_percent: f32,
    ram_percent: f32,
    ram_used_mb: u64,
    ram_total_mb: u64,
    status: String,
    profile_username: String,
    active_project: String,
}

async fn health(State(state): State<Arc<WebState>>) -> Json<HealthResponse> {
    let snap = { let mut m = state.monitor.lock().unwrap(); m.snapshot() };
    let (status, username, project) = {
        let s = state.settings.lock().unwrap();
        (
            state.status.lock().unwrap().clone(),
            s.profile_username.clone(),
            s.active_project.clone(),
        )
    };
    Json(HealthResponse {
        cpu_percent: snap.cpu_percent,
        ram_percent: snap.ram_percent,
        ram_used_mb: snap.ram_used_mb,
        ram_total_mb: snap.ram_total_mb,
        status,
        profile_username: username,
        active_project: project,
    })
}

// ── POST /api/command ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CommandRequest { command: String }

#[derive(Serialize)]
struct CommandResponse { logs: Vec<String>, thoughts: Vec<String> }

async fn command(
    State(state): State<Arc<WebState>>,
    Json(req): Json<CommandRequest>,
) -> Json<CommandResponse> {
    { *state.status.lock().unwrap() = "Running".to_string(); }
    let result = dispatch(&req.command).await;
    { *state.status.lock().unwrap() = "Idle".to_string(); }
    Json(result)
}

// ── GET|POST /api/settings ────────────────────────────────────────────────────

async fn get_settings(State(state): State<Arc<WebState>>) -> Json<crate::settings::Settings> {
    let mut s = state.settings.lock().unwrap().clone();
    s.api_key_set = crate::settings::api_key_exists();
    Json(s)
}

async fn post_settings(
    State(state): State<Arc<WebState>>,
    Json(mut settings): Json<crate::settings::Settings>,
) -> Json<SavedResponse> {
    settings.api_key_set = crate::settings::api_key_exists();
    match crate::settings::save(std::path::Path::new("."), &settings) {
        Ok(()) => { *state.settings.lock().unwrap() = settings; ok() }
        Err(e) => err(e.to_string()),
    }
}

// ── POST /api/settings/api-keys ───────────────────────────────────────────────

#[derive(Deserialize)]
struct ApiKeysRequest {
    groq_api_key: Option<String>,
    logic_model: Option<String>,
    audit_model: Option<String>,
    optimize_model: Option<String>,
}

async fn post_api_keys(
    State(state): State<Arc<WebState>>,
    Json(req): Json<ApiKeysRequest>,
) -> Json<SavedResponse> {
    let mut saved = true;
    if let Some(k) = &req.groq_api_key {
        let k = k.trim();
        if !k.is_empty() && crate::settings::update_env_var("GROQ_API_KEY", k).is_err() {
            saved = false;
        }
    }
    for (env_k, val) in [
        ("SYNAPSE_LOGIC_MODEL", &req.logic_model),
        ("SYNAPSE_AUDIT_MODEL", &req.audit_model),
        ("SYNAPSE_OPTIMIZE_MODEL", &req.optimize_model),
    ] {
        if let Some(v) = val {
            let v = v.trim();
            if !v.is_empty() && crate::settings::update_env_var(env_k, v).is_err() {
                saved = false;
            }
        }
    }
    state.settings.lock().unwrap().api_key_set = crate::settings::api_key_exists();
    Json(SavedResponse { saved, error: None })
}

// ── POST /api/profile/save ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ProfileRequest { username: String, display_name: String }

async fn save_profile(
    State(state): State<Arc<WebState>>,
    Json(req): Json<ProfileRequest>,
) -> Json<SavedResponse> {
    let mut s = state.settings.lock().unwrap().clone();
    s.profile_username = req.username.trim().to_string();
    s.profile_display_name = req.display_name.trim().to_string();
    match crate::settings::save(std::path::Path::new("."), &s) {
        Ok(()) => { *state.settings.lock().unwrap() = s; ok() }
        Err(e) => err(e.to_string()),
    }
}

// ── POST /api/ide/save ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct IdeSaveRequest { ide_connector: String, ide_workspace_path: String }

async fn save_ide(
    State(state): State<Arc<WebState>>,
    Json(req): Json<IdeSaveRequest>,
) -> Json<SavedResponse> {
    let wp = req.ide_workspace_path.trim();
    if !wp.is_empty() && !std::path::Path::new(wp).exists() {
        return err(format!("Path does not exist: {}", wp));
    }
    let mut s = state.settings.lock().unwrap().clone();
    s.ide_connector = req.ide_connector;
    s.ide_workspace_path = wp.to_string();
    match crate::settings::save(std::path::Path::new("."), &s) {
        Ok(()) => { *state.settings.lock().unwrap() = s; ok() }
        Err(e) => err(e.to_string()),
    }
}

// ── GET /api/projects ─────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ProjectsResponse {
    projects: Vec<crate::settings::ProjectEntry>,
    active_project: String,
}

async fn get_projects(State(state): State<Arc<WebState>>) -> Json<ProjectsResponse> {
    let s = state.settings.lock().unwrap();
    Json(ProjectsResponse {
        projects: s.projects.clone(),
        active_project: s.active_project.clone(),
    })
}

// ── POST /api/projects/add ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AddProjectRequest { name: String, path: String }

async fn add_project(
    State(state): State<Arc<WebState>>,
    Json(req): Json<AddProjectRequest>,
) -> Json<SavedResponse> {
    let path = req.path.trim().to_string();
    if path.is_empty() || req.name.trim().is_empty() {
        return err("Name and path are required");
    }
    if !std::path::Path::new(&path).exists() {
        return err(format!("Path does not exist: {}", path));
    }
    let entry = crate::settings::ProjectEntry {
        name: req.name.trim().to_string(),
        path,
        added_at: crate::settings::now_secs(),
    };
    match crate::settings::add_project(std::path::Path::new("."), entry) {
        Ok(s) => { *state.settings.lock().unwrap() = s; ok() }
        Err(e) => err(e.to_string()),
    }
}

// ── POST /api/projects/switch ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct PathRequest { path: String }

async fn switch_project(
    State(state): State<Arc<WebState>>,
    Json(req): Json<PathRequest>,
) -> Json<SavedResponse> {
    match crate::settings::switch_project(std::path::Path::new("."), &req.path) {
        Ok(s) => { *state.settings.lock().unwrap() = s; ok() }
        Err(e) => err(e.to_string()),
    }
}

// ── POST /api/projects/remove ─────────────────────────────────────────────────

async fn remove_project(
    State(state): State<Arc<WebState>>,
    Json(req): Json<PathRequest>,
) -> Json<SavedResponse> {
    match crate::settings::remove_project(std::path::Path::new("."), &req.path) {
        Ok(s) => { *state.settings.lock().unwrap() = s; ok() }
        Err(e) => err(e.to_string()),
    }
}

// ── GET /api/integrations/status ──────────────────────────────────────────────

#[derive(Serialize)]
struct IntegrationsStatus {
    ide: String,
    ide_path: String,
    ide_path_valid: bool,
    db_connections: usize,
    active_project: String,
    project_count: usize,
    profile_username: String,
    api_key_set: bool,
}

async fn integrations_status(State(state): State<Arc<WebState>>) -> Json<IntegrationsStatus> {
    let s = state.settings.lock().unwrap().clone();
    let ide_path_valid = !s.ide_workspace_path.is_empty()
        && std::path::Path::new(&s.ide_workspace_path).exists();
    Json(IntegrationsStatus {
        ide: s.ide_connector,
        ide_path: s.ide_workspace_path,
        ide_path_valid,
        db_connections: s.db_connections.len(),
        active_project: s.active_project,
        project_count: s.projects.len(),
        profile_username: s.profile_username,
        api_key_set: crate::settings::api_key_exists(),
    })
}

// ── POST /api/database/test ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct DbPathRequest { path: String }

#[derive(Serialize)]
struct DbTestResponse { ok: bool, status: String }

async fn db_test(Json(req): Json<DbPathRequest>) -> Json<DbTestResponse> {
    let path = req.path;
    match tokio::task::spawn_blocking(move || crate::database::test_connection(&path)).await {
        Ok(Ok(msg)) => Json(DbTestResponse { ok: true, status: msg }),
        Ok(Err(e)) => Json(DbTestResponse { ok: false, status: e.to_string() }),
        Err(_) => Json(DbTestResponse { ok: false, status: "Internal error".to_string() }),
    }
}

// ── POST /api/database/connect (typed) ───────────────────────────────────────

#[derive(Deserialize)]
struct DbConnectRequest {
    connector_type: String,
    path: Option<String>,
    #[allow(dead_code)] host: Option<String>,
    #[allow(dead_code)] port: Option<u16>,
    #[allow(dead_code)] database: Option<String>,
    #[allow(dead_code)] username: Option<String>,
}

async fn db_connect(Json(req): Json<DbConnectRequest>) -> Json<DbTestResponse> {
    let ct = req.connector_type;
    let path = req.path;
    match tokio::task::spawn_blocking(move || {
        crate::database::test_connection_by_type(&ct, path.as_deref())
    }).await {
        Ok(Ok(msg)) => Json(DbTestResponse { ok: true, status: msg }),
        Ok(Err(e)) => Json(DbTestResponse { ok: false, status: e.to_string() }),
        Err(_) => Json(DbTestResponse { ok: false, status: "Internal error".to_string() }),
    }
}

// ── POST /api/database/connections/save ───────────────────────────────────────

#[derive(Deserialize)]
struct DbConnSaveRequest {
    id: Option<String>,
    name: String,
    connector_type: String,
    path: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    username: Option<String>,
}

async fn db_connection_save(
    State(state): State<Arc<WebState>>,
    Json(req): Json<DbConnSaveRequest>,
) -> Json<SavedResponse> {
    if req.name.trim().is_empty() {
        return err("Connection name is required");
    }
    let id = req.id.filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("conn_{}", crate::settings::now_secs()));
    let conn = crate::settings::DbConnectionConfig {
        id,
        name: req.name.trim().to_string(),
        connector_type: req.connector_type,
        path: req.path.filter(|s| !s.is_empty()),
        host: req.host.filter(|s| !s.is_empty()),
        port: req.port,
        database: req.database.filter(|s| !s.is_empty()),
        username: req.username.filter(|s| !s.is_empty()),
    };
    match crate::settings::save_db_connection(std::path::Path::new("."), conn) {
        Ok(s) => { *state.settings.lock().unwrap() = s; ok() }
        Err(e) => err(e.to_string()),
    }
}

// ── POST /api/database/connections/remove ─────────────────────────────────────

#[derive(Deserialize)]
struct DbConnRemoveRequest { id: String }

async fn db_connection_remove(
    State(state): State<Arc<WebState>>,
    Json(req): Json<DbConnRemoveRequest>,
) -> Json<SavedResponse> {
    match crate::settings::remove_db_connection(std::path::Path::new("."), &req.id) {
        Ok(s) => { *state.settings.lock().unwrap() = s; ok() }
        Err(e) => err(e.to_string()),
    }
}

// ── SQLite explorer routes (existing, unchanged) ──────────────────────────────

#[derive(Serialize)]
struct DbTablesResponse {
    ok: bool,
    tables: Vec<crate::database::TableInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn db_tables(Json(req): Json<DbPathRequest>) -> Json<DbTablesResponse> {
    let path = req.path;
    match tokio::task::spawn_blocking(move || crate::database::list_tables(&path)).await {
        Ok(Ok(t)) => Json(DbTablesResponse { ok: true, tables: t, error: None }),
        Ok(Err(e)) => Json(DbTablesResponse { ok: false, tables: vec![], error: Some(e.to_string()) }),
        Err(_) => Json(DbTablesResponse { ok: false, tables: vec![], error: Some("Internal error".to_string()) }),
    }
}

#[derive(Deserialize)]
struct DbSchemaRequest { path: String, table: String }

#[derive(Serialize)]
struct DbSchemaResponse {
    ok: bool,
    columns: Vec<crate::database::ColumnInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn db_schema(Json(req): Json<DbSchemaRequest>) -> Json<DbSchemaResponse> {
    let (path, table) = (req.path, req.table);
    match tokio::task::spawn_blocking(move || crate::database::table_schema(&path, &table)).await {
        Ok(Ok(c)) => Json(DbSchemaResponse { ok: true, columns: c, error: None }),
        Ok(Err(e)) => Json(DbSchemaResponse { ok: false, columns: vec![], error: Some(e.to_string()) }),
        Err(_) => Json(DbSchemaResponse { ok: false, columns: vec![], error: Some("Internal error".to_string()) }),
    }
}

#[derive(Deserialize)]
struct DbQueryRequest { path: String, sql: String }

async fn db_query(Json(req): Json<DbQueryRequest>) -> Json<serde_json::Value> {
    let (path, sql) = (req.path, req.sql);
    match tokio::task::spawn_blocking(move || crate::database::run_query(&path, &sql)).await {
        Ok(Ok(r)) => Json(serde_json::json!({
            "ok": true, "columns": r.columns, "rows": r.rows, "row_count": r.row_count,
        })),
        Ok(Err(e)) => Json(serde_json::json!({
            "ok": false, "error": e.to_string(), "columns": [], "rows": [], "row_count": 0,
        })),
        Err(_) => Json(serde_json::json!({
            "ok": false, "error": "Internal error", "columns": [], "rows": [], "row_count": 0,
        })),
    }
}

// ── POST /api/projects/enhance ────────────────────────────────────────────────

#[derive(Deserialize)]
struct EnhanceRequest {
    project: String,
    mode: String,
    instruction: String,
}

#[derive(Serialize)]
struct EnhanceResponse {
    success: bool,
    logs: Vec<String>,
    slug: String,
    backup_path: String,
    changed_files: Vec<String>,
}

async fn api_enhance_project(Json(req): Json<EnhanceRequest>) -> Json<EnhanceResponse> {
    let out = crate::enhancer::enhance_project(&req.project, &req.mode, &req.instruction).await;
    let success = !out.slug.is_empty() && !out.changed_files.is_empty();
    Json(EnhanceResponse {
        success,
        logs: out.logs,
        slug: out.slug,
        backup_path: out.backup_path,
        changed_files: out.changed_files,
    })
}

// ── Generated projects ────────────────────────────────────────────────────────

fn valid_slug(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '-')
}

fn valid_asset(f: &str) -> bool {
    matches!(f, "styles.css" | "app.js" | "index.html" | "README.md")
}

fn slug_to_name(slug: &str) -> String {
    slug.split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Serialize)]
struct GeneratedProject {
    slug: String,
    name: String,
    project_type: String,
    path: String,
    file_count: usize,
    generated_at: u64,
}

async fn list_generated_projects() -> Json<Vec<GeneratedProject>> {
    let dir = std::path::Path::new("generated_projects");
    if !dir.exists() {
        return Json(vec![]);
    }
    let mut projects = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let slug = entry.file_name().to_string_lossy().to_string();
            if !valid_slug(&slug) {
                continue;
            }
            let mut project_type = "unknown".to_string();
            let mut generated_at = 0u64;
            let meta_path = path.join(".synapse-meta.json");
            if let Ok(meta_str) = std::fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                    project_type = meta.get("project_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    generated_at = meta.get("generated_at")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                }
            }
            let file_count = std::fs::read_dir(&path)
                .map(|rd| rd.flatten().filter(|e| e.path().is_file()).count())
                .unwrap_or(0);
            projects.push(GeneratedProject {
                name: slug_to_name(&slug),
                path: path.display().to_string(),
                slug,
                project_type,
                file_count,
                generated_at,
            });
        }
    }
    projects.sort_by(|a, b| b.generated_at.cmp(&a.generated_at));
    Json(projects)
}

async fn serve_project_page(Path(slug): Path<String>) -> Response {
    if !valid_slug(&slug) {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Invalid slug"))
            .unwrap();
    }
    let index = std::path::Path::new("generated_projects")
        .join(&slug)
        .join("index.html");
    if !index.exists() {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Project not found"))
            .unwrap();
    }
    match std::fs::read_to_string(&index) {
        Ok(mut html) => {
            let base = format!("<base href=\"/generated/{}/\">", slug);
            if let Some(pos) = html.find("<head>") {
                html.insert_str(pos + "<head>".len(), &base);
            } else {
                html.insert_str(0, &base);
            }
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(Body::from(html))
                .unwrap()
        }
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Read error"))
            .unwrap(),
    }
}

async fn serve_project_file(
    Path((slug, file)): Path<(String, String)>,
) -> Response {
    if !valid_slug(&slug) || !valid_asset(&file) {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Invalid request"))
            .unwrap();
    }
    let asset = std::path::Path::new("generated_projects")
        .join(&slug)
        .join(&file);
    match std::fs::read(&asset) {
        Ok(bytes) => {
            let ct = if file.ends_with(".css") { "text/css" }
                else if file.ends_with(".js") { "application/javascript" }
                else if file.ends_with(".html") { "text/html; charset=utf-8" }
                else { "text/plain; charset=utf-8" };
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", ct)
                .body(Body::from(bytes))
                .unwrap()
        }
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("File not found"))
            .unwrap(),
    }
}

async fn download_project_zip(Path(slug): Path<String>) -> Response {
    if !valid_slug(&slug) {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Invalid slug"))
            .unwrap();
    }
    let slug_clone = slug.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        use std::io::Write;
        let dir = std::path::Path::new("generated_projects").join(&slug_clone);
        if !dir.exists() {
            return Err("Project not found".to_string());
        }
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let opts = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for entry in walkdir::WalkDir::new(&dir).min_depth(1).max_depth(1) {
            let e = entry.map_err(|e| e.to_string())?;
            if !e.path().is_file() { continue; }
            let fname = e.file_name().to_string_lossy().to_string();
            if fname.starts_with('.') { continue; }
            let bytes = std::fs::read(e.path()).map_err(|e| e.to_string())?;
            zip.start_file(&fname, opts).map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
    })
    .await;
    match result {
        Ok(Ok(bytes)) => {
            let cd = format!("attachment; filename=\"{}.zip\"", slug);
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/zip")
                .header("Content-Disposition", cd)
                .body(Body::from(bytes))
                .unwrap()
        }
        Ok(Err(msg)) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(msg))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Internal error"))
            .unwrap(),
    }
}

// ── Command dispatch ──────────────────────────────────────────────────────────

fn split_proj_instr<'a>(rest: &'a str, default: &'a str) -> (&'a str, &'a str) {
    if let Some(i) = rest.find(' ') { (&rest[..i], rest[i + 1..].trim()) }
    else { (rest.trim(), default) }
}

async fn dispatch(cmd: &str) -> CommandResponse {
    let lower = cmd.trim().to_lowercase();
    let trimmed = lower.as_str();

    if let Some(rest) = trimmed.strip_prefix("improve project ") {
        let (proj, instr) = split_proj_instr(rest, "general improvement");
        let out = crate::enhancer::enhance_project(proj, "improve", instr).await;
        return CommandResponse { logs: out.logs, thoughts: vec![format!("[Enhancer] {}", out.slug)] };
    }
    if let Some(rest) = trimmed.strip_prefix("add feature ") {
        let (proj, instr) = split_proj_instr(rest, "enhancement");
        let out = crate::enhancer::enhance_project(proj, "add_feature", instr).await;
        return CommandResponse { logs: out.logs, thoughts: vec![format!("[Enhancer] {}", out.slug)] };
    }

    // build project <idea>  (must be checked before the match block)
    if let Some(idea) = trimmed.strip_prefix("build project") {
        let out = crate::builder::handle_build_project_command(idea).await;
        return CommandResponse { logs: out.logs, thoughts: out.thoughts };
    }

    match trimmed {
        "map project"  => cmd_map_project().await,
        "test sandbox" => cmd_test_sandbox().await,
        "ask models"   => cmd_ask_models().await,
        "run agent"    => cmd_run_agent().await,
        other => CommandResponse {
            logs: vec![format!(
                "[TUI] Unknown '{}'. Try: map project | test sandbox | ask models | run agent | build project <idea>",
                other
            )],
            thoughts: vec![format!("Unknown command: {}", other)],
        },
    }
}

async fn cmd_map_project() -> CommandResponse {
    let mut logs = vec!["[RAG] Scanning project structure...".to_string()];
    let mut thoughts = vec!["Mapping project files...".to_string()];
    match crate::rag::map_project(std::path::Path::new(".")) {
        Ok(map) => {
            logs.push(format!(
                "[RAG] {} files mapped  |  {} skipped  |  root: {}",
                map.files.len(), map.skipped_count, map.root.display()
            ));
            for node in map.files.iter().take(50) {
                let size = if node.size_bytes >= 1024 { format!("{}KB", node.size_bytes / 1024) }
                           else { format!("{}B", node.size_bytes) };
                let imp = if node.imports.is_empty() { String::new() }
                          else { format!("  [uses: {}]", node.imports.join(", ")) };
                logs.push(format!("  {}  [{}]  {}{}", node.relative_path, node.role.label(), size, imp));
            }
            if map.files.len() > 50 { logs.push(format!("  ... and {} more", map.files.len() - 50)); }
            thoughts.push(format!("Project mapped: {} files.", map.files.len()));
        }
        Err(e) => logs.push(format!("[RAG ERROR] {}", e)),
    }
    CommandResponse { logs, thoughts }
}

async fn cmd_test_sandbox() -> CommandResponse {
    let mut logs = vec!["[Sandbox] Compiling Rust test artifact...".to_string()];
    let mut thoughts = vec!["Sandbox: compiling Rust...".to_string()];
    let code = "fn main(){println!(\"Synapse sandbox: OK\");println!(\"Rust execution: confirmed\");}\n";
    let req = crate::sandbox::SandboxRequest::new(crate::sandbox::SandboxLanguage::Rust, code);
    match crate::sandbox::run_sandbox(req).await {
        Ok(r) => {
            logs.push(format!("[Sandbox] {}", r.summary()));
            for line in r.stdout.lines().take(10) { logs.push(format!("[Sandbox] {}", line)); }
            if !r.success && !r.stderr.is_empty() {
                logs.push(format!("[Sandbox] stderr: {}", &r.stderr[..r.stderr.len().min(200)].trim()));
            }
            thoughts.push(if r.success { "Sandbox: PASS".to_string() } else { "Sandbox: FAIL".to_string() });
        }
        Err(e) => logs.push(format!("[Sandbox ERROR] {}", e)),
    }
    CommandResponse { logs, thoughts }
}

async fn cmd_ask_models() -> CommandResponse {
    let mut logs = vec!["[Models] Running triple consensus...".to_string()];
    let mut thoughts = vec!["Sending to logic · audit · optimization models...".to_string()];
    let prompt = "Analyze the Synapse-Overlord Rust project and suggest one concrete improvement.";
    match crate::models::run_consensus(prompt).await {
        Ok(c) => {
            logs.push(format!("[Models] {}", c.summary()));
            for (l, r) in [("Logic", &c.logic), ("Audit", &c.audit), ("Optimize", &c.optimize)] {
                logs.push(format!("[{}] {}", l, r.content.chars().take(200).collect::<String>().trim().to_string()));
            }
            thoughts.push(if c.logic.offline { "Models: OFFLINE fallback".to_string() } else { "Models: consensus complete".to_string() });
        }
        Err(e) => logs.push(format!("[Models ERROR] {}", e)),
    }
    CommandResponse { logs, thoughts }
}

async fn cmd_run_agent() -> CommandResponse {
    let mut logs = vec!["[Agent] Starting full pipeline...".to_string()];
    let mut thoughts = vec!["Agent: initializing...".to_string()];
    let config = crate::agent::AgentConfig::default();
    let goal = "Analyze this Rust project structure and generate a minimal summary artifact.";
    match crate::agent::run_goal(goal, &config).await {
        Ok(s) => {
            for ev in s.events {
                match ev {
                    crate::agent::AgentEvent::Log(m) => logs.push(m),
                    crate::agent::AgentEvent::ThoughtStream(m) => thoughts.push(m),
                    crate::agent::AgentEvent::Done { success } => {
                        logs.push(format!("[Agent] Pipeline {}.", if success { "SUCCEEDED" } else { "FAILED" }));
                    }
                }
            }
        }
        Err(e) => logs.push(format!("[Agent ERROR] {}", e)),
    }
    CommandResponse { logs, thoughts }
}

// ── Embedded dashboard ────────────────────────────────────────────────────────

const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>SYNAPSE</title>
<style>
*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}
:root{--bg:#eeecea;--white:#ffffff;--border:#e2e0d8;--sidebar:#0d0d1f;--cyan:#00d4ff;--green:#4caf50;--mag:#e040fb;--yel:#ffeb3b;--sh:0 2px 12px rgba(0,0,0,.07)}
body{background:var(--bg);font-family:'Courier New',Consolas,monospace;display:flex;height:100vh;overflow:hidden;color:#1e1e2e}

/* ── Sidebar ── */
#sb{width:220px;flex-shrink:0;background:var(--sidebar);border-right:1px solid #1c1c30;display:flex;flex-direction:column;padding:16px 14px;gap:4px;overflow-y:auto}
.brand{color:var(--cyan);font-size:16px;font-weight:bold;letter-spacing:2px;padding-bottom:10px;border-bottom:1px solid #1c1c30;text-shadow:0 0 12px rgba(0,212,255,.4)}
.profbar{font-size:11px;color:#666;padding:4px 0 6px;border-bottom:1px solid #1a1a1a;margin-bottom:2px}
.profbar b{color:#00ff88}
#sd{font-size:12px;margin-bottom:2px}.idle{color:#00ff88}.running{color:var(--yel)}
.slbl{font-size:9px;letter-spacing:2px;color:#3a3a5c;font-weight:bold;margin-top:10px;margin-bottom:4px;text-transform:uppercase}
.cb{width:100%;padding:8px 12px;border:none;border-radius:6px;font-family:inherit;font-size:12px;font-weight:bold;cursor:pointer;text-align:left;transition:filter .15s,transform .1s}
.cb:hover:not(:disabled){filter:brightness(1.18);transform:translateX(2px)}.cb:disabled{opacity:.35;cursor:not-allowed}
.bc{background:var(--cyan);color:#000}.bg{background:var(--green);color:#000}
.bm{background:var(--mag);color:#000}.by{background:var(--yel);color:#000}
.bb{background:#2196f3;color:#fff}.bt{background:#00897b;color:#fff}
.bv{background:#5c6bc0;color:#fff}.bsl{background:#455a64;color:#fff}.bgo{background:#fb923c;color:#000}
.spacer{flex:1}
.sb-proj{font-size:11px;color:#555;padding:3px 0;border-bottom:1px solid #1a1a1a}
.sb-proj span{color:#aaa}
/* ── Hermes Agent ── */
.hermes-badge{font-size:9px;letter-spacing:1.5px;color:#00d4ff;background:rgba(0,212,255,.1);border:1px solid rgba(0,212,255,.25);border-radius:20px;padding:3px 10px;margin:2px 0 6px;text-align:center;font-weight:bold}
.hermes-pill{display:inline-flex;align-items:center;gap:4px;font-size:9px;padding:2px 9px;border-radius:10px;font-weight:bold;letter-spacing:.5px;white-space:nowrap}
.hp-idle{background:#0f172a;color:#475569}.hp-scan{background:#1e3a5f;color:#93c5fd}.hp-read{background:#14312a;color:#6ee7b7}.hp-analyze{background:#3b1f0e;color:#fdba74}.hp-ready{background:#14291e;color:#86efac}
.hermes-meta{display:flex;gap:5px;flex-wrap:wrap;padding:7px 14px;border-bottom:1px solid var(--border);background:#f8f9fa;flex-shrink:0}
.hm-stat{font-size:10px;padding:2px 7px;background:#f1f5f9;border-radius:4px;color:#475569;white-space:nowrap}
.hm-stat b{color:#1e293b}
.hermes-tools-wrap{padding:6px 14px 5px;border-bottom:1px solid var(--border);flex-shrink:0}
.htool-lbl{font-size:9px;font-weight:bold;letter-spacing:1.5px;color:#94a3b8;margin-bottom:4px}
.tool-chip{display:inline-flex;align-items:center;gap:3px;font-size:10px;padding:2px 7px;background:#f1f5f9;border-radius:12px;margin:2px;color:#475569;border:1px solid #e2e8f0}
.tool-chip.ok{background:#dcfce7;border-color:#86efac;color:#15803d}
.tool-chip.err{background:#fee2e2;border-color:#fca5a5;color:#991b1b}
.sug-item{padding:5px 0;border-bottom:1px solid #f3f4f6;font-size:11px;color:#334155;display:flex;gap:7px;align-items:flex-start;line-height:1.5}
.sug-icon{color:#6366f1;flex-shrink:0;font-size:13px;margin-top:1px}
.hermes-phdr{display:flex;align-items:center;justify-content:space-between;padding:10px 14px;border-bottom:1px solid var(--border);flex-shrink:0;background:#fafaf7}
.hermes-phdr-title{font-size:9px;font-weight:bold;letter-spacing:2px;color:#7b1fa2;text-transform:uppercase}
/* ── Toast notification ── */
#_toast{position:fixed;bottom:22px;left:50%;transform:translateX(-50%);background:#1e293b;color:#fca5a5;font-size:12px;font-family:'Courier New',monospace;padding:9px 22px;border-radius:8px;border:1px solid #f87171;z-index:9999;max-width:92vw;text-align:center;box-shadow:0 4px 28px rgba(0,0,0,.55);opacity:0;transition:opacity .35s;pointer-events:none;white-space:nowrap}
/* Sidebar gen list */
.sb-gen-item{display:flex;align-items:center;gap:6px;padding:5px 8px;border-radius:6px;cursor:pointer;font-size:11px;color:#94a3b8;transition:background .15s}
.sb-gen-item:hover{background:#1e293b;color:#e2e8f0}
.sb-gen-dot{width:6px;height:6px;border-radius:50%;background:#00ff88;flex-shrink:0;margin-top:1px}
.sb-gen-name{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
/* Preview panel */
#vPreview{gap:10px}
#pvFrame{border:none;background:#fff}
.pv-info{width:260px;flex-shrink:0;display:flex;flex-direction:column;gap:10px;overflow-y:auto}
.pv-iframe-wrap{flex:1;display:flex;flex-direction:column;overflow:hidden}


/* ── Main ── */
#main{flex:1;display:flex;flex-direction:column;padding:12px;gap:10px;overflow:hidden;min-width:0}
.view{display:none;flex-direction:column;flex:1;overflow:hidden}
.view.active{display:flex}

/* ── Dashboard ── */
#cards{display:flex;gap:10px;flex-shrink:0}
.card{flex:1;background:var(--white);border-radius:10px;border:1px solid var(--border);padding:12px 16px;min-width:0;box-shadow:var(--sh)}
.clbl{font-size:10px;font-weight:bold;letter-spacing:1.5px;margin-bottom:8px}
.cpu .clbl{color:#1565c0}.ram .clbl{color:#2e7d32}.grd .clbl{color:#6a1b9a}.sts .clbl{color:#e65100}
.bar{width:100%;height:6px;background:#eee;border-radius:3px;overflow:hidden;margin-bottom:5px}
.fill{height:100%;border-radius:3px;transition:width .6s ease,background .6s ease}
.msub{font-size:11px;color:#777}
.gval{font-size:14px;font-weight:bold;margin-top:4px}
.gok{color:#2e7d32}.gwarn{color:#e65100}.gcrit{color:#c62828}
.sval{font-size:14px;font-weight:bold;color:#333;margin-top:4px}
#panels{display:flex;gap:10px;flex:1;overflow:hidden;min-height:0}
.panel{background:var(--white);border-radius:10px;border:1px solid var(--border);display:flex;flex-direction:column;overflow:hidden;box-shadow:var(--sh)}
.pl{flex:38}.pr{flex:62}
.phdr{font-size:9px;font-weight:bold;letter-spacing:2px;padding:10px 14px;border-bottom:1px solid var(--border);flex-shrink:0;background:#fafaf7;text-transform:uppercase}
.pl .phdr{color:#7b1fa2}.pr .phdr{color:#1565c0}
.pbody{flex:1;overflow-y:auto;padding:10px 14px;font-size:11px;line-height:1.85}
.ti{color:#8e24aa;border-bottom:1px solid #f3e5f5;padding:2px 0;font-size:11px}
.ln{color:#4a4a5a}.lc{color:#0097a7;font-weight:bold}.lo{color:#2e7d32;font-weight:bold}.lw{color:#e65100}.le{color:#c62828;font-weight:bold}
#cRow{display:flex;align-items:center;gap:8px;background:var(--white);border:2px solid #f0c000;border-radius:12px;padding:10px 16px;flex-shrink:0;box-shadow:0 2px 18px rgba(240,192,0,.12)}
#cLbl{font-size:9px;font-weight:bold;letter-spacing:2px;color:#c67c00;white-space:nowrap;text-transform:uppercase}
#cInput{flex:1;border:none;outline:none;font-family:inherit;font-size:13px;background:transparent;color:#1e1e2e;min-width:0;caret-color:#f0c000}
#cRun{padding:7px 20px;background:var(--yel);border:none;border-radius:7px;font-family:inherit;font-size:12px;font-weight:bold;cursor:pointer;flex-shrink:0;letter-spacing:.3px}
#cRun:hover{background:#fdd835;box-shadow:0 2px 8px rgba(0,0,0,.12)}#cRun:disabled{opacity:.45;cursor:not-allowed}
#cClr{padding:7px 11px;background:transparent;border:1px solid #e0dfd8;border-radius:7px;font-family:inherit;font-size:13px;color:#94a3b8;cursor:pointer;flex-shrink:0;line-height:1;transition:all .15s}
#cClr:hover{background:#f1f0eb;color:#64748b;border-color:#cbd5e1}

/* ── Shared card/form ── */
.sc{background:var(--white);border-radius:10px;border:1px solid var(--border);padding:14px 16px;box-shadow:var(--sh)}
.sc-t{font-size:9px;font-weight:bold;letter-spacing:2px;color:#64748b;margin-bottom:10px;text-transform:uppercase}
.frow{display:flex;align-items:center;gap:10px;margin-bottom:7px}
.frow label{font-size:11px;color:#64748b;width:115px;flex-shrink:0}
.fi{flex:1;padding:6px 10px;border:1px solid #e2e0d8;border-radius:6px;font-family:inherit;font-size:12px;outline:none;background:#fafaf7;transition:border-color .15s}
.fi:focus{border-color:#6366f1;background:#fff;box-shadow:0 0 0 2px rgba(99,102,241,.08)}
.sbtn{padding:6px 14px;border:none;border-radius:6px;font-family:inherit;font-size:12px;font-weight:bold;cursor:pointer;transition:filter .15s,box-shadow .15s}
.sbtn:hover{filter:brightness(0.92);box-shadow:0 2px 8px rgba(0,0,0,.1)}
.s-blue{background:#2196f3;color:#fff}.s-green{background:#4caf50;color:#fff}
.s-teal{background:#00897b;color:#fff}.s-slate{background:#78909c;color:#fff}
.s-red{background:#e53935;color:#fff}
.chk{display:flex;align-items:center;gap:8px;margin-bottom:7px;font-size:12px;color:#444;cursor:pointer}
.chk input{width:14px;height:14px;cursor:pointer}
.fmsg{font-size:11px;margin-top:4px;text-align:right;min-height:14px}
.fmsg.ok{color:#2e7d32}.fmsg.err{color:#c62828}

/* ── Settings view ── */
#vSettings{gap:10px;overflow-y:auto;padding-right:2px}

/* ── DB view ── */
#vDb{gap:10px}
#dbMain{display:flex;gap:10px;flex:1;overflow:hidden;min-height:0}
#dbLeft{width:185px;flex-shrink:0;display:flex;flex-direction:column;overflow:hidden}
#dbRight{flex:1;display:flex;flex-direction:column;gap:10px;overflow:hidden}
#tableList{flex:1;overflow-y:auto;font-size:12px;margin-top:6px}
.tbl-btn{display:block;width:100%;text-align:left;padding:4px 6px;background:none;border:none;cursor:pointer;font-family:inherit;font-size:12px;border-radius:3px}
.tbl-btn:hover{background:#f0f0f0}
#schemaWrap{max-height:120px;overflow-y:auto;font-size:11px;margin-top:6px}
.schema-row{padding:2px 0;border-bottom:1px solid #f5f5f5;color:#444}
.schema-pk{color:#1565c0;font-weight:bold}
#queryWrap{flex:1;display:flex;flex-direction:column;overflow:hidden}
#sqlQuery{flex:0 0 68px;font-family:inherit;font-size:12px;border:1px solid #ddd;border-radius:3px;padding:6px;resize:none;outline:none}
#sqlQuery:focus{border-color:#00897b}
#resultsWrap{flex:1;overflow:auto;margin-top:8px}
#resultTable{width:100%;border-collapse:collapse;font-size:11px}
#resultTable th{background:#f5f5f5;border:1px solid #ddd;padding:4px 8px;text-align:left;white-space:nowrap;position:sticky;top:0}
#resultTable td{border:1px solid #eee;padding:3px 8px;max-width:260px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}

/* ── Conn manager ── */
#connMgr{flex-shrink:0}
#connForm{display:flex;gap:6px;flex-wrap:wrap;align-items:flex-end;margin-bottom:8px}
.connField{display:flex;flex-direction:column;gap:3px;font-size:10px;color:#888}
.connField input,.connField select{padding:4px 6px;border:1px solid #ddd;border-radius:3px;font-family:inherit;font-size:12px;outline:none}
.connField input:focus,.connField select:focus{border-color:#2196f3}
#connList{max-height:90px;overflow-y:auto}
.conn-row{display:flex;align-items:center;gap:6px;padding:3px 0;border-bottom:1px solid #f0f0f0;font-size:11px}
.conn-tag{font-size:10px;padding:1px 5px;border-radius:10px;background:#e3f2fd;color:#1565c0;margin-left:4px}

/* ── Projects view ── */
#vProjects{gap:10px;overflow-y:auto}
.proj-row{display:flex;align-items:center;gap:8px;padding:7px 0;border-bottom:1px solid #f0f0f0}
.proj-info{flex:1;min-width:0}
.proj-name{font-size:13px;font-weight:bold}
.proj-path{font-size:11px;color:#888;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.proj-active{background:#e8f5e9;border-radius:10px;font-size:10px;color:#2e7d32;padding:1px 7px;flex-shrink:0}

/* ── Generated Projects ── */
#vGenerated{gap:10px}
.gen-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));gap:14px;padding:4px}
.gen-card{background:var(--white);border:1px solid var(--border);border-radius:12px;padding:16px 18px;box-shadow:var(--sh);transition:box-shadow .2s,transform .2s}
.gen-card:hover{box-shadow:0 8px 28px rgba(0,0,0,.1);transform:translateY(-2px)}
.gen-card-top{display:flex;align-items:flex-start;justify-content:space-between;gap:8px;margin-bottom:6px}
.gen-card-name{font-size:14px;font-weight:bold;color:#1e293b;flex:1;word-break:break-word}
.gen-type-badge{font-size:10px;padding:2px 8px;border-radius:10px;background:#e0e7ff;color:#3730a3;white-space:nowrap;flex-shrink:0}
.gen-path{font-size:10px;color:#94a3b8;margin-bottom:6px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.gen-files{font-size:11px;color:#64748b;margin-bottom:10px}
.gen-status{color:#16a34a;font-weight:bold}
.gen-actions{display:flex;gap:6px;flex-wrap:wrap}
.act-btn{padding:5px 12px;border:none;border-radius:20px;font-family:inherit;font-size:11px;font-weight:600;cursor:pointer;transition:all .15s;letter-spacing:.3px;line-height:1.2}
.act-btn:hover{filter:brightness(1.1);transform:translateY(-1px);box-shadow:0 3px 10px rgba(0,0,0,.18)}
.act-preview{background:#6366f1;color:#fff}.act-download{background:#0891b2;color:#fff}
.act-copy{background:#64748b;color:#fff}.act-readme{background:#059669;color:#fff}
.act-improve{background:#7c3aed;color:#fff}.act-feature{background:#0f766e;color:#fff}
/* README modal */
.readme-modal{display:none;position:fixed;inset:0;background:rgba(0,0,0,.5);z-index:500;align-items:center;justify-content:center}
.readme-box{background:#fff;border-radius:8px;width:min(700px,90vw);max-height:80vh;display:flex;flex-direction:column;overflow:hidden}
.readme-hdr{display:flex;align-items:center;justify-content:space-between;padding:12px 16px;border-bottom:1px solid #e2e8f0;font-size:14px}
.readme-hdr button{background:none;border:none;font-size:18px;cursor:pointer;color:#94a3b8}
.readme-pre{flex:1;overflow:auto;padding:16px;font-size:12px;line-height:1.6;white-space:pre-wrap;font-family:'Courier New',monospace;color:#1e293b}
</style>
</head>
<body>

<div id="sb">
  <div class="brand">◈ SYNAPSE</div>
  <div class="hermes-badge">⚡ BUILT WITH HERMES AGENT</div>
  <div class="profbar">◉ <b id="profName">Guest</b></div>
  <div id="sd" class="idle">● Idle</div>

  <div class="slbl">COMMANDS</div>
  <button class="cb bc" onclick="runCmd('map project')">◆ Map Project</button>
  <button class="cb bg" onclick="runCmd('test sandbox')">◆ Test Sandbox</button>
  <button class="cb bm" onclick="runCmd('ask models')">◆ Ask Models</button>
  <button class="cb by" onclick="runHermes()">◆ Run Agent</button>

  <div class="slbl">VIEWS</div>
  <button class="cb bb"  onclick="setView('vDash')">◆ Dashboard</button>
  <button class="cb bt"  onclick="setView('vSettings')">⚙ Settings</button>
  <button class="cb bsl" onclick="setView('vDb')">◈ Database</button>
  <button class="cb bv"  onclick="setView('vProjects')">⊞ Workspace</button>
  <button class="cb bgo" onclick="setView('vGenerated')">◆ Generated</button>

  <div class="slbl">GENERATED</div>
  <div id="sbGenList" style="max-height:160px;overflow-y:auto"></div>

  <div class="spacer"></div>
  <div class="sb-proj">Project: <span id="sbProjName">none</span></div>
</div>

<div id="main">

<!-- ── Dashboard ── -->
<div id="vDash" class="view active">
  <div id="cards">
    <div class="card cpu">
      <div class="clbl">CPU USAGE</div>
      <div class="bar"><div class="fill" id="cpuFill" style="width:0%;background:#42a5f5"></div></div>
      <div class="msub" id="cpuLbl">0.0%</div>
    </div>
    <div class="card ram">
      <div class="clbl">RAM USAGE</div>
      <div class="bar"><div class="fill" id="ramFill" style="width:0%;background:#66bb6a"></div></div>
      <div class="msub" id="ramLbl">0.0%</div>
    </div>
    <div class="card grd">
      <div class="clbl">MEMORY GUARD</div>
      <div class="gval gok" id="guardVal">● NOMINAL</div>
    </div>
    <div class="card sts">
      <div class="clbl">STATUS</div>
      <div class="sval" id="statVal">Idle</div>
    </div>
  </div>
  <div id="panels">
    <div class="panel pl" style="min-height:0;display:flex;flex-direction:column">
      <div class="hermes-phdr">
        <span class="hermes-phdr-title">HERMES ARCHITECT</span>
        <span id="hermesStatus" class="hermes-pill hp-idle">● IDLE</span>
      </div>
      <div id="hermesMeta" class="hermes-meta" style="display:none"></div>
      <div id="hermesToolsWrap" class="hermes-tools-wrap" style="display:none">
        <div class="htool-lbl">TOOL CALLS</div>
        <div id="hermesTools"></div>
      </div>
      <div class="pbody" id="tLog" style="flex:1;overflow-y:auto">
        <div style="color:#94a3b8;font-size:11px;padding:4px 0;line-height:1.7">
          Click <b style="color:#7b1fa2">◆ Run Agent</b> to launch Hermes Architect.<br>
          The agent will scan your project, read source files,<br>
          and return engineering analysis &amp; improvement suggestions.
        </div>
      </div>
    </div>
    <div class="panel pr">
      <div class="phdr">EXECUTION LOG</div>
      <div class="pbody" id="eLog">
        <div class="ln">Synapse-Overlord ready.</div>
        <div class="ln">Use commands or type below.</div>
      </div>
    </div>
  </div>
  <div id="cRow">
    <span id="cLbl">COMMAND AGENCY ›</span>
    <input id="cInput" type="text" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false" placeholder="build project <idea>  |  improve project <slug> <instruction>  |  map project"/>
    <button id="cRun" onclick="submit()">▶ Run</button>
    <button id="cClr" onclick="g('cInput').value='';g('cInput').focus()" title="Clear">✕</button>
  </div>
</div>

<!-- ── Settings ── -->
<div id="vSettings" class="view">

  <div class="sc">
    <div class="sc-t">Profile</div>
    <div class="frow"><label>Username</label><input id="sUsername" class="fi" type="text" placeholder="your-handle"></div>
    <div class="frow"><label>Display Name</label><input id="sDisplayName" class="fi" type="text" placeholder="Your Name"></div>
    <div class="frow" style="justify-content:flex-end;margin-bottom:0">
      <button class="sbtn s-blue" onclick="saveProfile()">Save Profile</button>
    </div>
    <div class="fmsg" id="profileMsg"></div>
  </div>

  <div class="sc">
    <div class="sc-t">AI Model Settings</div>
    <div class="frow"><label>AI Mode</label>
      <select id="sAiMode" class="fi">
        <option value="cloud">Cloud (Groq API)</option>
        <option value="offline">Offline</option>
      </select>
    </div>
    <div class="frow"><label>Logic Model</label><input id="sLogic" class="fi" type="text"></div>
    <div class="frow"><label>Audit Model</label><input id="sAudit" class="fi" type="text"></div>
    <div class="frow"><label>Optimize Model</label><input id="sOptimize" class="fi" type="text"></div>
    <div class="frow" style="justify-content:flex-end;margin-bottom:0">
      <button class="sbtn s-blue" onclick="saveSettings()">Save Model Settings</button>
    </div>
    <div class="fmsg" id="settingsMsg"></div>
  </div>

  <div class="sc">
    <div class="sc-t">API Key (.env)</div>
    <div style="font-size:11px;color:#888;margin-bottom:8px">
      GROQ key status: <span id="keyStatus" style="font-weight:bold">–</span>
    </div>
    <div class="frow"><label>GROQ API Key</label><input id="sApiKey" class="fi" type="password" placeholder="gsk_…"></div>
    <div class="frow"><label>Logic Model</label><input id="eLogic" class="fi" type="text" placeholder="optional env override"></div>
    <div class="frow"><label>Audit Model</label><input id="eAudit" class="fi" type="text" placeholder="optional env override"></div>
    <div class="frow"><label>Optimize Model</label><input id="eOptimize" class="fi" type="text" placeholder="optional env override"></div>
    <div class="frow" style="justify-content:flex-end;margin-bottom:0">
      <button class="sbtn s-green" onclick="saveApiKeys()">Save to .env</button>
    </div>
    <div class="fmsg" id="apiKeyMsg"></div>
  </div>

  <div class="sc">
    <div class="sc-t">IDE &amp; Workspace</div>
    <div class="frow"><label>IDE</label>
      <select id="sIde" class="fi">
        <option value="none">None</option>
        <option value="vscode">VS Code</option>
        <option value="pycharm">PyCharm</option>
        <option value="webstorm">WebStorm</option>
        <option value="cursor">Cursor</option>
        <option value="windsurf">Windsurf</option>
        <option value="codespaces">GitHub Codespaces</option>
      </select>
    </div>
    <div class="frow"><label>Workspace Path</label><input id="sIdePath" class="fi" type="text" placeholder="/absolute/path/to/workspace"></div>
    <div class="frow" style="justify-content:flex-end;margin-bottom:0">
      <button class="sbtn s-blue" onclick="saveIde()">Save IDE Config</button>
    </div>
    <div class="fmsg" id="ideMsg"></div>
  </div>

  <div class="sc">
    <div class="sc-t">Safety</div>
    <label class="chk"><input type="checkbox" id="sSafetyBlock"> Block Destructive Commands (DROP, DELETE, rm -rf …)</label>
    <label class="chk"><input type="checkbox" id="sSafetyApproval"> Require Approval for Risky Commands</label>
    <div class="frow" style="justify-content:flex-end;margin-top:4px;margin-bottom:0">
      <button class="sbtn s-blue" onclick="saveSettings()">Save Safety Settings</button>
    </div>
  </div>

</div>

<!-- ── Database ── -->
<div id="vDb" class="view">

  <div class="sc" id="connMgr">
    <div class="sc-t">Connection Manager</div>
    <div id="connForm">
      <div class="connField">Type
        <select id="connType" onchange="onConnTypeChange()" style="width:130px">
          <option value="sqlite">SQLite</option>
          <option value="postgresql">PostgreSQL</option>
          <option value="mysql">MySQL</option>
          <option value="mongodb">MongoDB</option>
          <option value="supabase">Supabase</option>
          <option value="neon">Neon</option>
          <option value="planetscale">PlanetScale</option>
          <option value="firebase">Firebase</option>
        </select>
      </div>
      <div class="connField">Name<input id="connName" placeholder="My DB" style="width:120px"></div>
      <div class="connField" id="connPathField">SQLite Path<input id="connPath" placeholder="/path/to/db.sqlite" style="width:200px"></div>
      <div class="connField" id="connHostField" style="display:none">Host<input id="connHost" placeholder="localhost" style="width:140px"></div>
      <div class="connField" id="connDbField" style="display:none">Database<input id="connDb" placeholder="dbname" style="width:120px"></div>
      <div class="connField" id="connUserField" style="display:none">User<input id="connUser" placeholder="user" style="width:100px"></div>
      <div class="connField" style="justify-content:flex-end">
        <div style="display:flex;gap:6px;margin-top:14px">
          <button class="sbtn s-teal" onclick="testConn()">Test</button>
          <button class="sbtn s-blue" onclick="saveConn()">Save</button>
        </div>
      </div>
    </div>
    <div id="connTestMsg" style="font-size:11px;color:#888;margin-bottom:6px"></div>
    <div id="connList"></div>
  </div>

  <div class="sc" style="flex-shrink:0">
    <div class="sc-t">SQLite Explorer</div>
    <div style="display:flex;gap:8px;align-items:center">
      <input id="dbPath" class="fi" type="text" placeholder="/path/to/database.sqlite" style="flex:1">
      <button class="sbtn s-teal" onclick="dbTest()">Test</button>
      <button class="sbtn s-blue" onclick="dbLoadTables()">Load Tables</button>
    </div>
    <div id="dbStatus" style="font-size:11px;color:#888;margin-top:6px">Not Connected</div>
  </div>

  <div id="dbMain">
    <div id="dbLeft" class="sc">
      <div class="sc-t" style="margin-bottom:2px">Tables</div>
      <div id="tableList"><div style="color:#aaa;font-size:11px">Load tables to begin</div></div>
    </div>
    <div id="dbRight">
      <div class="sc" style="flex-shrink:0">
        <div class="sc-t" style="margin-bottom:4px">
          Schema <span id="schemaTableName" style="color:#888;font-weight:normal;font-size:11px"></span>
        </div>
        <div id="schemaWrap"><div style="color:#aaa;font-size:11px">Select a table</div></div>
      </div>
      <div class="sc" id="queryWrap">
        <div class="sc-t" style="margin-bottom:6px">SELECT Query</div>
        <textarea id="sqlQuery" placeholder="SELECT * FROM your_table LIMIT 50"></textarea>
        <div style="display:flex;justify-content:space-between;align-items:center;margin-top:6px">
          <div id="queryErr" style="font-size:11px;color:#c62828"></div>
          <button class="sbtn s-teal" onclick="dbQuery()">▶ Run SELECT</button>
        </div>
        <div id="resultsWrap"><table id="resultTable"></table></div>
        <div id="rowCount" style="font-size:11px;color:#888;margin-top:4px"></div>
      </div>
    </div>
  </div>
</div>

<!-- ── Projects / Workspace ── -->
<div id="vProjects" class="view">

  <div class="sc" style="flex-shrink:0">
    <div class="sc-t">Active Project</div>
    <div id="activeProjDisplay" style="font-size:13px;color:#333;padding:4px 0">No project selected</div>
  </div>

  <div class="sc" style="flex-shrink:0">
    <div class="sc-t">Add Project</div>
    <div class="frow"><label>Name</label><input id="projName" class="fi" type="text" placeholder="My Project"></div>
    <div class="frow"><label>Path</label><input id="projPath" class="fi" type="text" placeholder="/absolute/path/to/project"></div>
    <div class="frow" style="justify-content:flex-end;margin-bottom:0">
      <button class="sbtn s-blue" onclick="addProject()">Add Project</button>
    </div>
    <div class="fmsg" id="projMsg"></div>
  </div>

  <div class="sc" style="flex:1;display:flex;flex-direction:column;overflow:hidden">
    <div class="sc-t" style="margin-bottom:6px">Saved Projects</div>
    <div id="projectsList" style="flex:1;overflow-y:auto"></div>
  </div>

</div>

<!-- ── Generated Projects ── -->
<div id="vGenerated" class="view">
  <div class="sc" style="flex-shrink:0;padding:10px 16px">
    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px">
      <div class="sc-t" style="margin-bottom:0">Generated Projects</div>
      <button class="sbtn s-teal" onclick="loadGeneratedProjects()">↻ Refresh</button>
    </div>
    <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap">
      <input id="genSearch" class="fi" type="text" placeholder="Search projects..." style="flex:1;min-width:100px" oninput="filterGenProjects()">
      <select id="genTypeFilter" class="fi" style="width:auto" onchange="filterGenProjects()">
        <option value="">All Types</option>
        <option value="medical/shop">Medical</option>
        <option value="portfolio">Portfolio</option>
        <option value="ecommerce">Ecommerce</option>
        <option value="restaurant/menu">Restaurant</option>
        <option value="student-dashboard">Student</option>
        <option value="quiz-app">Quiz</option>
        <option value="notes/todo">Notes/Todo</option>
        <option value="ai-chatbot">AI Chatbot</option>
        <option value="admin-dashboard">Admin</option>
        <option value="file-tracker">File Tracker</option>
        <option value="expense-tracker">Expense</option>
        <option value="landing-page">Landing Page</option>
      </select>
    </div>
  </div>
  <div id="genGrid" class="gen-grid" style="flex:1;overflow-y:auto"></div>
</div>

<!-- ── Project Preview ── -->
<div id="vPreview" class="view">
  <div style="display:flex;gap:10px;flex:1;overflow:hidden;min-height:0">
    <div class="pv-info">
      <div class="sc">
        <div class="sc-t">PROJECT</div>
        <div id="pvName" style="font-size:15px;font-weight:bold;color:#1e293b;margin-bottom:8px;line-height:1.3"></div>
        <div id="pvBadge" style="margin-bottom:6px"></div>
        <div id="pvPath" style="font-size:10px;color:#94a3b8;word-break:break-all;margin-bottom:4px"></div>
        <div id="pvFiles" style="font-size:11px;color:#64748b"></div>
      </div>
      <div class="sc">
        <div class="sc-t">ACTIONS</div>
        <div style="display:flex;flex-direction:column;gap:6px">
          <button class="act-btn act-preview"  style="border-radius:7px;padding:9px 12px;text-align:left" onclick="pvFullPage()">↗ Preview Full Page</button>
          <button class="act-btn act-download" style="border-radius:7px;padding:9px 12px;text-align:left" onclick="pvDownload()">⬇ Download ZIP</button>
          <button class="act-btn act-readme"   style="border-radius:7px;padding:9px 12px;text-align:left" onclick="pvReadme()">📄 View README</button>
          <button class="act-btn act-copy"     style="border-radius:7px;padding:9px 12px;text-align:left" onclick="pvCopy()">⎘ Copy Path</button>
          <button class="act-btn act-improve"  style="border-radius:7px;padding:9px 12px;text-align:left" onclick="pvImprove()">✦ Improve Project</button>
          <button class="act-btn act-feature"  style="border-radius:7px;padding:9px 12px;text-align:left" onclick="pvFeature()">✦ Add Feature</button>
        </div>
      </div>
      <button class="sbtn s-slate" style="width:100%;padding:9px" onclick="setView('vGenerated')">← All Projects</button>
    </div>
    <div class="sc pv-iframe-wrap" style="padding:0">
      <div style="display:flex;align-items:center;justify-content:space-between;padding:10px 14px;border-bottom:1px solid var(--border);flex-shrink:0">
        <div class="sc-t" style="margin-bottom:0">LIVE PREVIEW</div>
        <button class="sbtn s-slate" style="padding:4px 10px;font-size:11px" onclick="g('pvFrame').src=g('pvFrame').src">↻ Reload</button>
      </div>
      <iframe id="pvFrame" style="flex:1;border:none;background:#fff;width:100%;height:100%" sandbox="allow-scripts allow-same-origin allow-forms"></iframe>
    </div>
  </div>
</div>

</div><!-- #main -->

<div id="readmeModal" class="readme-modal" onclick="if(event.target===this)closeReadme()">
  <div class="readme-box">
    <div class="readme-hdr"><b>README</b><button onclick="closeReadme()">✕</button></div>
    <pre id="readmeContent" class="readme-pre"></pre>
  </div>
</div>

<div id="enhModal" class="readme-modal" onclick="if(event.target===this)closeEnhance()">
  <div class="readme-box" style="max-width:500px">
    <div class="readme-hdr"><b id="enhTitle">Improve Project</b><button onclick="closeEnhance()">✕</button></div>
    <div style="padding:16px">
      <input id="enhInput" class="fi" type="text" placeholder="Describe the enhancement..." style="width:100%;margin-bottom:12px">
      <div style="font-size:11px;color:#888;margin-bottom:10px">
        Examples: <em>add dark mode toggle</em> · <em>premium card styling</em> · <em>contact form behavior</em> · <em>cart with localStorage</em>
      </div>
      <div style="display:flex;justify-content:flex-end;gap:8px">
        <button class="sbtn s-slate" onclick="closeEnhance()">Cancel</button>
        <button class="sbtn s-blue" onclick="runEnhance()">▶ Run Enhancement</button>
      </div>
      <div id="enhMsg" style="font-size:11px;margin-top:8px;color:#888;min-height:14px"></div>
    </div>
  </div>
</div>

<script>
const g = id => document.getElementById(id);
const VIEWS = ['vDash','vSettings','vDb','vProjects','vGenerated','vPreview'];

// ── Network helpers ────────────────────────────────────────────────────────────
function showToast(msg){
  let t=g('_toast');
  if(!t){
    t=document.createElement('div');t.id='_toast';
    t.style.cssText='position:fixed;bottom:22px;left:50%;transform:translateX(-50%);background:#1e293b;color:#fca5a5;font-size:12px;font-family:\'Courier New\',monospace;padding:9px 22px;border-radius:8px;border:1px solid #f87171;z-index:9999;max-width:92vw;text-align:center;box-shadow:0 4px 28px rgba(0,0,0,.55);opacity:0;transition:opacity .35s;pointer-events:none';
    document.body.appendChild(t);
  }
  t.textContent=msg; t.style.opacity='1';
  clearTimeout(t._tid); t._tid=setTimeout(()=>{t.style.opacity='0';},4500);
}
async function safeFetch(url,opts={}){
  try{
    const res=await fetch(url,opts);
    if(!res.ok){
      const txt=await res.text().catch(()=>'');
      throw new Error('HTTP '+res.status+(txt?': '+txt.slice(0,120):''));
    }
    return await res.json();
  }catch(e){
    const raw=String(e);
    const msg=raw.includes('Failed to fetch')
      ?'⚠ Cannot reach server — is Synapse-Overlord running on localhost:3000?'
      :'⚠ Request failed: '+raw.replace('TypeError: ','').replace('Error: ','').slice(0,100);
    showToast(msg);
    return null;
  }
}

// ── View switching ─────────────────────────────────────────────────────────────
async function setView(id) {
  VIEWS.forEach(v => {
    const el = g(v);
    const on = v === id;
    el.classList.toggle('active', on);
  });
  if (id === 'vSettings')  loadSettings();
  if (id === 'vProjects')  loadProjects();
  if (id === 'vDb')        loadConnections();
  if (id === 'vGenerated') loadGeneratedProjects();
}

// ── Health polling ─────────────────────────────────────────────────────────────
function barClr(p, cpu) {
  return p>=90?'#ef5350':p>=(cpu?60:75)?'#ffa726':(cpu?'#42a5f5':'#66bb6a');
}
async function pollHealth() {
  try {
    const d = await fetch('/api/health').then(r=>r.json());
    const cpu=+(d.cpu_percent||0), ram=+(d.ram_percent||0);
    g('cpuFill').style.cssText=`width:${cpu}%;background:${barClr(cpu,true)}`;
    g('cpuLbl').textContent=cpu.toFixed(1)+'%';
    g('ramFill').style.cssText=`width:${ram}%;background:${barClr(ram,false)}`;
    g('ramLbl').textContent=ram.toFixed(1)+'%  '+(d.ram_used_mb||0)+' / '+(d.ram_total_mb||0)+' MB';
    const gv=g('guardVal');
    if(ram>=90){gv.textContent='■ CRITICAL';gv.className='gval gcrit';}
    else if(ram>=75){gv.textContent='▲ WARNING';gv.className='gval gwarn';}
    else{gv.textContent='● NOMINAL';gv.className='gval gok';}
    g('statVal').textContent=d.status||'Idle';
    // profile + project in sidebar
    const uname=d.profile_username;
    g('profName').textContent=uname||'Guest';
    const proj=d.active_project;
    const pname=proj?proj.split(/[\\/]/).pop():'none';
    g('sbProjName').textContent=pname;
    g('sbProjName').style.color=proj?'#00ff88':'#555';
  }catch(_){}
}
setInterval(pollHealth,1000); pollHealth();

// ── Log helpers ────────────────────────────────────────────────────────────────
function logCls(s){
  if(s.includes('[CRITICAL]')||s.includes('ERROR'))return 'le';
  if(s.includes('WARN')||s.includes('FAIL')||s.includes('[!]'))return 'lw';
  if(s.startsWith('> '))return 'lc';
  if(s.includes('SUCCEED')||s.includes(': OK')||s.includes('PASS'))return 'lo';
  return 'ln';
}
function addTo(id,text,cls){
  const p=g(id),d=document.createElement('div');
  d.className=cls; d.textContent=text; p.appendChild(d); p.scrollTop=p.scrollHeight;
}
function showMsg(id,text,isOk){
  const el=g(id); el.textContent=text; el.className='fmsg '+(isOk?'ok':'err');
  setTimeout(()=>{ el.textContent=''; }, 3000);
}

// ── Busy ──────────────────────────────────────────────────────────────────────
function setBusy(on){
  document.querySelectorAll('.cb,#cRun,#cClr').forEach(b=>{b.disabled=on;});
  const sd=g('sd'); sd.textContent=on?'◌ Running':'● Idle'; sd.className=on?'running':'idle';
}

// ── Hermes Architect ───────────────────────────────────────────────────────────
function setHermesStatus(state,label){
  const el=g('hermesStatus'); if(!el)return;
  const cls={idle:'hp-idle',scan:'hp-scan',read:'hp-read',analyze:'hp-analyze',ready:'hp-ready'}[state]||'hp-idle';
  const icon={idle:'●',scan:'◌',read:'◌',analyze:'◌',ready:'✓'}[state]||'●';
  el.className='hermes-pill '+cls;
  el.textContent=icon+' '+(label||state.toUpperCase());
}

async function runHermes(){
  const tlog=g('tLog'), meta=g('hermesMeta'), tw=g('hermesToolsWrap'), tools=g('hermesTools');
  // Reset panel
  if(tlog)tlog.innerHTML='';
  if(meta)meta.style.display='none';
  if(tw)tw.style.display='none';

  setBusy(true);
  setHermesStatus('scan','SCANNING PROJECT');
  if(tlog){
    const d=document.createElement('div');
    d.style.cssText='color:#60a5fa;font-size:11px;padding:4px 0';
    d.textContent='◌ Scanning project files…';
    tlog.appendChild(d);
  }
  addTo('eLog','> [Hermes] Starting architect analysis…','lc');

  try{
    setHermesStatus('read','READING FILES');
    const r=await fetch('/api/architect/analyze',{
      method:'POST',
      headers:{'Content-Type':'application/json'},
      body:JSON.stringify({
        goal:'You are the Hermes Architect analyzing a Rust/Axum project called Synapse-Overlord — a local-first AI project builder. Use your tools step by step: 1) list_directory on src/ to discover all modules, 2) file_read src/main.rs to understand the entry point and routing, 3) file_read 2-3 key module files (e.g. src/routes/hermes.rs, src/web/mod.rs, src/builder/mod.rs or similar), 4) file_search for important patterns. Then produce a structured engineering report with: a 2-sentence project summary, a list of key modules and their roles, and exactly 5 concrete improvement suggestions each as a bullet line starting with "- ".',
        max_tool_calls:7
      })
    }).then(r=>r.json());

    setHermesStatus('analyze','ANALYZING');
    await new Promise(res=>setTimeout(res,200));

    if(tlog)tlog.innerHTML='';

    if(r.success||r.analysis){
      setHermesStatus('ready','READY');

      // ── Meta bar ────────────────────────────────────────────────────────────
      if(meta){
        meta.innerHTML=
          '<span class="hm-stat">📁 <b>'+(r.files_analyzed||r.files_mapped||0)+'</b> files</span>'+
          '<span class="hm-stat">🔧 <b>'+((r.tool_calls||[]).length)+'</b> tool calls</span>'+
          '<span class="hm-stat">⚡ <b>'+(r.timing_ms||r.duration_ms||0)+'ms</b></span>'+
          '<span class="hm-stat">🤖 <b>'+(r.backend||'?')+'</b></span>';
        meta.style.display='flex';
      }

      // ── Tool call chips ──────────────────────────────────────────────────────
      const tcArr=r.tool_calls||[];
      if(tw&&tools&&tcArr.length){
        tools.innerHTML=tcArr.map(tc=>'<span class="tool-chip '+(tc.success?'ok':'err')+'">'+(tc.success?'✓':'✗')+' '+tc.name+'</span>').join('');
        tw.style.display='block';
      }

      // ── Execution log ────────────────────────────────────────────────────────
      addTo('eLog','[Hermes] '+(r.files_analyzed||r.files_mapped||0)+' files · '+tcArr.length+' tool calls · '+(r.timing_ms||r.duration_ms||0)+'ms · '+(r.backend||'?')+'/'+(r.model||'?'),'lo');
      tcArr.forEach(tc=>{
        const p=(tc.result_preview||'').slice(0,80).replace(/\n/g,' ');
        addTo('eLog','  ['+(tc.success?'✓':'✗')+'] '+tc.name+(p?' → '+p:''),tc.success?'ln':'lw');
      });

      // ── Summary + Suggestions (top of panel) ────────────────────────────────
      const sugs=r.suggestions||[];
      if(sugs.length){
        const hdr=document.createElement('div');
        hdr.style.cssText='font-size:9px;font-weight:bold;letter-spacing:1.5px;color:#6366f1;margin:4px 0;text-transform:uppercase';
        hdr.textContent='◆ SUGGESTIONS';
        if(tlog)tlog.appendChild(hdr);
        sugs.forEach(s=>{
          const d=document.createElement('div');
          d.className='sug-item';
          d.innerHTML='<span class="sug-icon">›</span><span>'+s+'</span>';
          if(tlog)tlog.appendChild(d);
        });
      }

      // ── Full analysis text ───────────────────────────────────────────────────
      const analysis=r.analysis||'';
      if(analysis){
        const sep=document.createElement('div');
        sep.style.cssText='font-size:9px;font-weight:bold;letter-spacing:1.5px;color:#64748b;margin:10px 0 4px;text-transform:uppercase';
        sep.textContent='◆ FULL ANALYSIS';
        if(tlog)tlog.appendChild(sep);
        analysis.split('\n').forEach(line=>{
          const t=line.trim(); if(!t)return;
          const d=document.createElement('div');
          if(t.startsWith('-')||t.startsWith('•')||t.startsWith('*')){
            d.className='sug-item';
            d.innerHTML='<span class="sug-icon">›</span><span>'+t.replace(/^[-•*]\s*/,'')+'</span>';
          }else{
            d.style.cssText='font-size:11px;color:#475569;padding:2px 0;line-height:1.6';
            d.textContent=t;
          }
          if(tlog)tlog.appendChild(d);
        });
      }

      if(!sugs.length&&!analysis){
        addTo('tLog','Analysis complete. No output returned — check GROQ_API_KEY in .env','ln');
      }

    }else{
      setHermesStatus('idle','ERROR');
      addTo('eLog','[Hermes ERROR] '+(r.error||'Analysis failed'),'le');
      if(tlog){
        const d=document.createElement('div');
        d.style.cssText='color:#c62828;font-size:11px;padding:4px 0';
        d.textContent='❌ '+(r.error||'Analysis failed — check GROQ_API_KEY in .env');
        tlog.appendChild(d);
      }
    }
  }catch(e){
    setHermesStatus('idle','ERROR');
    addTo('eLog','[Hermes ERROR] '+e,'le');
    if(tlog){
      const d=document.createElement('div');
      d.style.cssText='color:#c62828;font-size:11px;padding:4px 0';
      d.textContent='❌ Connection error: '+e;
      tlog.appendChild(d);
    }
  }
  setBusy(false);
}

// ── Dashboard commands ─────────────────────────────────────────────────────────
async function runCmd(cmd){ g('cInput').value=cmd; await submit(); }
async function submit(){
  const inp=g('cInput'), cmd=inp.value.trim(); if(!cmd)return;
  inp.value=''; addTo('eLog','> '+cmd,'lc'); setBusy(true);
  const d=await safeFetch('/api/command',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({command:cmd})});
  if(d){
    (d.logs||[]).forEach(l=>addTo('eLog',l,logCls(l)));
    (d.thoughts||[]).forEach(t=>addTo('tLog',t,'ti'));
    if(cmd.toLowerCase().startsWith('build project'))setView('vGenerated');
  }else{
    addTo('eLog','[ERROR] Server unreachable — run: cargo run -- web','le');
  }
  setBusy(false);
}
g('cInput').addEventListener('keydown',e=>{ if(e.key==='Enter')submit(); });

// ── Settings ───────────────────────────────────────────────────────────────────
async function loadSettings(){
  const s=await safeFetch('/api/settings');
  if(!s){showToast('⚠ Could not load settings');return;}
  g('sUsername').value=s.profile_username||'';
  g('sDisplayName').value=s.profile_display_name||'';
  g('sAiMode').value=s.ai_mode||'cloud';
  g('sLogic').value=s.logic_model||'';
  g('sAudit').value=s.audit_model||'';
  g('sOptimize').value=s.optimize_model||'';
  g('sIde').value=s.ide_connector||'none';
  g('sIdePath').value=s.ide_workspace_path||'';
  g('sSafetyBlock').checked=s.safety_block_destructive!==false;
  g('sSafetyApproval').checked=s.safety_needs_approval!==false;
  if(s.db_sqlite_path) g('dbPath').value=s.db_sqlite_path;
  const ks=g('keyStatus');
  if(s.api_key_set){ks.textContent='● Set';ks.style.color='#2e7d32';}
  else{ks.textContent='○ Not Set';ks.style.color='#e65100';}
}

async function saveProfile(){
  const r=await safeFetch('/api/profile/save',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({username:g('sUsername').value,display_name:g('sDisplayName').value})});
  if(r)showMsg('profileMsg',r.saved?'✓ Saved':'✗ '+(r.error||'Error'),r.saved);
}

async function saveSettings(){
  const body={
    ai_mode:g('sAiMode').value, logic_model:g('sLogic').value,
    audit_model:g('sAudit').value, optimize_model:g('sOptimize').value,
    safety_block_destructive:g('sSafetyBlock').checked,
    safety_needs_approval:g('sSafetyApproval').checked,
    profile_username:g('sUsername').value, profile_display_name:g('sDisplayName').value,
    ide_connector:g('sIde').value, ide_workspace_path:g('sIdePath').value,
    api_key_set:false, db_sqlite_path:g('dbPath').value||'',
    projects:[], db_connections:[], active_project:'', active_db_id:'',
  };
  const r=await safeFetch('/api/settings',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  if(r)showMsg('settingsMsg',r.saved?'✓ Saved':'✗ '+(r.error||'Error'),r.saved);
}

async function saveApiKeys(){
  const body={groq_api_key:g('sApiKey').value,logic_model:g('eLogic').value||null,audit_model:g('eAudit').value||null,optimize_model:g('eOptimize').value||null};
  g('sApiKey').value='';
  const r=await safeFetch('/api/settings/api-keys',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  if(r){showMsg('apiKeyMsg',r.saved?'✓ Saved to .env':'✗ Save failed',r.saved);if(r.saved)loadSettings();}
}

async function saveIde(){
  const r=await safeFetch('/api/ide/save',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({ide_connector:g('sIde').value,ide_workspace_path:g('sIdePath').value})});
  if(r)showMsg('ideMsg',r.saved?'✓ Saved':'✗ '+(r.error||'Error'),r.saved);
}

// ── Projects ───────────────────────────────────────────────────────────────────
async function loadProjects(){
  const r=await safeFetch('/api/projects');
  if(!r){g('activeProjDisplay').textContent='Error loading projects';g('activeProjDisplay').style.color='#c62828';return;}
  g('activeProjDisplay').textContent=r.active_project||'No project selected';
  g('activeProjDisplay').style.color=r.active_project?'#2e7d32':'#888';
  const list=g('projectsList'); list.innerHTML='';
  (r.projects||[]).forEach(p=>{
    const row=document.createElement('div'); row.className='proj-row';
    const isActive=p.path===r.active_project;
    row.innerHTML=`<div class="proj-info"><div class="proj-name">${p.name}</div><div class="proj-path">${p.path}</div></div>${isActive?'<span class="proj-active">active</span>':''}<button class="sbtn s-blue" onclick="switchProj('${p.path}')">Use</button><button class="sbtn s-red" onclick="removeProj('${p.path}')">✕</button>`;
    list.appendChild(row);
  });
  if(!r.projects||!r.projects.length){
    list.innerHTML='<div style="color:#aaa;font-size:12px;padding:8px 0">No projects saved. Add a project above.</div>';
  }
}
async function addProject(){
  const name=g('projName').value.trim(), path=g('projPath').value.trim();
  if(!name||!path){showMsg('projMsg','✗ Name and path required',false);return;}
  const r=await safeFetch('/api/projects/add',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({name,path})});
  if(r){showMsg('projMsg',r.saved?'✓ Project added':'✗ '+(r.error||'Error'),r.saved);if(r.saved){g('projName').value='';g('projPath').value='';loadProjects();}}
}
async function switchProj(path){
  const r=await safeFetch('/api/projects/switch',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path})});
  if(r)loadProjects();
}
async function removeProj(path){
  const r=await safeFetch('/api/projects/remove',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path})});
  if(r)loadProjects();
}

// ── Connection manager ─────────────────────────────────────────────────────────
function onConnTypeChange(){
  const isSql=g('connType').value==='sqlite';
  g('connPathField').style.display=isSql?'flex':'none';
  ['connHostField','connDbField','connUserField'].forEach(id=>g(id).style.display=isSql?'none':'flex');
}
async function testConn(){
  const body={connector_type:g('connType').value,path:g('connPath').value||null,host:g('connHost').value||null};
  const r=await safeFetch('/api/database/connect',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  if(r){g('connTestMsg').textContent=r.status;g('connTestMsg').style.color=r.ok?'#2e7d32':'#c62828';}
}
async function saveConn(){
  const body={name:g('connName').value,connector_type:g('connType').value,path:g('connPath').value||null,host:g('connHost').value||null,database:g('connDb').value||null,username:g('connUser').value||null};
  if(!body.name){g('connTestMsg').textContent='Connection name required';return;}
  const r=await safeFetch('/api/database/connections/save',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  if(r){g('connTestMsg').textContent=r.saved?'✓ Connection saved':'✗ '+(r.error||'Error');g('connTestMsg').style.color=r.saved?'#2e7d32':'#c62828';if(r.saved)loadConnections();}
}
async function loadConnections(){
  const s=await safeFetch('/api/settings');
  if(!s)return;
  const list=g('connList'); list.innerHTML='';
  (s.db_connections||[]).forEach(c=>{
    const row=document.createElement('div'); row.className='conn-row';
    const label=c.connector_type.charAt(0).toUpperCase()+c.connector_type.slice(1);
    const detail=c.path||c.host||'';
    row.innerHTML=`<span style="flex:1;font-weight:bold">${c.name}</span><span class="conn-tag">${label}</span><span style="color:#888;font-size:11px;flex:1;overflow:hidden;text-overflow:ellipsis">${detail}</span><button class="sbtn s-teal" onclick="useConn('${c.id}')" style="padding:3px 8px;font-size:11px">Use</button><button class="sbtn s-red" onclick="delConn('${c.id}')" style="padding:3px 8px;font-size:11px">✕</button>`;
    list.appendChild(row);
  });
}
async function useConn(id){
  const s=await safeFetch('/api/settings');
  if(!s)return;
  const c=(s.db_connections||[]).find(x=>x.id===id); if(!c)return;
  g('connType').value=c.connector_type; onConnTypeChange();
  g('connName').value=c.name;
  if(c.path){g('connPath').value=c.path; if(c.connector_type==='sqlite')g('dbPath').value=c.path;}
  if(c.host)g('connHost').value=c.host;
  if(c.database)g('connDb').value=c.database;
  if(c.username)g('connUser').value=c.username;
}
async function delConn(id){
  const r=await safeFetch('/api/database/connections/remove',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id})});
  if(r)loadConnections();
}

// ── SQLite explorer ────────────────────────────────────────────────────────────
function dbPath(){return g('dbPath').value.trim();}
async function dbTest(){
  const path=dbPath(); if(!path){g('dbStatus').textContent='Enter a path first.';return;}
  g('dbStatus').textContent='Testing…'; g('dbStatus').style.color='#888';
  const r=await safeFetch('/api/database/test',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path})});
  if(!r){g('dbStatus').textContent='Connection failed';g('dbStatus').style.color='#c62828';return;}
  g('dbStatus').textContent=r.status; g('dbStatus').style.color=r.ok?'#2e7d32':'#c62828';
}
async function dbLoadTables(){
  const path=dbPath(); if(!path)return;
  const r=await safeFetch('/api/database/sqlite/tables',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path})});
  const list=g('tableList'); list.innerHTML='';
  if(!r){list.innerHTML='<div style="color:#aaa;font-size:11px">Request failed</div>';return;}
  if(r.ok&&r.tables&&r.tables.length){
    r.tables.forEach(t=>{
      const b=document.createElement('button'); b.className='tbl-btn'; b.textContent=t.name;
      b.onclick=()=>dbSchema(t.name); list.appendChild(b);
    });
  }else{list.innerHTML='<div style="color:#aaa;font-size:11px">'+(r.error||'No tables found')+'</div>';}
}
async function dbSchema(name){
  const path=dbPath();
  const r=await safeFetch('/api/database/sqlite/schema',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path,table:name})});
  if(!r)return;
  g('schemaTableName').textContent=name;
  const wrap=g('schemaWrap'); wrap.innerHTML='';
  if(r.ok&&r.columns){
    r.columns.forEach(col=>{
      const d=document.createElement('div'); d.className='schema-row'+(col.pk?' schema-pk':'');
      d.textContent=`${col.name}  ${col.col_type}${col.pk?' PK':''}${col.notnull?' NOT NULL':''}`;
      wrap.appendChild(d);
    });
    g('sqlQuery').value=`SELECT * FROM "${name}" LIMIT 50`;
  }
}
async function dbQuery(){
  const path=dbPath(), sql=g('sqlQuery').value.trim();
  g('queryErr').textContent=''; g('rowCount').textContent=''; g('resultTable').innerHTML='';
  if(!path||!sql)return;
  const r=await safeFetch('/api/database/sqlite/query',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path,sql})});
  if(!r){g('queryErr').textContent='Request failed';return;}
  if(!r.ok){g('queryErr').textContent=r.error||'Query failed';return;}
  const tbl=g('resultTable');
  const thead=tbl.createTHead(), hrow=thead.insertRow();
  (r.columns||[]).forEach(c=>{const th=document.createElement('th');th.textContent=c;hrow.appendChild(th);});
  const tbody=tbl.createTBody();
  (r.rows||[]).forEach(row=>{const tr=tbody.insertRow();row.forEach(val=>{const td=tr.insertCell();td.textContent=val===null?'NULL':String(val);});});
  g('rowCount').textContent=(r.row_count||0)+' row'+(r.row_count!==1?'s':'');
}

// ── Generated Projects ─────────────────────────────────────────────────────────
let _genProjects=[], _pvSlug='', _pvPath='';
function getProj(slug){ return _genProjects.find(p=>p.slug===slug)||null; }

async function loadGeneratedProjects(){
  const grid=g('genGrid');
  if(grid) grid.innerHTML='<div style="color:#aaa;font-size:12px;padding:16px">Loading...</div>';
  try{
    _genProjects=await fetch('/api/generated-projects').then(r=>r.json());
    populateSidebarGen();
    filterGenProjects();
  }catch(e){
    if(grid) grid.innerHTML='<div style="color:#c62828;font-size:12px;padding:16px">Error: '+e+'</div>';
  }
}

function populateSidebarGen(){
  const list=g('sbGenList'); if(!list)return;
  if(!_genProjects.length){
    list.innerHTML='<div style="font-size:10px;color:#333;padding:4px 8px">No projects yet</div>';
    return;
  }
  list.innerHTML=_genProjects.slice(0,10).map(p=>`<div class="sb-gen-item" onclick="openProjectPreview('${p.slug}')"><div class="sb-gen-dot"></div><div class="sb-gen-name" title="${p.name}">${p.name}</div></div>`).join('');
}

function filterGenProjects(){
  const q=g('genSearch')?g('genSearch').value.toLowerCase():'';
  const tf=g('genTypeFilter')?g('genTypeFilter').value:'';
  const grid=g('genGrid'); if(!grid)return;
  const filtered=_genProjects.filter(p=>{
    const mq=!q||(p.name.toLowerCase().includes(q)||p.project_type.toLowerCase().includes(q)||p.slug.toLowerCase().includes(q));
    const mt=!tf||p.project_type===tf;
    return mq&&mt;
  });
  if(!filtered.length){
    grid.innerHTML='<div style="color:#aaa;font-size:12px;padding:16px">'+(_genProjects.length?'No projects match filter.':'No generated projects yet. Use: build project &lt;idea&gt;')+'</div>';
    return;
  }
  grid.innerHTML=filtered.map(p=>`<div class="gen-card">
  <div class="gen-card-top">
    <div class="gen-card-name">${p.name}</div>
    <span class="gen-type-badge">${p.project_type}</span>
  </div>
  <div class="gen-path" title="${p.path}">${p.path}</div>
  <div class="gen-files">${p.file_count} files &middot; <span class="gen-status">Ready</span></div>
  <div class="gen-actions">
    <button class="act-btn act-preview"  onclick="openProjectPreview('${p.slug}')">Open</button>
    <button class="act-btn act-download" onclick="downloadZip('${p.slug}')">ZIP</button>
    <button class="act-btn act-copy"     onclick="copyPath('${p.path.replace(/\\/g,'/')}')">Copy Path</button>
    <button class="act-btn act-readme"   onclick="viewReadme('${p.slug}')">README</button>
    <button class="act-btn act-improve"  onclick="openEnhance('${p.slug}','improve')">Improve</button>
    <button class="act-btn act-feature"  onclick="openEnhance('${p.slug}','add_feature')">Feature</button>
  </div>
</div>`).join('');
}

function openProjectPreview(slug){
  const p=getProj(slug);
  if(!p){ loadGeneratedProjects().then(()=>{ const pp=getProj(slug); if(pp) openProjectPreview(pp.slug); }); return; }
  _pvSlug=p.slug; _pvPath=p.path;
  g('pvName').textContent=p.name;
  g('pvBadge').innerHTML='<span class="gen-type-badge">'+p.project_type+'</span>';
  g('pvPath').textContent=p.path;
  g('pvFiles').textContent=p.file_count+' files · Ready ✓';
  g('pvFrame').src='/project/'+p.slug;
  setView('vPreview');
}

function pvFullPage(){if(_pvSlug)window.open('/project/'+_pvSlug,'_blank');}
function pvDownload(){if(_pvSlug)window.location.href='/api/projects/download/'+_pvSlug;}
function pvReadme(){if(_pvSlug)viewReadme(_pvSlug);}
function pvCopy(){if(_pvPath)copyPath(_pvPath.replace(/\\/g,'/'));}
function pvImprove(){if(_pvSlug)openEnhance(_pvSlug,'improve');}
function pvFeature(){if(_pvSlug)openEnhance(_pvSlug,'add_feature');}

function previewProj(slug){window.open('/project/'+slug,'_blank');}
function downloadZip(slug){window.location.href='/api/projects/download/'+slug;}
async function copyPath(path){
  try{await navigator.clipboard.writeText(path);}
  catch(_){const t=document.createElement('textarea');t.value=path;document.body.appendChild(t);t.select();document.execCommand('copy');document.body.removeChild(t);}
}
async function viewReadme(slug){
  try{
    const txt=await fetch('/generated/'+slug+'/README.md').then(r=>r.ok?r.text():Promise.resolve('README not found'));
    g('readmeContent').textContent=txt;
  }catch(_){g('readmeContent').textContent='README not found';}
  g('readmeModal').style.display='flex';
}
function closeReadme(){g('readmeModal').style.display='none';}

// ── Project Enhancer ───────────────────────────────────────────────────────────
let _eSlug='', _eMode='';
function openEnhance(slug,mode){
  _eSlug=slug; _eMode=mode;
  g('enhTitle').textContent=(mode==='improve'?'Improve: ':'Add Feature: ')+slug;
  g('enhInput').value=''; g('enhMsg').textContent='';
  g('enhModal').style.display='flex';
  setTimeout(()=>g('enhInput').focus(),50);
}
function closeEnhance(){g('enhModal').style.display='none';}
async function runEnhance(){
  const instr=g('enhInput').value.trim();
  if(!instr){g('enhMsg').textContent='Enter an instruction.';g('enhMsg').style.color='#c62828';return;}
  g('enhMsg').textContent='Running…'; g('enhMsg').style.color='#888';
  try{
    const r=await fetch('/api/projects/enhance',{method:'POST',headers:{'Content-Type':'application/json'},
      body:JSON.stringify({project:_eSlug,mode:_eMode,instruction:instr})}).then(r=>r.json());
    setView('vDash');
    (r.logs||[]).forEach(l=>addTo('eLog',l,logCls(l)));
    g('enhMsg').style.color=r.success?'#2e7d32':'#c62828';
    g('enhMsg').textContent=r.success
      ?'✓ Done — '+(r.changed_files||[]).join(', ')
      :'✗ '+((r.logs||[]).slice(-1)[0]||'Failed');
    if(r.success)setTimeout(()=>{closeEnhance();setView('vGenerated');},1400);
  }catch(e){g('enhMsg').textContent='Error: '+e;g('enhMsg').style.color='#c62828';}
}
g('enhInput').addEventListener('keydown',e=>{if(e.key==='Enter')runEnhance();});
// Populate sidebar on load
loadGeneratedProjects();
</script>
</body>
</html>"##;
