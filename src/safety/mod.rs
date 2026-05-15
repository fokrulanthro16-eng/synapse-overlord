#![allow(dead_code)]

/// Risk classification for a command string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRisk {
    /// Read-only or harmless — no confirmation needed.
    Safe,
    /// State-changing but not system-destructive — show to user and confirm.
    NeedsApproval,
    /// Dangerous or system-destructive — must never auto-execute.
    Blocked,
}

impl CommandRisk {
    pub fn label(self) -> &'static str {
        match self {
            CommandRisk::Safe => "safe",
            CommandRisk::NeedsApproval => "needs-approval",
            CommandRisk::Blocked => "BLOCKED",
        }
    }

    pub fn is_blocked(self) -> bool {
        matches!(self, CommandRisk::Blocked)
    }

    pub fn requires_approval(self) -> bool {
        !matches!(self, CommandRisk::Safe)
    }
}

/// Classify `command` by risk level.
/// Normalises to lowercase before matching — callers do not need to pre-process.
pub fn classify_command(command: &str) -> CommandRisk {
    let lower = command.trim().to_lowercase();
    if is_blocked(&lower) {
        CommandRisk::Blocked
    } else if needs_approval(&lower) {
        CommandRisk::NeedsApproval
    } else {
        CommandRisk::Safe
    }
}

// ── Blocked ──────────────────────────────────────────────────────────────────

/// Substrings whose presence anywhere in the command makes it Blocked.
const BLOCKED_SUBSTRINGS: &[&str] = &[
    // Recursive / force deletion (Unix style)
    "rm -rf",
    "rm -fr",
    "rm -r -f",
    "rm -f -r",
    "rm --no-preserve-root",
    // Windows recursive deletion flags
    "del /s",
    "del /q",
    // Low-level disk / partition tools
    "diskpart",
    "mkfs",
    "dd if=",
    // System power state
    "shutdown",
    "reboot",
    // Registry destructive ops
    "reg delete",
    "reg add",
    // ACL / ownership — can lock users out of system
    "takeown",
    "icacls",
    // Secure wipe
    "cipher /w",
    // PowerShell encoded or policy-bypassed execution
    "powershell -encodedcommand",
    "powershell -enc ",
    "pwsh -encodedcommand",
    "pwsh -enc ",
    "-executionpolicy bypass",
];

/// Path fragments whose presence anywhere in the command makes it Blocked.
/// Covers both backslash and forward-slash variants (after lowercasing).
const BLOCKED_SYSTEM_PATHS: &[&str] = &[
    // Windows system trees
    "c:\\windows",
    "c:/windows",
    "%systemroot%",
    "%windir%",
    "system32",
    "syswow64",
    "c:\\program files",
    "c:/program files",
    // Unix system trees (WSL / cross-platform)
    "/etc/",
    "/usr/bin",
    "/usr/sbin",
    "/bin/",
    "/sbin/",
    "/boot/",
];

fn is_blocked(cmd: &str) -> bool {
    // Any reference to a protected system path is unconditionally blocked
    if BLOCKED_SYSTEM_PATHS.iter().any(|p| cmd.contains(p)) {
        return true;
    }

    if BLOCKED_SUBSTRINGS.iter().any(|p| cmd.contains(p)) {
        return true;
    }

    // `format` as the leading word blocks disk-format commands while
    // allowing `cargo fmt`, `rustfmt`, `git format-patch`, etc.
    if first_word(cmd) == "format" {
        return true;
    }

    false
}

// ── Needs approval ────────────────────────────────────────────────────────────

/// Substrings that flag a command as requiring explicit user confirmation.
const APPROVAL_SUBSTRINGS: &[&str] = &[
    // File deletion (non-recursive single files)
    "rm ",
    "del ",
    "remove-item",
    "rmdir",
    "rd ",
    // Git state-mutating operations
    "git commit",
    "git push",
    "git reset",
    "git clean",
    "git rebase",
    "git merge",
    "git stash drop",
    // Package / system installation
    "cargo install",
    "npm install",
    "pip install",
    "choco install",
    "winget install",
    "scoop install",
    // Network / service management
    "netsh",
    "net user",
    "net localgroup",
    "sc stop",
    "sc delete",
    "sc create",
    "taskkill",
    // Registry read/export (writes already Blocked above)
    "reg query",
    "reg export",
    // Any PowerShell invocation not already Blocked above
    "powershell",
    "pwsh",
];

fn needs_approval(cmd: &str) -> bool {
    APPROVAL_SUBSTRINGS.iter().any(|p| cmd.contains(p))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn first_word(cmd: &str) -> &str {
    cmd.split_whitespace().next().unwrap_or("")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_commands() {
        assert_eq!(classify_command("cargo check"), CommandRisk::Safe);
        assert_eq!(classify_command("git status"), CommandRisk::Safe);
        assert_eq!(classify_command("git log --oneline"), CommandRisk::Safe);
        assert_eq!(classify_command("cargo test"), CommandRisk::Safe);
        assert_eq!(classify_command("rustfmt src/main.rs"), CommandRisk::Safe);
        assert_eq!(classify_command("cargo fmt"), CommandRisk::Safe);
        assert_eq!(classify_command("ls -la"), CommandRisk::Safe);
        assert_eq!(classify_command("dir"), CommandRisk::Safe);
    }

    #[test]
    fn blocked_commands() {
        assert_eq!(classify_command("rm -rf /"), CommandRisk::Blocked);
        assert_eq!(classify_command("del /s /q C:\\Users"), CommandRisk::Blocked);
        assert_eq!(classify_command("format C:"), CommandRisk::Blocked);
        assert_eq!(classify_command("diskpart"), CommandRisk::Blocked);
        assert_eq!(classify_command("shutdown /s /t 0"), CommandRisk::Blocked);
        assert_eq!(classify_command("reg delete HKLM\\SOFTWARE\\foo"), CommandRisk::Blocked);
        assert_eq!(classify_command("takeown /f C:\\Windows"), CommandRisk::Blocked);
        assert_eq!(classify_command("icacls C:\\Windows /grant Everyone:F"), CommandRisk::Blocked);
        assert_eq!(classify_command("cipher /w:C:\\"), CommandRisk::Blocked);
        assert_eq!(classify_command("dd if=/dev/zero of=/dev/sda"), CommandRisk::Blocked);
        assert_eq!(classify_command("Remove-Item C:\\Windows\\system32\\foo"), CommandRisk::Blocked);
    }

    #[test]
    fn approval_commands() {
        assert_eq!(classify_command("rm old_file.txt"), CommandRisk::NeedsApproval);
        assert_eq!(classify_command("del temp.txt"), CommandRisk::NeedsApproval);
        assert_eq!(classify_command("git commit -m 'feat: add thing'"), CommandRisk::NeedsApproval);
        assert_eq!(classify_command("git push origin main"), CommandRisk::NeedsApproval);
        assert_eq!(classify_command("cargo install ripgrep"), CommandRisk::NeedsApproval);
        assert_eq!(classify_command("Remove-Item myproject\\dist"), CommandRisk::NeedsApproval);
    }
}
