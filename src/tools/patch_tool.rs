#![allow(dead_code)]

//! Two-phase patch proposal system.
//!
//! **Workflow**
//! 1. Hermes calls `PatchStore::propose()` → returns a patch ID.
//! 2. The API layer returns the ID to the user with an approve/reject choice.
//! 3. User calls `PatchStore::approve(id)` → content is written to disk.
//!    OR `PatchStore::reject(id)` → discarded, nothing written.
//!
//! **Safety**
//! - `propose()` validates the target path via `safe_resolve_new` before
//!   storing anything.
//! - `approve()` re-validates the path at write time (belt-and-suspenders).
//! - Parent directories are created only on explicit approval.
//! - Patches expire after `PATCH_TTL_SECS` seconds and cannot be approved
//!   after expiry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};

use super::file_tools::safe_resolve_new;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Pending patches expire after this many seconds.
pub const PATCH_TTL_SECS: u64 = 600; // 10 minutes

/// Monotonically increasing sequence number — guarantees unique patch IDs even
/// when multiple proposals arrive within the same unix second.
static PATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

impl PatchStatus {
    pub fn label(&self) -> &'static str {
        match self {
            PatchStatus::Pending  => "pending",
            PatchStatus::Approved => "approved",
            PatchStatus::Rejected => "rejected",
            PatchStatus::Expired  => "expired",
        }
    }
}

/// A proposed file write, held in memory until approved or rejected.
#[derive(Debug, Clone)]
pub struct PatchProposal {
    /// Unique ID, e.g. `"patch_1717000000"`.
    pub id: String,
    /// Resolved absolute path (checked at propose time).
    pub file_path: PathBuf,
    /// Original user-supplied relative path (for display / re-validation).
    pub relative_path: String,
    /// Human-readable description of what the patch does.
    pub description: String,
    /// The full content to write on approval.
    pub new_content: String,
    pub status: PatchStatus,
    /// Unix timestamp (seconds) when the proposal was created.
    pub proposed_at: u64,
}

// ── PatchStore ────────────────────────────────────────────────────────────────

/// Thread-safe in-memory store for patch proposals.
#[derive(Clone, Default)]
pub struct PatchStore(Arc<Mutex<HashMap<String, PatchProposal>>>);

impl PatchStore {
    pub fn new() -> Self {
        Self::default()
    }

    // ── propose ───────────────────────────────────────────────────────────────

    /// Register a new patch proposal.
    ///
    /// Validates that `relative_path` stays inside `root` before storing.
    /// Returns the patch ID (e.g. `"patch_1717000000"`) on success.
    pub fn propose(
        &self,
        root: &Path,
        relative_path: &str,
        description: &str,
        new_content: &str,
    ) -> Result<String> {
        // Validate path safety before storing anything
        let absolute_path = safe_resolve_new(root, relative_path)?;

        let now = now_secs();
        // Combine timestamp + monotonic counter so IDs are unique even when
        // two proposals arrive in the same second.
        let seq = PATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("patch_{ts}_{seq:06}", ts = now, seq = seq);

        let proposal = PatchProposal {
            id: id.clone(),
            file_path: absolute_path,
            relative_path: relative_path.to_string(),
            description: description.to_string(),
            new_content: new_content.to_string(),
            status: PatchStatus::Pending,
            proposed_at: now,
        };

        self.0.lock().unwrap().insert(id.clone(), proposal);
        Ok(id)
    }

    // ── approve ───────────────────────────────────────────────────────────────

    /// Approve a pending patch and write the file to disk.
    ///
    /// Errors if:
    /// - The ID is not found.
    /// - The patch is not in `Pending` state.
    /// - The patch has expired.
    /// - Path re-validation fails (safety check).
    /// - The write fails.
    pub fn approve(&self, id: &str, root: &Path) -> Result<()> {
        let mut store = self.0.lock().unwrap();
        let proposal = store
            .get_mut(id)
            .ok_or_else(|| anyhow!("Patch '{}' not found", id))?;

        if proposal.status != PatchStatus::Pending {
            return Err(anyhow!(
                "Cannot approve patch '{}' — status is '{}'",
                id,
                proposal.status.label()
            ));
        }

        // Expiry check
        if now_secs().saturating_sub(proposal.proposed_at) > PATCH_TTL_SECS {
            proposal.status = PatchStatus::Expired;
            return Err(anyhow!("Patch '{}' has expired", id));
        }

        // Re-validate path at approval time
        let re_validated = safe_resolve_new(root, &proposal.relative_path)?;
        if re_validated != proposal.file_path {
            return Err(anyhow!(
                "Path validation mismatch for patch '{}' — write rejected for safety",
                id
            ));
        }

        // Create any missing parent directories
        if let Some(parent) = proposal.file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                anyhow!(
                    "Failed to create parent directories for '{}': {}",
                    proposal.relative_path,
                    e
                )
            })?;
        }

        // Write the file
        std::fs::write(&proposal.file_path, proposal.new_content.as_bytes()).map_err(|e| {
            anyhow!("Failed to write '{}': {}", proposal.relative_path, e)
        })?;

        proposal.status = PatchStatus::Approved;
        Ok(())
    }

    // ── reject ────────────────────────────────────────────────────────────────

    /// Reject a pending patch. No disk I/O is performed.
    pub fn reject(&self, id: &str) -> Result<()> {
        let mut store = self.0.lock().unwrap();
        let proposal = store
            .get_mut(id)
            .ok_or_else(|| anyhow!("Patch '{}' not found", id))?;

        if proposal.status != PatchStatus::Pending {
            return Err(anyhow!(
                "Cannot reject patch '{}' — status is '{}'",
                id,
                proposal.status.label()
            ));
        }

        proposal.status = PatchStatus::Rejected;
        Ok(())
    }

    // ── list_pending ──────────────────────────────────────────────────────────

    /// Return all pending (non-expired) proposals.
    /// Lazily marks expired proposals as `Expired` inside the lock.
    pub fn list_pending(&self) -> Vec<PatchProposal> {
        let mut store = self.0.lock().unwrap();
        let now = now_secs();

        for p in store.values_mut() {
            if p.status == PatchStatus::Pending
                && now.saturating_sub(p.proposed_at) > PATCH_TTL_SECS
            {
                p.status = PatchStatus::Expired;
            }
        }

        store
            .values()
            .filter(|p| p.status == PatchStatus::Pending)
            .cloned()
            .collect()
    }

    // ── get ───────────────────────────────────────────────────────────────────

    /// Retrieve a single proposal by ID (any status).
    pub fn get(&self, id: &str) -> Option<PatchProposal> {
        self.0.lock().unwrap().get(id).cloned()
    }

    // ── gc ────────────────────────────────────────────────────────────────────

    /// Remove all non-Pending entries (Approved, Rejected, Expired) from the
    /// store.  Returns the number of entries removed.
    ///
    /// Call this periodically (e.g., in `list_patches_handler`) to prevent
    /// unbounded memory growth in long-running deployments.
    ///
    /// Note: Pending-but-expired entries are NOT removed here — they are lazily
    /// marked Expired on the next `list_pending()` call, then cleaned up on the
    /// *following* `gc()` call.  This gives the handler one more chance to
    /// surface them with an `"expired"` status before they vanish.
    pub fn gc(&self) -> usize {
        let mut store = self.0.lock().unwrap();
        let before = store.len();
        store.retain(|_, p| p.status == PatchStatus::Pending);
        before - store.len()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir { TempDir::new().unwrap() }

    #[test]
    fn propose_returns_id() {
        let d = tmp();
        let store = PatchStore::new();
        let id = store
            .propose(d.path(), "new_file.rs", "Add greeting fn", "pub fn greet() {}\n")
            .unwrap();
        assert!(id.starts_with("patch_"), "id: {id}");
    }

    #[test]
    fn approve_writes_file() {
        let d = tmp();
        let store = PatchStore::new();
        let id = store
            .propose(d.path(), "hello.rs", "Add hello fn", "pub fn hello() {}\n")
            .unwrap();

        store.approve(&id, d.path()).unwrap();

        let written = std::fs::read_to_string(d.path().join("hello.rs")).unwrap();
        assert!(written.contains("hello"));
    }

    #[test]
    fn approve_twice_fails() {
        let d = tmp();
        let store = PatchStore::new();
        let id = store.propose(d.path(), "f.rs", "desc", "content").unwrap();
        store.approve(&id, d.path()).unwrap();
        assert!(store.approve(&id, d.path()).is_err());
    }

    #[test]
    fn reject_prevents_write() {
        let d = tmp();
        let store = PatchStore::new();
        let id = store.propose(d.path(), "r.rs", "desc", "content").unwrap();
        store.reject(&id).unwrap();
        assert!(!d.path().join("r.rs").exists());
    }

    #[test]
    fn reject_twice_fails() {
        let d = tmp();
        let store = PatchStore::new();
        let id = store.propose(d.path(), "r.rs", "desc", "content").unwrap();
        store.reject(&id).unwrap();
        assert!(store.reject(&id).is_err());
    }

    #[test]
    fn propose_traversal_rejected() {
        let d = tmp();
        let store = PatchStore::new();
        assert!(store
            .propose(d.path(), "../../evil.rs", "evil", "code")
            .is_err());
    }

    #[test]
    fn list_pending_excludes_approved() {
        let d = tmp();
        let store = PatchStore::new();
        let id1 = store.propose(d.path(), "a.rs", "d", "c").unwrap();
        let _id2 = store.propose(d.path(), "b.rs", "d", "c").unwrap();
        store.approve(&id1, d.path()).unwrap();

        let pending = store.list_pending();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].relative_path.contains("b.rs"));
    }

    #[test]
    fn approve_creates_nested_dirs() {
        let d = tmp();
        let store = PatchStore::new();
        let id = store
            .propose(d.path(), "deep/nested/file.rs", "desc", "code")
            .unwrap();
        store.approve(&id, d.path()).unwrap();
        assert!(d.path().join("deep/nested/file.rs").exists());
    }
}
