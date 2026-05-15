use anyhow::{anyhow, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct TableInfo {
    pub name: String,
}

#[derive(Serialize)]
pub struct ColumnInfo {
    pub cid: i64,
    pub name: String,
    pub col_type: String,
    pub notnull: bool,
    pub pk: bool,
}

#[derive(Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
}

// ── Safety checks ─────────────────────────────────────────────────────────────

const BLOCKED_KEYWORDS: &[&str] = &[
    "DROP", "DELETE", "UPDATE", "INSERT", "ALTER",
    "TRUNCATE", "CREATE", "REPLACE", "ATTACH", "DETACH",
    "PRAGMA",
];

fn is_safe_select(sql: &str) -> bool {
    let upper = sql.trim().to_uppercase();
    if !upper.starts_with("SELECT") {
        return false;
    }
    let words: Vec<&str> = upper.split_whitespace().collect();
    !words.iter().any(|w| BLOCKED_KEYWORDS.contains(w))
}

fn validate_identifier(name: &str) -> Result<()> {
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(anyhow!("Invalid identifier '{}'", name))
    }
}

// ── Connection helper ─────────────────────────────────────────────────────────

fn open_ro(path: &str) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| anyhow!("Cannot open '{}': {}", path, e))
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn test_connection(path: &str) -> Result<String> {
    let conn = open_ro(path)?;
    let version: String =
        conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    Ok(format!("Connected — SQLite v{}", version))
}

pub fn list_tables(path: &str) -> Result<Vec<TableInfo>> {
    let conn = open_ro(path)?;
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
    )?;
    let tables = stmt
        .query_map([], |row| Ok(TableInfo { name: row.get(0)? }))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(tables)
}

pub fn table_schema(path: &str, table: &str) -> Result<Vec<ColumnInfo>> {
    validate_identifier(table)?;
    let conn = open_ro(path)?;
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table))?;
    let cols = stmt
        .query_map([], |row| {
            Ok(ColumnInfo {
                cid: row.get(0)?,
                name: row.get(1)?,
                col_type: row.get(2)?,
                notnull: row.get::<_, i32>(3)? != 0,
                pk: row.get::<_, i32>(5)? != 0,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(cols)
}

pub fn run_query(path: &str, sql: &str) -> Result<QueryResult> {
    if !is_safe_select(sql) {
        return Err(anyhow!(
            "Only SELECT queries are permitted. \
             DROP / DELETE / UPDATE / INSERT / ALTER / CREATE are blocked."
        ));
    }

    let conn = open_ro(path)?;
    let mut stmt = conn.prepare(sql.trim())?;

    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let col_count = columns.len();

    let rows: Vec<Vec<serde_json::Value>> = stmt
        .query_map([], |row| {
            let mut vals = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let v = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => {
                        serde_json::Value::Number(n.into())
                    }
                    rusqlite::types::ValueRef::Real(f) => {
                        serde_json::Number::from_f64(f)
                            .map(serde_json::Value::Number)
                            .unwrap_or_else(|| serde_json::Value::String(f.to_string()))
                    }
                    rusqlite::types::ValueRef::Text(t) => {
                        serde_json::Value::String(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(_) => {
                        serde_json::Value::String("[BLOB]".to_string())
                    }
                };
                vals.push(v);
            }
            Ok(vals)
        })?
        .filter_map(|r| r.ok())
        .take(500)
        .collect();

    let row_count = rows.len();
    Ok(QueryResult { columns, rows, row_count })
}

// ── Multi-connector typed test ────────────────────────────────────────────────

/// Tests a connection by connector type. SQLite is fully implemented;
/// all other drivers return a safe placeholder until their crate is added.
pub fn test_connection_by_type(connector_type: &str, sqlite_path: Option<&str>) -> Result<String> {
    match connector_type {
        "sqlite" => {
            let path = sqlite_path.ok_or_else(|| anyhow!("SQLite path is required"))?;
            test_connection(path)
        }
        "postgresql" => Ok(
            "PostgreSQL: driver pending. \
             Add `tokio-postgres` or `sqlx` to Cargo.toml to enable real connections."
                .to_string(),
        ),
        "mysql" => Ok(
            "MySQL: driver pending. \
             Add `mysql_async` or `sqlx` to Cargo.toml to enable real connections."
                .to_string(),
        ),
        "mongodb" => Ok(
            "MongoDB: driver pending. \
             Add `mongodb` to Cargo.toml to enable real connections."
                .to_string(),
        ),
        "supabase" => Ok(
            "Supabase: uses PostgreSQL wire protocol. \
             PostgreSQL driver pending."
                .to_string(),
        ),
        "neon" => Ok(
            "Neon: uses PostgreSQL wire protocol over WebSocket. \
             PostgreSQL driver pending."
                .to_string(),
        ),
        "planetscale" => Ok(
            "PlanetScale: uses MySQL wire protocol. \
             MySQL driver pending."
                .to_string(),
        ),
        "firebase" => Ok(
            "Firebase: uses REST/gRPC, not SQL. \
             REST connector pending — no SQL queries will be available."
                .to_string(),
        ),
        other => Err(anyhow!("Unknown connector type: '{}'", other)),
    }
}

/// Connector metadata for the UI.
#[allow(dead_code)]
pub fn connector_label(t: &str) -> &'static str {
    match t {
        "sqlite"      => "SQLite",
        "postgresql"  => "PostgreSQL",
        "mysql"       => "MySQL",
        "mongodb"     => "MongoDB",
        "supabase"    => "Supabase",
        "neon"        => "Neon",
        "planetscale" => "PlanetScale",
        "firebase"    => "Firebase",
        _             => "Unknown",
    }
}

#[allow(dead_code)]
pub fn connector_supports_sql(t: &str) -> bool {
    matches!(t, "sqlite" | "postgresql" | "mysql" | "supabase" | "neon" | "planetscale")
}
