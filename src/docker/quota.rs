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

// ── ZFS quota support ─────────────────────────────────────────────────────────

/// Returns the ZFS dataset name whose mountpoint is a prefix of `path`.
/// Picks the longest matching mount (most specific dataset).
///
/// In `/proc/mounts`, ZFS lines look like:
/// ```
/// pool/datasets/foo  /mnt/foo  zfs  rw,...  0 0
/// ```
/// The first field is the dataset name and the second is the mount point.
pub fn zfs_dataset_for(path: &Path) -> Option<String> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let abs_str = abs.to_string_lossy();
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;

    let mut best: Option<(usize, String)> = None;
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let dataset   = parts.next()?;
        let mount     = parts.next()?;
        let fstype    = parts.next()?;
        if fstype != "zfs" { continue; }

        let m = if mount == "/" { "/".to_string() } else { format!("{}/", mount) };
        let a = if abs_str.ends_with('/') { abs_str.to_string() } else { format!("{}/", &*abs_str) };
        if !a.starts_with(&m) { continue; }

        if best.as_ref().map_or(true, |(len, _)| mount.len() > *len) {
            best = Some((mount.len(), dataset.to_string()));
        }
    }
    best.map(|(_, d)| d)
}

/// Creates a ZFS child dataset for `volume_path` and sets a `refquota` on it.
///
/// - `volume_path` — absolute path of the per-container volume directory.
///   The parent directory is used to locate the parent ZFS dataset.
///   The last component of `volume_path` is used as the child dataset name.
/// - `limit_bytes` — hard quota in bytes applied via `zfs set refquota=`.
///
/// `zfs create` will also create the mount point directory automatically.
pub async fn apply_zfs_quota(volume_path: &Path, limit_bytes: u64) -> Result<()> {
    let parent = volume_path.parent()
        .ok_or_else(|| anyhow::anyhow!("ZFS: volume_path has no parent: {:?}", volume_path))?;
    let child_name = volume_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("ZFS: volume_path has no file name: {:?}", volume_path))?;

    let parent_dataset = zfs_dataset_for(parent)
        .ok_or_else(|| anyhow::anyhow!("ZFS: parent {:?} is not under a ZFS mount", parent))?;

    let child_dataset = format!("{}/{}", parent_dataset, child_name);

    // Create the child dataset (this also creates the mountpoint directory)
    let out = Command::new("zfs")
        .args(["create", &child_dataset])
        .output()
        .await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // "dataset already exists" is fine — just continue to set refquota
        if !stderr.contains("already exists") {
            bail!("zfs create {}: {}", child_dataset, stderr.trim());
        }
    }

    // Set refquota
    let out = Command::new("zfs")
        .args(["set", &format!("refquota={}", limit_bytes), &child_dataset])
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "zfs set refquota={} {}: {}",
            limit_bytes,
            child_dataset,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    info!(
        "ZFS quota applied: dataset={} path={:?} refquota={}",
        child_dataset, volume_path, limit_bytes
    );
    Ok(())
}

/// Destroys the ZFS child dataset for `volume_path`.
/// Best-effort, does not fail if the dataset does not exist.
pub async fn remove_zfs_quota(volume_path: &Path) {
    let parent = match volume_path.parent() {
        Some(p) => p,
        None => return,
    };
    let child_name = match volume_path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return,
    };
    let parent_dataset = match zfs_dataset_for(parent) {
        Some(d) => d,
        None => return,
    };
    let child_dataset = format!("{}/{}", parent_dataset, child_name);
    let _ = Command::new("zfs")
        .args(["destroy", &child_dataset])
        .output()
        .await;
}

// ── Btrfs subvolume quota support ─────────────────────────────────────────────

/// Detects whether `path` is on a Btrfs filesystem.
/// Returns the mount point of the Btrfs volume if found.
pub fn btrfs_mount_for(path: &Path) -> Option<String> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let abs_str = abs.to_string_lossy();
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;

    let mut best: Option<(usize, String)> = None;
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let _dev   = parts.next()?;
        let mount  = parts.next()?;
        let fstype = parts.next()?;
        if fstype != "btrfs" { continue; }

        let m = if mount == "/" { "/".to_string() } else { format!("{}/", mount) };
        let a = if abs_str.ends_with('/') { abs_str.to_string() } else { format!("{}/", &*abs_str) };
        if !a.starts_with(&m) { continue; }

        if best.as_ref().map_or(true, |(len, _)| mount.len() > *len) {
            best = Some((mount.len(), mount.to_string()));
        }
    }
    best.map(|(_, m)| m)
}

/// Creates a Btrfs subvolume for `volume_path` and enforces a quota via a qgroup.
///
/// Steps:
/// 1. Enable quotas on the Btrfs filesystem (`btrfs quota enable <mount>`).
/// 2. Create the subvolume with `btrfs subvolume create <path>`.
/// 3. Obtain the subvolume id (`btrfs subvolume show <path>`).
/// 4. Set the exclusive limit on the auto-created qgroup (`btrfs qgroup limit <bytes> <path>`).
pub async fn apply_btrfs_quota(volume_path: &Path, limit_bytes: u64) -> Result<()> {
    let mount = btrfs_mount_for(volume_path).ok_or_else(|| {
        anyhow::anyhow!("Path {:?} is not on a Btrfs filesystem", volume_path)
    })?;

    // Enable quotas (idempotent — safe to run if already enabled)
    let _ = Command::new("btrfs")
        .args(["quota", "enable", &mount])
        .output()
        .await;

    let path_str = volume_path.to_string_lossy().to_string();

    // Create the subvolume (this also creates the directory)
    let out = Command::new("btrfs")
        .args(["subvolume", "create", &path_str])
        .output()
        .await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains("exists") {
            bail!("btrfs subvolume create {}: {}", path_str, stderr.trim());
        }
    }

    // Set exclusive (hard) quota on the subvolume path
    let out = Command::new("btrfs")
        .args(["qgroup", "limit", &limit_bytes.to_string(), &path_str])
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "btrfs qgroup limit {} {}: {}",
            limit_bytes, path_str,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    info!(
        "Btrfs quota applied: path={:?} limit={} mount={}",
        volume_path, limit_bytes, mount
    );
    Ok(())
}

/// Destroys the Btrfs subvolume for `volume_path`, removing quota automatically.
/// Best-effort.
pub async fn remove_btrfs_quota(volume_path: &Path) {
    let path_str = volume_path.to_string_lossy().to_string();
    let _ = Command::new("btrfs")
        .args(["subvolume", "delete", &path_str])
        .output()
        .await;
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
    if let Some(mount) = ext4_pquota_mount(volume_path) {
        let _ = Command::new("setquota")
            .args([
                "-P",
                &project_id.to_string(),
                "0", "0", "0", "0",
                &mount,
            ])
            .output()
            .await;
    }
}
