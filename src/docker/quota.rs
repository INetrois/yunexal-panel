use std::path::Path;
use tokio::process::Command;
use tracing::{info, warn};
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

// ── ext4 project quota support ────────────────────────────────────────────────

/// Finds an ext4 mount point for `path` that has project quota enabled
/// (`prjquota` or `prjjquota` mount option). Returns `None` if not found.
pub fn ext4_pquota_mount(path: &Path) -> Option<String> {
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

        if fstype != "ext4" {
            continue;
        }
        let has_pquota = opts.split(',').any(|o| o == "prjquota" || o == "prjjquota");
        if !has_pquota {
            continue;
        }

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

fn ext4_pquota_mounts() -> Vec<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut out: Vec<String> = Vec::new();

    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let _dev = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let mount = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let fstype = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let opts = parts.next().unwrap_or("");

        if fstype != "ext4" {
            continue;
        }

        let has_pquota = opts.split(',').any(|o| o == "prjquota" || o == "prjjquota");
        if !has_pquota {
            continue;
        }

        if !out.iter().any(|m| m == mount) {
            out.push(mount.to_string());
        }
    }

    out
}

async fn clear_ext4_project_quota_on_mount(project_id: u32, mount: &str) -> bool {
    match Command::new("setquota")
        .args([
            "-P",
            &project_id.to_string(),
            "0", "0", "0", "0",
            mount,
        ])
        .output()
        .await
    {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if !stderr.is_empty() {
                warn!(
                    "ext4 quota clear failed: project={} mount={} err={}",
                    project_id,
                    mount,
                    stderr
                );
            }
            false
        }
        Err(e) => {
            warn!(
                "ext4 quota clear command failed: project={} mount={} err={}",
                project_id,
                mount,
                e
            );
            false
        }
    }
}

/// Applies an ext4 project quota to `volume_path`.
///
/// - `project_id` — should be the server's db_id cast to u32; used as the project number.
/// - `limit_bytes` — hard block quota.
///
/// Requires:
/// - The filesystem under `volume_path` is ext4 mounted with `prjquota` or `prjjquota`.
/// - The process has root access to run `chattr` and `setquota`.
pub async fn apply_ext4_quota(volume_path: &Path, project_id: u32, limit_bytes: u64) -> Result<()> {
    let mount = ext4_pquota_mount(volume_path).ok_or_else(|| {
        anyhow::anyhow!(
            "Path {:?} is not on an ext4 filesystem with prjquota/prjjquota",
            volume_path
        )
    })?;

    let path_str = volume_path.to_string_lossy().to_string();

    // Associate the directory tree with the project ID
    let out = Command::new("chattr")
        .args(["+P", "-p", &project_id.to_string(), &path_str])
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "chattr +P -p {} {}: {}",
            project_id,
            path_str,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    // Set hard block quota (setquota uses kilobytes)
    let limit_kb = (limit_bytes + 1023) / 1024;
    // setquota -P <projid> <block_soft> <block_hard> <inode_soft> <inode_hard> <mount>
    let out = Command::new("setquota")
        .args([
            "-P",
            &project_id.to_string(),
            "0",
            &limit_kb.to_string(),
            "0", "0",
            &mount,
        ])
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "setquota -P {} 0 {} 0 0 {}: {}",
            project_id,
            limit_kb,
            mount,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    info!(
        "ext4 quota applied: project={} path={:?} limit={}k mount={}",
        project_id, volume_path, limit_kb, mount
    );
    Ok(())
}

/// Removes ext4 project quota for `project_id`. Resets block limit to zero.
/// Best-effort.
pub async fn remove_ext4_quota(project_id: u32, volume_path: &Path) {
    let mut mounts: Vec<String> = Vec::new();

    if let Some(primary) = ext4_pquota_mount(volume_path) {
        mounts.push(primary);
    }

    for m in ext4_pquota_mounts() {
        if !mounts.iter().any(|x| x == &m) {
            mounts.push(m);
        }
    }

    if mounts.is_empty() {
        warn!(
            "ext4 quota clear skipped: project={} path={:?} no ext4 prjquota mounts detected",
            project_id,
            volume_path
        );
        return;
    }

    let mut cleared_any = false;
    for mount in mounts {
        if clear_ext4_project_quota_on_mount(project_id, &mount).await {
            cleared_any = true;
        }
    }

    if cleared_any {
        info!(
            "ext4 quota cleared: project={} path={:?}",
            project_id,
            volume_path
        );
    }
}
