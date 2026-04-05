use std::path::Path;
use tokio::process::Command;
use tracing::info;
use anyhow::{bail, Result};

// ── Size parsing ──────────────────────────────────────────────────────────────

/// Parses a human-readable size string ("10g", "500m", "2048k") into bytes.
/// Returns `None` if the string is empty, zero, or unparseable.
pub fn parse_disk_limit(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    let digits_end = s.find(|c: char| !c.is_numeric() && c != '.').unwrap_or(s.len());
    let (num_str, unit) = s.split_at(digits_end);
    let num: f64 = num_str.parse().ok()?;
    if num <= 0.0 {
        return None;
    }
    Some(match unit.trim() {
        "gb" | "g" => (num * 1024.0 * 1024.0 * 1024.0) as u64,
        "mb" | "m" => (num * 1024.0 * 1024.0) as u64,
        "kb" | "k" => (num * 1024.0) as u64,
        _           => num as u64,
    })
}

// ── Filesystem detection ──────────────────────────────────────────────────────

/// Finds the XFS mount point for `path` that has project quota enabled
/// (`pquota` or `prjquota` mount option). Returns `None` if not found.
pub fn xfs_pquota_mount(path: &Path) -> Option<String> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let abs_str = abs.to_string_lossy();
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;

    let mut best: Option<(usize, String)> = None;
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let _dev   = parts.next()?;
        let mount  = parts.next()?;
        let fstype = parts.next()?;
        let opts   = parts.next().unwrap_or("");

        if fstype != "xfs" {
            continue;
        }
        let has_pquota = opts.split(',').any(|o| o == "pquota" || o == "prjquota");
        if !has_pquota {
            continue;
        }

        // Normalize trailing slash for prefix comparison
        let m = if mount == "/" { "/".to_string() } else { format!("{}/", mount) };
        let a = if abs_str.ends_with('/') {
            abs_str.to_string()
        } else {
            format!("{}/", &*abs_str)
        };
        if !a.starts_with(&m) {
            continue;
        }

        if best.as_ref().map_or(true, |(len, _)| mount.len() > *len) {
            best = Some((mount.len(), mount.to_string()));
        }
    }
    best.map(|(_, m)| m)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn ensure_file(path: &str) {
    if !std::path::Path::new(path).exists() {
        let _ = std::fs::write(path, "");
    }
}

fn has_project_entry(file: &str, prefix: &str) -> bool {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .any(|l| l.starts_with(prefix))
}

fn append_line(file: &str, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?;
    writeln!(f, "{}", line)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Applies an XFS project quota to `volume_path`.
///
/// - `project_id` — should be the server's db_id cast to u32; used as the XFS project number.
/// - `limit_bytes` — hard (= soft) block quota.
///
/// Requires:
/// - The filesystem under `volume_path` is XFS mounted with `pquota` or `prjquota`.
/// - The process has write access to `/etc/projects` and `/etc/projid` and can run `xfs_quota -x`.
pub async fn apply_xfs_quota(volume_path: &Path, project_id: u32, limit_bytes: u64) -> Result<()> {
    let mount = xfs_pquota_mount(volume_path).ok_or_else(|| {
        anyhow::anyhow!(
            "Path {:?} is not on an XFS filesystem with pquota/prjquota",
            volume_path
        )
    })?;

    ensure_file("/etc/projects");
    ensure_file("/etc/projid");

    let proj_prefix = format!("{}:", project_id);
    let proj_name   = format!("yunexal_{}", project_id);
    let id_prefix   = format!("{}:", proj_name);

    if !has_project_entry("/etc/projects", &proj_prefix) {
        append_line("/etc/projects", &format!("{}:{}", project_id, volume_path.display()))?;
    }
    if !has_project_entry("/etc/projid", &id_prefix) {
        append_line("/etc/projid", &format!("{}:{}", proj_name, project_id))?;
    }

    // Initialize project (associates the directory tree with the project ID)
    let out = Command::new("xfs_quota")
        .args(["-x", "-c", &format!("project -s {}", project_id), &mount])
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "xfs_quota project -s {}: {}",
            project_id,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    // Set hard = soft block limit (xfs_quota uses kilobytes with 'k' suffix)
    let limit_kb = (limit_bytes + 1023) / 1024;
    let out = Command::new("xfs_quota")
        .args([
            "-x", "-c",
            &format!("limit -p bhard={}k bsoft={}k {}", limit_kb, limit_kb, project_id),
            &mount,
        ])
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "xfs_quota limit project {}: {}",
            project_id,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    info!(
        "XFS quota applied: project={} path={:?} limit={}k mount={}",
        project_id, volume_path, limit_kb, mount
    );
    Ok(())
}

/// Removes XFS project quota entries for `project_id`.
/// Best-effort: first resets the limit to 0, then cleans up `/etc/projects` and `/etc/projid`.
pub async fn remove_xfs_quota(project_id: u32, volume_path: &Path) {
    if let Some(mount) = xfs_pquota_mount(volume_path) {
        let _ = Command::new("xfs_quota")
            .args([
                "-x", "-c",
                &format!("limit -p bhard=0k bsoft=0k {}", project_id),
                &mount,
            ])
            .output()
            .await;
    }

    let proj_prefix = format!("{}:", project_id);
    if let Ok(content) = std::fs::read_to_string("/etc/projects") {
        let filtered: String = content
            .lines()
            .filter(|l| !l.starts_with(&proj_prefix))
            .flat_map(|l| [l, "\n"])
            .collect();
        let _ = std::fs::write("/etc/projects", filtered);
    }

    let name_prefix = format!("yunexal_{}:", project_id);
    if let Ok(content) = std::fs::read_to_string("/etc/projid") {
        let filtered: String = content
            .lines()
            .filter(|l| !l.starts_with(&name_prefix))
            .flat_map(|l| [l, "\n"])
            .collect();
        let _ = std::fs::write("/etc/projid", filtered);
    }
}
