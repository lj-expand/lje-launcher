//! Handles operations using the git CLI

use std::os::windows::process::CommandExt;
use std::path::Path;

pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: i32
}

pub fn git(dir: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0") // Disable git locks to avoid issues with concurrent operations
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| format!("Failed to execute git: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    Ok(GitOutput { stdout, stderr, code })
}

/// Parsed git state for a repository, from one `status --porcelain=v1 --branch`.
#[derive(Debug, Clone)]
pub struct GitStatus {
    pub commit: Option<String>,
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub dirty: bool,
}

pub fn git_or(dir: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let out = git(dir, args)?;
    if out.code != 0 {
        let message = if out.stderr.trim().is_empty() {
            out.stdout.trim().to_string()
        } else {
            out.stderr.trim().to_string()
        };
        return Err(if message.is_empty() {
            format!("git {} exited with {}", args.join(" "), out.code)
        } else {
            message
        });
    }
    Ok(out)
}

/// Local git state (no network): branch, ahead/behind vs the local tracking
/// ref, dirty flag, and short commit.
pub fn status(dir: &Path) -> Result<GitStatus, String> {
    let commit = git(dir, &["rev-parse", "--short", "HEAD"])
        .ok()
        .filter(|o| o.code == 0)
        .map(|o| o.stdout.trim().to_string());

    let out = git_or(dir, &["status", "--porcelain=v1", "--branch"])?;

    let mut branch = None;
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut dirty = false;

    for line in out.stdout.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            // "main...origin/main [ahead 1, behind 2]" | "main" | "No commits yet on main"
            let (b, upstream) = match rest.split_once("...") {
                Some((b, up)) => (b, Some(up)),
                None => (rest, None),
            };
            branch = Some(b.to_string());
            if let Some(up) = upstream {
                if let Some(start) = up.find('[') {
                    let end = up[start + 1..].find(']').map(|i| start + 1 + i).unwrap_or(up.len());
                    for part in up[start + 1..end].split(',') {
                        let part = part.trim();
                        if let Some(n) = part.strip_prefix("ahead ") {
                            ahead = n.trim().parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix("behind ") {
                            behind = n.trim().parse().unwrap_or(0);
                        }
                    }
                }
            }
        } else if !line.trim().is_empty() {
            dirty = true;
        }
    }

    Ok(GitStatus {
        commit,
        branch,
        ahead,
        behind,
        dirty,
    })
}
