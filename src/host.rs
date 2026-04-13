use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitSystem {
    Systemd,
    OpenRc,
    SysV,
}

fn find_in_path(cmd: &str) -> Option<PathBuf> {
    if cmd.contains('/') {
        let p = PathBuf::from(cmd);
        return if p.exists() { Some(p) } else { None };
    }

    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(cmd);
        if full.exists() {
            return Some(full);
        }
    }
    None
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("yes")
                || v.eq_ignore_ascii_case("on")
                || v == "1"
        })
        .unwrap_or(false)
}

fn parse_effective_uid_from_status(status: &str) -> Option<u32> {
    status
        .lines()
        .find(|line| line.starts_with("Uid:"))?
        .split_whitespace()
        .nth(2)
        .and_then(|s| s.parse().ok())
}

pub fn resolve_command_path(cmd: &str, absolute_fallbacks: &[&str]) -> String {
    if let Some(path) = find_in_path(cmd) {
        return path.to_string_lossy().to_string();
    }

    for candidate in absolute_fallbacks {
        if Path::new(candidate).exists() {
            return (*candidate).to_string();
        }
    }

    absolute_fallbacks
        .first()
        .copied()
        .unwrap_or(cmd)
        .to_string()
}

pub fn resolve_admin_tool(tool: &str) -> String {
    match tool {
        "sudo" => resolve_command_path("sudo", &["/usr/bin/sudo", "/bin/sudo"]),
        "ufw" => resolve_command_path("ufw", &["/usr/sbin/ufw", "/sbin/ufw", "/usr/bin/ufw", "/bin/ufw"]),
        "systemctl" => resolve_command_path("systemctl", &["/usr/bin/systemctl", "/bin/systemctl"]),
        "rc-service" => resolve_command_path("rc-service", &["/sbin/rc-service", "/usr/sbin/rc-service"]),
        "service" => resolve_command_path(
            "service",
            &["/usr/sbin/service", "/sbin/service", "/usr/bin/service", "/bin/service"],
        ),
        "tee" => resolve_command_path("tee", &["/usr/bin/tee", "/bin/tee"]),
        "mount" => resolve_command_path("mount", &["/usr/bin/mount", "/bin/mount", "/usr/sbin/mount"]),
        "umount" => resolve_command_path("umount", &["/usr/bin/umount", "/bin/umount", "/usr/sbin/umount"]),
        "rsync" => resolve_command_path("rsync", &["/usr/bin/rsync", "/bin/rsync"]),
        "cp" => resolve_command_path("cp", &["/usr/bin/cp", "/bin/cp"]),
        "mkdir" => resolve_command_path("mkdir", &["/usr/bin/mkdir", "/bin/mkdir"]),
        "mkfs.ext4" => resolve_command_path("mkfs.ext4", &["/usr/sbin/mkfs.ext4", "/sbin/mkfs.ext4"]),
        "mkfs.xfs" => resolve_command_path("mkfs.xfs", &["/usr/sbin/mkfs.xfs", "/sbin/mkfs.xfs"]),
        "mkfs.btrfs" => resolve_command_path("mkfs.btrfs", &["/usr/sbin/mkfs.btrfs", "/sbin/mkfs.btrfs"]),
        "zfs" => resolve_command_path("zfs", &["/usr/sbin/zfs", "/sbin/zfs", "/usr/bin/zfs"]),
        "zpool" => resolve_command_path("zpool", &["/usr/sbin/zpool", "/sbin/zpool", "/usr/bin/zpool"]),
        "btrfs" => resolve_command_path("btrfs", &["/usr/bin/btrfs", "/sbin/btrfs"]),
        _ => resolve_command_path(tool, &[]),
    }
}

pub fn detect_init_system() -> InitSystem {
    let has_systemd_runtime = Path::new("/run/systemd/system").exists();
    let has_systemctl = find_in_path("systemctl").is_some();
    if has_systemd_runtime && has_systemctl {
        return InitSystem::Systemd;
    }

    let has_openrc = find_in_path("rc-service").is_some() || Path::new("/sbin/openrc").exists();
    if has_openrc {
        return InitSystem::OpenRc;
    }

    InitSystem::SysV
}

pub fn is_effective_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| parse_effective_uid_from_status(&s))
        .map(|uid| uid == 0)
        .unwrap_or_else(|| std::env::var("USER").map(|u| u == "root").unwrap_or(false))
}

pub fn should_use_sudo() -> bool {
    let has_sudo = find_in_path("sudo").is_some()
        || Path::new("/usr/bin/sudo").exists()
        || Path::new("/bin/sudo").exists();

    if env_truthy("YUNEXAL_NO_SUDO") {
        return false;
    }
    if env_truthy("YUNEXAL_FORCE_SUDO") {
        return has_sudo;
    }

    !is_effective_root() && has_sudo
}

pub fn privileged_command(program: &str) -> tokio::process::Command {
    if should_use_sudo() {
        let mut cmd = tokio::process::Command::new(resolve_admin_tool("sudo"));
        cmd.arg("-n").arg(program);
        cmd
    } else {
        tokio::process::Command::new(program)
    }
}

pub fn docker_restart_command_parts() -> (String, Vec<String>) {
    match detect_init_system() {
        InitSystem::Systemd => (
            resolve_admin_tool("systemctl"),
            vec!["restart".to_string(), "docker".to_string()],
        ),
        InitSystem::OpenRc => (
            resolve_admin_tool("rc-service"),
            vec!["docker".to_string(), "restart".to_string()],
        ),
        InitSystem::SysV => (
            resolve_admin_tool("service"),
            vec!["docker".to_string(), "restart".to_string()],
        ),
    }
}

pub fn docker_restart_display_command() -> String {
    match detect_init_system() {
        InitSystem::Systemd => "systemctl restart docker".to_string(),
        InitSystem::OpenRc => "rc-service docker restart".to_string(),
        InitSystem::SysV => "service docker restart".to_string(),
    }
}

pub fn docker_restart_sudoers_entry() -> String {
    let (program, args) = docker_restart_command_parts();
    if args.is_empty() {
        program
    } else {
        format!("{} {}", program, args.join(" "))
    }
}

pub fn ufw_sudoers_entry() -> String {
    resolve_admin_tool("ufw")
}

pub fn sudoers_fix_command(user: &str, sudoers_file: &str, entries: &[String]) -> String {
    let safe_user = user.replace('\'', "");
    let allow = entries.join(", ");
    format!(
        "echo '{safe_user} ALL=(ALL) NOPASSWD: {allow}' | sudo tee {sudoers_file} && sudo chmod 440 {sudoers_file}"
    )
}
