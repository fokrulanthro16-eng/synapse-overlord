//! Pure-Rust unified diff generator.
//!
//! Produces standard unified-diff output compatible with `patch(1)` and most
//! code-review UIs.  Uses an LCS-based edit-script algorithm.
//!
//! Design constraints:
//!   - No external crate dependencies — stdlib only.
//!   - Input capped at `MAX_INPUT_LINES` lines per file.
//!   - Output capped at `MAX_OUTPUT_LINES` lines total.
//!   - 3-line context window around each changed hunk.

const MAX_INPUT_LINES: usize = 2_000;
const MAX_OUTPUT_LINES: usize = 200;
const CONTEXT_LINES: usize = 3;

/// Generate a unified diff between `original` and `modified`.
///
/// Returns a string in standard unified-diff format, or `"(no changes)"` when
/// the two inputs are identical.  The labels appear after `---` / `+++`.
///
/// # Example
/// ```
/// let diff = generate_unified_diff(
///     "fn old() {}\n",
///     "fn new() {}\n",
///     "a/src/lib.rs",
///     "b/src/lib.rs",
/// );
/// ```
pub fn generate_unified_diff(
    original: &str,
    modified: &str,
    original_label: &str,
    modified_label: &str,
) -> String {
    let orig_v: Vec<&str> = original.lines().collect();
    let modi_v: Vec<&str> = modified.lines().collect();

    // Cap inputs to prevent excessive memory use on huge files
    let orig = &orig_v[..orig_v.len().min(MAX_INPUT_LINES)];
    let modi = &modi_v[..modi_v.len().min(MAX_INPUT_LINES)];

    if orig == modi {
        return "(no changes)".to_string();
    }

    let edits = lcs_diff(orig, modi);
    let hunks = build_hunks(&edits, orig, modi);

    let mut out = String::new();
    out.push_str(&format!("--- {}\n", original_label));
    out.push_str(&format!("+++ {}\n", modified_label));

    let mut lines_written = 2usize;
    for hunk in &hunks {
        let hunk_line_count = hunk.lines().count();
        if lines_written + hunk_line_count > MAX_OUTPUT_LINES {
            out.push_str("[diff truncated]\n");
            break;
        }
        out.push_str(hunk);
        lines_written += hunk_line_count;
    }

    out
}

// ── Edit operations ───────────────────────────────────────────────────────────

/// A line-level edit operation produced by the LCS back-trace.
#[derive(Debug)]
enum Edit {
    /// Line present in both files (context).  Carries (orig_idx, mod_idx).
    Keep(usize, usize),
    /// Line deleted from the original.  Carries orig_idx.
    Remove(usize),
    /// Line inserted in the modified.  Carries mod_idx.
    Add(usize),
}

// ── LCS-based diff algorithm ──────────────────────────────────────────────────

/// Compute a list of `Edit` operations via an LCS DP table + back-trace.
fn lcs_diff(orig: &[&str], modi: &[&str]) -> Vec<Edit> {
    let m = orig.len();
    let n = modi.len();

    // dp[i][j] = LCS length of orig[i..] × modi[j..]
    let mut dp = vec![vec![0u32; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            dp[i][j] = if orig[i] == modi[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    // Back-trace to build the edit sequence
    let mut edits = Vec::with_capacity(m + n);
    let (mut i, mut j) = (0usize, 0usize);
    while i < m || j < n {
        if i < m && j < n && orig[i] == modi[j] {
            // Both lines match — context
            edits.push(Edit::Keep(i, j));
            i += 1; j += 1;
        } else if i < m && (j >= n || dp[i + 1][j] >= dp[i][j + 1]) {
            // orig[i] is not in the LCS — remove it
            edits.push(Edit::Remove(i));
            i += 1;
        } else {
            // modi[j] is not in the LCS — add it
            edits.push(Edit::Add(j));
            j += 1;
        }
    }

    edits
}

// ── Hunk building ─────────────────────────────────────────────────────────────

/// Group the flat edit list into unified-diff hunk strings.
fn build_hunks(edits: &[Edit], orig: &[&str], modi: &[&str]) -> Vec<String> {
    // Positions in `edits` that represent actual changes (not Keep)
    let changes: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter(|(_, e)| !matches!(e, Edit::Keep(_, _)))
        .map(|(i, _)| i)
        .collect();

    if changes.is_empty() {
        return vec![];
    }

    // Merge nearby change positions into hunk ranges [op_start, op_end]
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let (mut rs, mut re) = (changes[0], changes[0]);
    for &c in &changes[1..] {
        if c <= re + 2 * CONTEXT_LINES + 1 {
            re = c; // close enough — extend current hunk
        } else {
            ranges.push((rs, re));
            rs = c; re = c;
        }
    }
    ranges.push((rs, re));

    // Render each hunk
    ranges
        .into_iter()
        .map(|(hstart, hend)| {
            let op_start = hstart.saturating_sub(CONTEXT_LINES);
            let op_end   = (hend + CONTEXT_LINES + 1).min(edits.len());
            let slice    = &edits[op_start..op_end];

            // Count lines contributed to each file side
            let orig_count: usize = slice
                .iter()
                .filter(|e| matches!(e, Edit::Keep(_, _) | Edit::Remove(_)))
                .count();
            let mod_count: usize = slice
                .iter()
                .filter(|e| matches!(e, Edit::Keep(_, _) | Edit::Add(_)))
                .count();

            // 1-indexed start line in each file (0 if the hunk begins with a pure Add)
            let orig_start = slice
                .iter()
                .find_map(|e| match e {
                    Edit::Keep(oi, _) | Edit::Remove(oi) => Some(oi + 1),
                    _ => None,
                })
                .unwrap_or(0);
            let mod_start = slice
                .iter()
                .find_map(|e| match e {
                    Edit::Keep(_, mi) | Edit::Add(mi) => Some(mi + 1),
                    _ => None,
                })
                .unwrap_or(0);

            // Build hunk body
            let mut body = format!(
                "@@ -{},{} +{},{} @@\n",
                orig_start, orig_count, mod_start, mod_count,
            );
            for edit in slice {
                match edit {
                    Edit::Keep(oi, _) => {
                        body.push(' ');
                        body.push_str(orig[*oi]);
                        body.push('\n');
                    }
                    Edit::Remove(oi) => {
                        body.push('-');
                        body.push_str(orig[*oi]);
                        body.push('\n');
                    }
                    Edit::Add(mi) => {
                        body.push('+');
                        body.push_str(modi[*mi]);
                        body.push('\n');
                    }
                }
            }
            body
        })
        .collect()
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_inputs_returns_sentinel() {
        let r = generate_unified_diff("hello\nworld\n", "hello\nworld\n", "a", "b");
        assert_eq!(r, "(no changes)");
    }

    #[test]
    fn replacement_shows_minus_and_plus() {
        let r = generate_unified_diff("a\nb\nc\n", "a\nB\nc\n", "a/f", "b/f");
        assert!(r.contains("-b\n"), "missing -b\n: {}", r);
        assert!(r.contains("+B\n"), "missing +B\n: {}", r);
        assert!(r.contains(" a\n"), "missing context a\n: {}", r);
        assert!(r.contains(" c\n"), "missing context c\n: {}", r);
    }

    #[test]
    fn pure_addition() {
        let r = generate_unified_diff("a\nb\n", "a\nb\nc\n", "a/f", "b/f");
        assert!(r.contains("+c\n"), "missing +c: {}", r);
        assert!(!r.contains("-c\n"), "spurious -c: {}", r);
    }

    #[test]
    fn pure_deletion() {
        let r = generate_unified_diff("a\nb\nc\n", "a\nc\n", "a/f", "b/f");
        assert!(r.contains("-b\n"), "missing -b: {}", r);
        assert!(!r.contains("+b\n"), "spurious +b: {}", r);
    }

    #[test]
    fn empty_original_is_pure_add() {
        let r = generate_unified_diff("", "line1\nline2\n", "a/f", "b/f");
        assert!(r.contains("+line1\n"), "{}", r);
        assert!(r.contains("+line2\n"), "{}", r);
    }

    #[test]
    fn header_lines_present() {
        let r = generate_unified_diff("x\n", "y\n", "a/orig", "b/mod");
        assert!(r.starts_with("--- a/orig\n+++ b/mod\n"), "bad header: {}", r);
    }
}
