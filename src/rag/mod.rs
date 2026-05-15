use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::WalkDir;

const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    ".next",
    ".venv",
    "__pycache__",
    ".cache",
    ".idea",
    ".vs",
];

// Files larger than this are skipped entirely
const MAX_FILE_SIZE: u64 = 1_048_576; // 1 MB

// Files larger than this are not read for import scanning
const MAX_SCAN_SIZE: u64 = 51_200; // 50 KB

// Max lines read per file during import scanning
const MAX_SCAN_LINES: usize = 100;

// Max imports recorded per file
const MAX_IMPORTS: usize = 20;

#[derive(Debug, Clone)]
pub enum RoleHint {
    RustEntrypoint,    // main.rs
    RustModule,        // mod.rs
    RustSource,        // other *.rs
    RustManifest,      // Cargo.toml
    NodeManifest,      // package.json
    NodeScript,        // *.ts, *.js (non-component)
    PythonSource,      // *.py
    FrontendComponent, // *.tsx, *.jsx
    StyleSheet,        // *.css, *.scss
    EnvConfig,         // .env*
    Documentation,     // *.md, *.txt
    Config,            // *.toml, *.yaml, *.json (generic)
    LockFile,          // lock files
    Unknown,
}

impl RoleHint {
    pub fn label(&self) -> &'static str {
        match self {
            RoleHint::RustEntrypoint => "rust/entrypoint",
            RoleHint::RustModule => "rust/module",
            RoleHint::RustSource => "rust/source",
            RoleHint::RustManifest => "rust/manifest",
            RoleHint::NodeManifest => "node/manifest",
            RoleHint::NodeScript => "node/script",
            RoleHint::PythonSource => "python/source",
            RoleHint::FrontendComponent => "frontend/component",
            RoleHint::StyleSheet => "frontend/style",
            RoleHint::EnvConfig => "config/env",
            RoleHint::Documentation => "docs",
            RoleHint::Config => "config",
            RoleHint::LockFile => "lockfile",
            RoleHint::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileNode {
    pub path: PathBuf,
    pub relative_path: String,
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub role: RoleHint,
    /// use/mod/import lines extracted from the file
    pub imports: Vec<String>,
}

#[derive(Debug)]
pub struct ProjectMap {
    pub root: PathBuf,
    pub files: Vec<FileNode>,
    pub skipped_count: usize,
}

pub fn map_project(root: &Path) -> Result<ProjectMap> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut files: Vec<FileNode> = Vec::new();
    let mut skipped_count: usize = 0;

    let walker = WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Prune ignored directory names before descending
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                return !IGNORED_DIRS.contains(&name.as_ref());
            }
            true
        });

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                skipped_count += 1;
                continue;
            }
        };

        if entry.file_type().is_dir() {
            continue;
        }

        let path = entry.path().to_path_buf();

        let size_bytes = match entry.metadata() {
            Ok(m) => m.len(),
            Err(_) => {
                skipped_count += 1;
                continue;
            }
        };

        if size_bytes > MAX_FILE_SIZE {
            skipped_count += 1;
            continue;
        }

        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase().to_string());

        let role = detect_role(&file_name, extension.as_deref());

        let imports = if should_scan(&role, size_bytes) {
            scan_imports(&path, extension.as_deref()).unwrap_or_default()
        } else {
            Vec::new()
        };

        files.push(FileNode {
            path,
            relative_path,
            extension,
            size_bytes,
            role,
            imports,
        });
    }

    // Manifests and entrypoints first, then alphabetical
    files.sort_by(|a, b| {
        role_priority(&b.role)
            .cmp(&role_priority(&a.role))
            .then(a.relative_path.cmp(&b.relative_path))
    });

    Ok(ProjectMap {
        root,
        files,
        skipped_count,
    })
}

fn detect_role(file_name: &str, ext: Option<&str>) -> RoleHint {
    // Exact filename matches take precedence
    match file_name {
        "cargo.toml" => return RoleHint::RustManifest,
        "package.json" => return RoleHint::NodeManifest,
        "main.rs" => return RoleHint::RustEntrypoint,
        "mod.rs" => return RoleHint::RustModule,
        ".env" | ".env.local" | ".env.example" | ".env.production" => {
            return RoleHint::EnvConfig;
        }
        "cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" => {
            return RoleHint::LockFile;
        }
        _ => {}
    }

    // .env.* prefix catch (e.g. ".env.development")
    if file_name.starts_with(".env") {
        return RoleHint::EnvConfig;
    }

    match ext {
        Some("rs") => RoleHint::RustSource,
        Some("py") => RoleHint::PythonSource,
        Some("tsx") | Some("jsx") => RoleHint::FrontendComponent,
        Some("ts") | Some("js") | Some("mjs") | Some("cjs") => RoleHint::NodeScript,
        Some("css") | Some("scss") | Some("sass") | Some("less") => RoleHint::StyleSheet,
        Some("md") | Some("mdx") | Some("rst") | Some("txt") => RoleHint::Documentation,
        Some("toml") | Some("yaml") | Some("yml") | Some("json") | Some("ini") | Some("cfg") => {
            RoleHint::Config
        }
        _ => RoleHint::Unknown,
    }
}

fn should_scan(role: &RoleHint, size: u64) -> bool {
    matches!(
        role,
        RoleHint::RustEntrypoint
            | RoleHint::RustModule
            | RoleHint::RustSource
            | RoleHint::PythonSource
    ) && size <= MAX_SCAN_SIZE
}

fn scan_imports(path: &Path, ext: Option<&str>) -> Result<Vec<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut imports = Vec::new();
    let mut lines_read = 0;

    for line in reader.lines() {
        let line = line?;
        lines_read += 1;
        if lines_read > MAX_SCAN_LINES {
            break;
        }

        let trimmed = line.trim();

        match ext {
            Some("rs") => {
                if trimmed.starts_with("use ") || trimmed.starts_with("mod ") {
                    // Extract the first token after the keyword (e.g. "use tokio;" → "tokio")
                    if let Some(token) = trimmed.split_whitespace().nth(1) {
                        let clean = token
                            .trim_end_matches(';')
                            .trim_end_matches('{')
                            .trim_end_matches("::")
                            .to_string();
                        if !clean.is_empty() {
                            imports.push(clean);
                        }
                    }
                }
            }
            Some("py") => {
                if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                    imports.push(trimmed.to_string());
                }
            }
            _ => {}
        }

        if imports.len() >= MAX_IMPORTS {
            break;
        }
    }

    Ok(imports)
}

fn role_priority(role: &RoleHint) -> u8 {
    match role {
        RoleHint::RustManifest | RoleHint::NodeManifest => 10,
        RoleHint::RustEntrypoint => 9,
        RoleHint::EnvConfig => 8,
        RoleHint::Documentation => 7,
        RoleHint::RustModule => 6,
        RoleHint::RustSource => 5,
        RoleHint::PythonSource | RoleHint::FrontendComponent => 4,
        RoleHint::NodeScript => 3,
        RoleHint::Config => 2,
        RoleHint::StyleSheet => 1,
        RoleHint::LockFile | RoleHint::Unknown => 0,
    }
}
