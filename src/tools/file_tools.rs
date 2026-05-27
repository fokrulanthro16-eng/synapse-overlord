#![allow(dead_code)]

//! Read-only file tools: `file_read`, `file_list`, `file_search`.
//!
//! **Path safety contract**
//!
//! Every function that accepts a user-supplied path MUST call `safe_resolve`
//! (for existing paths) or `safe_resolve_new` (for paths that may not exist
//! yet, used by the patch system).  Both functions:
//!
//! 1. Reject null bytes immediately (pre-FS check).
//! 2. Join the user path onto the canonicalized project root.
//! 3. Verify the result still lives inside the root.
//!
//! `safe_resolve` uses `Path::canonicalize` which resolves symlinks and
//! requires the path to exist — so traversal via symlinks is also blocked.
//!
//! `safe_resolve_new` uses lexical normalization (no FS access) for paths
//! that haven't been created yet.

use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Result};

// ── Argument structs ──────────────────────────────────────────────────────────

pub struct FileReadArgs {
    pub path: String,
    pub max_lines: usize,
}

pub struct FileListArgs {
    pub path: String,
}

pub struct FileSearchArgs {
    pub pattern: String,
    /// Directory to search in (relative to root).
    pub path: String,
}

// ── Constants ─────────────────────────────────────────────────────────────────

/// Hard cap on how many bytes we'll read from a single file.
const MAX_READ_BYTES: u64 = 65_536; // 64 KB

/// Hard cap on lines returned per read call (user may set lower via max_lines).
const HARD_MAX_LINES: usize = 500;

/// Max matching lines returned by a single search call.
const MAX_SEARCH_RESULTS: usize = 50;

/// Directories skipped during traversal.
const IGNORED_DIRS: &[&str] = &[
    "target", "node_modules", ".git", "dist", "build",
    ".next", ".venv", "__pycache__", ".cache", ".idea", ".vs",
];

// ── Safe path resolvers ───────────────────────────────────────────────────────

/// Resolve `user_path` relative to `root`, then verify the resolved
/// canonical path is contained within `root`.
///
/// Requires the path to already exist (uses `canonicalize`).
/// Blocks path traversal via `..`, symlinks pointing outside root, null bytes,
/// and any absolute path supplied by the caller.
pub(crate) fn safe_resolve(root: &Path, user_path: &str) -> Result<PathBuf> {
    reject_null_bytes(user_path)?;
    reject_absolute(user_path)?;

    let canonical_root = root
        .canonicalize()
        .map_err(|_| anyhow!("Project root '{}' is not accessible", root.display()))?;

    let joined = canonical_root.join(user_path);

    let canonical_target = joined
        .canonicalize()
        .map_err(|_| anyhow!("Path not found: '{}'", user_path))?;

    if !canonical_target.starts_with(&canonical_root) {
        return Err(anyhow!(
            "Access denied: '{}' is outside the project root",
            user_path
        ));
    }

    Ok(canonical_target)
}

/// Resolve `user_path` relative to `root` using *lexical* normalization only
/// (no filesystem access).  Used for patch proposals where the file may not
/// exist yet.
///
/// Rejects: null bytes, absolute paths, and any `..` traversal that would
/// escape the root.
pub fn safe_resolve_new(root: &Path, user_path: &str) -> Result<PathBuf> {
    reject_null_bytes(user_path)?;
    reject_absolute(user_path)?;

    let canonical_root = root
        .canonicalize()
        .map_err(|_| anyhow!("Project root '{}' is not accessible", root.display()))?;

    let joined = canonical_root.join(user_path);
    let normalized = normalize_path(&joined);

    if !normalized.starts_with(&canonical_root) {
        return Err(anyhow!(
            "Access denied: '{}' escapes the project root",
            user_path
        ));
    }

    Ok(normalized)
}

/// Lexically normalize a path by resolving `.` and `..` components
/// without hitting the filesystem.  Matches the algorithm used by Cargo.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(p)  => out.push(p.as_os_str()),
            Component::RootDir    => out.push(component),
            Component::CurDir     => { /* skip */ }
            Component::ParentDir  => {
                if !out.pop() {
                    // Can't go above an empty path — push literally so the
                    // starts_with check will catch the escape attempt.
                    out.push(component);
                }
            }
            Component::Normal(n)  => out.push(n),
        }
    }
    out
}

fn reject_null_bytes(s: &str) -> Result<()> {
    if s.contains('\0') {
        Err(anyhow!("Path contains null byte — rejected"))
    } else {
        Ok(())
    }
}

/// Reject user-supplied paths that are absolute or root-relative.
///
/// Two cases must be blocked:
///
/// 1. **Truly absolute** (has a prefix on Windows, leading `/` on Unix) — caught
///    by `Path::is_absolute()`.
///
/// 2. **Root-relative on Windows** — paths like `/etc/passwd` or `\Windows\…`
///    are *not* considered absolute by `Path::is_absolute()` on Windows (no
///    drive letter), but `PathBuf::join("/etc/passwd")` silently resolves them
///    to `{current_drive}:\etc\passwd`, potentially escaping the project root.
///    We block these by inspecting the raw byte prefix.
fn reject_absolute(s: &str) -> Result<()> {
    let is_abs = std::path::Path::new(s).is_absolute()
        || s.starts_with('/')
        || s.starts_with('\\');

    if is_abs {
        Err(anyhow!(
            "Absolute paths are not permitted: '{}'. \
             Provide a path relative to the project root.",
            s
        ))
    } else {
        Ok(())
    }
}

// ── Tool: file_read ───────────────────────────────────────────────────────────

/// Read a file within `root` and return numbered lines.
///
/// Limits: 64 KB size cap, `HARD_MAX_LINES` line cap (user cap via `args.max_lines`).
pub fn read_file(root: &Path, args: FileReadArgs) -> Result<String> {
    let target = safe_resolve(root, &args.path)?;

    let meta = std::fs::metadata(&target)
        .map_err(|e| anyhow!("Cannot stat '{}': {}", args.path, e))?;

    if meta.is_dir() {
        return Err(anyhow!(
            "'{}' is a directory. Use file_list to list its contents.",
            args.path
        ));
    }

    if meta.len() > MAX_READ_BYTES {
        return Err(anyhow!(
            "'{}' is {} bytes — exceeds the {} byte read limit. \
             Use file_search to locate specific content instead.",
            args.path,
            meta.len(),
            MAX_READ_BYTES
        ));
    }

    let max = args.max_lines.min(HARD_MAX_LINES).max(1);
    let file = std::fs::File::open(&target)
        .map_err(|e| anyhow!("Cannot open '{}': {}", args.path, e))?;
    let reader = BufReader::new(file);

    let mut shown: Vec<String> = Vec::new();
    let mut total = 0usize;

    for raw in reader.lines() {
        total += 1;
        let line = raw.map_err(|e| anyhow!("Read error in '{}': {}", args.path, e))?;
        if total <= max {
            shown.push(format!("{:>4} │ {}", total, line));
        }
    }

    let mut out = format!(
        "File: {path}\nSize: {size} bytes | Shown: {shown}/{total} lines\n{sep}\n",
        path  = args.path,
        size  = meta.len(),
        shown = shown.len(),
        total = total,
        sep   = "─".repeat(60),
    );
    out.push_str(&shown.join("\n"));

    if total > max {
        out.push_str(&format!(
            "\n… {} more lines not shown (increase max_lines or use file_search)",
            total - max
        ));
    }

    Ok(out)
}

// ── Tool: file_list ───────────────────────────────────────────────────────────

/// List the immediate children of a directory within `root`.
///
/// Hidden entries (`.`-prefixed) and build-artefact directories are excluded.
pub fn list_directory(root: &Path, args: FileListArgs) -> Result<String> {
    let target = safe_resolve(root, &args.path)?;

    if !target.is_dir() {
        return Err(anyhow!(
            "'{}' is a file, not a directory. Use file_read to read it.",
            args.path
        ));
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|_| anyhow!("Project root inaccessible"))?;

    let mut entries: Vec<(String, &'static str, String, String)> = Vec::new();

    for item in std::fs::read_dir(&target)
        .map_err(|e| anyhow!("Cannot read directory '{}': {}", args.path, e))?
        .flatten()
    {
        let path = item.path();
        let name = item.file_name().to_string_lossy().to_string();

        // Skip hidden and ignored names
        if name.starts_with('.') { continue; }
        if path.is_dir() && IGNORED_DIRS.contains(&name.as_str()) { continue; }

        let rel = path
            .strip_prefix(&canonical_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        let (kind, size_str) = if path.is_dir() {
            ("dir ", String::new())
        } else {
            let bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
            let s = if bytes >= 1024 {
                format!("{} KB", bytes / 1024)
            } else {
                format!("{} B", bytes)
            };
            ("file", s)
        };

        entries.push((name, kind, rel, size_str));
    }

    // Directories before files; alphabetical within each group
    entries.sort_by(|a, b| {
        let a_dir = a.1 == "dir ";
        let b_dir = b.1 == "dir ";
        b_dir.cmp(&a_dir).then(a.0.cmp(&b.0))
    });

    let label = if args.path == "." { "(project root)" } else { &args.path };
    let mut out = format!(
        "Directory: {label}\n{count} entries\n{sep}\n",
        label = label,
        count = entries.len(),
        sep   = "─".repeat(60),
    );

    for (_, kind, rel, size) in &entries {
        if size.is_empty() {
            out.push_str(&format!("  [{kind}]  {rel}\n", kind = kind, rel = rel));
        } else {
            out.push_str(&format!(
                "  [{kind}]  {rel}  ({size})\n",
                kind = kind, rel = rel, size = size
            ));
        }
    }

    Ok(out)
}

// ── Tool: file_search ─────────────────────────────────────────────────────────

/// Case-insensitive plain-text search across source files under `root`.
///
/// At most `MAX_SEARCH_RESULTS` matching lines are returned.
/// Binary files, files >512 KB, and build-artefact directories are skipped.
pub fn search_files(root: &Path, args: FileSearchArgs) -> Result<String> {
    let search_root = safe_resolve(root, &args.path)?;
    if !search_root.is_dir() {
        return Err(anyhow!("Search path '{}' is not a directory", args.path));
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|_| anyhow!("Project root inaccessible"))?;

    let pattern = args.pattern.to_lowercase();
    if pattern.is_empty() {
        return Err(anyhow!("Search pattern cannot be empty"));
    }

    let mut results: Vec<String> = Vec::new();
    let mut files_scanned = 0usize;
    search_recursive(&search_root, &canonical_root, &pattern, &mut results, &mut files_scanned);

    let mut out = format!(
        "Search: '{pat}' in '{dir}'\nFiles scanned: {scanned} | Matches: {matches}\n{sep}\n",
        pat     = args.pattern,
        dir     = args.path,
        scanned = files_scanned,
        matches = results.len(),
        sep     = "─".repeat(60),
    );

    if results.is_empty() {
        out.push_str("No matches found.\n");
    } else {
        for line in results.iter().take(MAX_SEARCH_RESULTS) {
            out.push_str(line);
            out.push('\n');
        }
        if results.len() > MAX_SEARCH_RESULTS {
            out.push_str(&format!(
                "… {} more matches omitted — refine your search pattern\n",
                results.len() - MAX_SEARCH_RESULTS
            ));
        }
    }

    Ok(out)
}

fn search_recursive(
    dir: &Path,
    root: &Path,
    pattern: &str,
    results: &mut Vec<String>,
    scanned: &mut usize,
) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };

    for item in rd.flatten() {
        let path = item.path();
        let name = item.file_name().to_string_lossy().to_string();

        if name.starts_with('.') { continue; }

        if path.is_dir() {
            if IGNORED_DIRS.contains(&name.as_str()) { continue; }
            search_recursive(&path, root, pattern, results, scanned);
            continue;
        }

        if !path.is_file() { continue; }

        // Only search text-like source files
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !is_text_ext(&ext) { continue; }

        // Size guard
        if path.metadata().map(|m| m.len()).unwrap_or(0) > 512_000 { continue; }

        *scanned += 1;

        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if let Ok(file) = std::fs::File::open(&path) {
            for (i, raw) in BufReader::new(file).lines().enumerate() {
                // Global cap to prevent runaway on huge repos
                if results.len() >= MAX_SEARCH_RESULTS * 4 { return; }

                let line = match raw { Ok(l) => l, Err(_) => break };
                if line.to_lowercase().contains(pattern) {
                    results.push(format!("{}:{}: {}", relative, i + 1, line.trim()));
                }
            }
        }
    }
}

fn is_text_ext(ext: &str) -> bool {
    matches!(
        ext,
        "rs"   | "toml" | "md"   | "txt"  | "json" | "yaml" | "yml"
        | "js" | "ts"   | "jsx"  | "tsx"  | "py"   | "html" | "css"
        | "scss" | "sh" | "env"  | "cfg"  | "ini"  | "lock"
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_tree() -> TempDir {
        let d = TempDir::new().unwrap();
        fs::write(d.path().join("hello.rs"), b"fn main() { println!(\"hello\"); }\n").unwrap();
        fs::create_dir(d.path().join("src")).unwrap();
        fs::write(
            d.path().join("src").join("lib.rs"),
            b"pub fn greet() -> &'static str { \"hi\" }\n",
        )
        .unwrap();
        d
    }

    // ── safe_resolve ──────────────────────────────────────────────────────────

    #[test]
    fn safe_resolve_allows_valid_path() {
        let d = make_tree();
        let resolved = safe_resolve(d.path(), "hello.rs").unwrap();
        assert!(resolved.exists());
    }

    #[test]
    fn safe_resolve_blocks_traversal() {
        let d = make_tree();
        let err = safe_resolve(d.path(), "../../etc/passwd").unwrap_err();
        let msg = err.to_string();
        // Either "not found" or "outside" depending on OS
        assert!(
            msg.contains("outside") || msg.contains("not found") || msg.contains("Access denied"),
            "expected traversal rejection, got: {msg}"
        );
    }

    #[test]
    fn safe_resolve_blocks_null_byte() {
        let d = make_tree();
        assert!(safe_resolve(d.path(), "src/\0main.rs").is_err());
    }

    #[test]
    fn safe_resolve_blocks_absolute_path() {
        let d = make_tree();
        // Absolute path must be rejected regardless of where it points
        let result = safe_resolve(d.path(), "/etc/passwd");
        assert!(result.is_err(), "absolute path should be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Absolute"), "expected 'Absolute' in error: {msg}");
    }

    #[test]
    fn safe_resolve_new_blocks_absolute_path() {
        let d = make_tree();
        let result = safe_resolve_new(d.path(), "/tmp/evil.rs");
        assert!(result.is_err(), "absolute path in safe_resolve_new should be rejected");
    }

    // ── safe_resolve_new ──────────────────────────────────────────────────────

    #[test]
    fn safe_resolve_new_allows_nonexistent_child() {
        let d = make_tree();
        let r = safe_resolve_new(d.path(), "new_module.rs").unwrap();
        assert_eq!(r, d.path().canonicalize().unwrap().join("new_module.rs"));
    }

    #[test]
    fn safe_resolve_new_blocks_traversal() {
        let d = make_tree();
        assert!(safe_resolve_new(d.path(), "../../evil.rs").is_err());
    }

    // ── read_file ─────────────────────────────────────────────────────────────

    #[test]
    fn read_file_returns_numbered_lines() {
        let d = make_tree();
        let out = read_file(d.path(), FileReadArgs { path: "hello.rs".into(), max_lines: 200 })
            .unwrap();
        assert!(out.contains("   1 │"), "expected line number: {out}");
        assert!(out.contains("println"));
    }

    #[test]
    fn read_file_rejects_directory() {
        let d = make_tree();
        let err = read_file(d.path(), FileReadArgs { path: "src".into(), max_lines: 200 })
            .unwrap_err();
        assert!(err.to_string().contains("directory"));
    }

    #[test]
    fn read_file_rejects_traversal() {
        let d = make_tree();
        assert!(
            read_file(d.path(), FileReadArgs { path: "../../passwd".into(), max_lines: 10 })
                .is_err()
        );
    }

    // ── list_directory ────────────────────────────────────────────────────────

    #[test]
    fn list_directory_shows_entries() {
        let d = make_tree();
        let out = list_directory(d.path(), FileListArgs { path: ".".into() }).unwrap();
        assert!(out.contains("hello.rs") || out.contains("src"), "output: {out}");
    }

    #[test]
    fn list_directory_rejects_file_path() {
        let d = make_tree();
        let err =
            list_directory(d.path(), FileListArgs { path: "hello.rs".into() }).unwrap_err();
        assert!(err.to_string().contains("file, not a directory"));
    }

    // ── search_files ──────────────────────────────────────────────────────────

    #[test]
    fn search_files_finds_pattern() {
        let d = make_tree();
        let out = search_files(
            d.path(),
            FileSearchArgs { pattern: "println".into(), path: ".".into() },
        )
        .unwrap();
        assert!(out.contains("hello.rs"));
        assert!(out.contains("println"));
    }

    #[test]
    fn search_files_no_match() {
        let d = make_tree();
        let out = search_files(
            d.path(),
            FileSearchArgs { pattern: "ZZZNOMATCH999".into(), path: ".".into() },
        )
        .unwrap();
        assert!(out.contains("No matches found"));
    }

    #[test]
    fn search_files_case_insensitive() {
        let d = make_tree();
        let out = search_files(
            d.path(),
            FileSearchArgs { pattern: "PRINTLN".into(), path: ".".into() },
        )
        .unwrap();
        assert!(out.contains("hello.rs"), "case-insensitive match should hit: {out}");
    }
}
