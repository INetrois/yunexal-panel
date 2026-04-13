// yunexal-setup — interactive setup wizard (replaces setup.sh)
// Compiled as a separate binary alongside the main yunexal-panel server.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use yunexal_panel::{db, password};

// ── Colour / print helpers ────────────────────────────────────────────────────

macro_rules! info {
    ($($t:tt)*) => { println!("\x1b[34m[INFO]\x1b[0m  {}", format!($($t)*)) };
}
macro_rules! ok {
    ($($t:tt)*) => { println!("\x1b[32m[OK]\x1b[0m    {}", format!($($t)*)) };
}
macro_rules! warn {
    ($($t:tt)*) => { println!("\x1b[33m[WARN]\x1b[0m  {}", format!($($t)*)) };
}
macro_rules! header {
    ($($t:tt)*) => { println!("\n\x1b[1m\x1b[34m══ {} ══\x1b[0m", format!($($t)*)) };
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Prompt with an optional default. Returns entered text or default.
fn prompt(question: &str, default: Option<&str>) -> Result<String> {
    let default_hint = default.map(|d| format!(" [{}]", d)).unwrap_or_default();
    print!("\x1b[34m{}{}\x1b[0m: ", question, default_hint);
    io::stdout().flush()?;
    let line = read_line()?;
    if line.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(line)
    }
}

/// Yes/No prompt. Returns `true` for `y`, `false` otherwise. `default_yes` controls
/// what happens when the user presses Enter without input.
fn prompt_yn(question: &str, default_yes: bool) -> Result<bool> {
    let hint = if default_yes { "Y/n" } else { "y/N" };
    print!("\x1b[33m{} [{}]\x1b[0m: ", question, hint);
    io::stdout().flush()?;
    let line = read_line()?.to_lowercase();
    if line.is_empty() {
        Ok(default_yes)
    } else {
        Ok(line.starts_with('y'))
    }
}

/// Read a password without echoing it to the terminal.
fn prompt_password(question: &str) -> Result<String> {
    print!("\x1b[34m{}\x1b[0m: ", question);
    io::stdout().flush()?;
    rpassword::read_password().context("Failed to read password")
}

fn read_line() -> Result<String> {
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

// ── Root check ────────────────────────────────────────────────────────────────

fn check_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

/// Returns the real invoking user (strips sudo).
fn real_user() -> String {
    std::env::var("SUDO_USER").unwrap_or_else(|_| {
        std::process::Command::new("logname")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "root".to_string())
    })
}

fn is_alpine_host() -> bool {
    Path::new("/etc/alpine-release").exists()
}

fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ensure_apk_packages(packages: &[&str]) -> Result<()> {
    let mut args = vec!["add", "--no-cache"];
    args.extend_from_slice(packages);

    let status = std::process::Command::new("apk")
        .args(args)
        .status()
        .context("Failed to execute apk add")?;

    if !status.success() {
        anyhow::bail!("apk add failed for required packages");
    }

    Ok(())
}

// ── Secret generation ─────────────────────────────────────────────────────────

/// Generates a 64-byte random hex string using /dev/urandom.
fn gen_secret() -> Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 64];
    std::fs::File::open("/dev/urandom")
        .context("Failed to open /dev/urandom")?
        .read_exact(&mut buf)
        .context("Failed to read /dev/urandom")?;
    Ok(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let opt_reset          = args.iter().any(|a| a == "--reset");
    let opt_non_interactive = args.iter().any(|a| a == "--non-interactive");

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: yunexal-setup [--reset] [--non-interactive]");
        println!();
        println!("  --reset              Wipe DB and .env without prompting");
        println!("  --non-interactive    Read credentials from PANEL_USERNAME / PANEL_PASSWORD env vars");
        return Ok(());
    }

    // ── Header ────────────────────────────────────────────────────────────────
    println!("\n\x1b[1m╔══════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1m║      Yunexal Panel — Setup Wizard        ║\x1b[0m");
    println!("\x1b[1m╚══════════════════════════════════════════╝\x1b[0m\n");

    if !check_root() {
        eprintln!("\x1b[31m[ERROR]\x1b[0m This tool must be run as root (use sudo).");
        std::process::exit(1);
    }

    if !is_alpine_host() {
        eprintln!("\x1b[31m[ERROR]\x1b[0m This setup supports Alpine Linux only (musl-only flow).");
        std::process::exit(1);
    }

    if !command_exists("rc-service") || !command_exists("rc-update") || !command_exists("apk") {
        eprintln!("\x1b[31m[ERROR]\x1b[0m Missing Alpine/OpenRC tools (rc-service, rc-update, apk).");
        std::process::exit(1);
    }

    let real_user = real_user();
    let script_dir = std::env::current_dir()
        .context("Failed to determine working directory")?;

    // ── Step 1: Reset ─────────────────────────────────────────────────────────
    header!("Step 1: Reset");

    let do_reset = if opt_reset {
        true
    } else {
        prompt_yn("Wipe existing database and .env?", false)?
    };

    if do_reset {
        step_reset(&script_dir).await;
    } else {
        info!("Skipping reset.");
    }

    // ── Step 2: Docker ────────────────────────────────────────────────────────
    header!("Step 2: Docker");
    step_docker(&real_user).await?;

    // ── Step 3: .env ─────────────────────────────────────────────────────────
    header!("Step 3: Environment (.env)");
    step_env(&script_dir, &real_user)?;

    // ── Step 4: Admin user ───────────────────────────────────────────────────
    header!("Step 4: Admin user");
    step_admin_user(opt_non_interactive, &script_dir, &real_user).await?;

    // ── Step 5: Import containers ─────────────────────────────────────────────
    header!("Step 5: Import Docker containers");
    step_import_containers(&script_dir).await;

    // ── Step 6: OpenRC service ────────────────────────────────────────────────
    header!("Step 6: OpenRC service");
    step_openrc_service(&script_dir, &real_user)?;

    // ── Step 7: nginx reverse proxy ───────────────────────────────────────────
    header!("Step 7: nginx reverse proxy");
    step_nginx(&script_dir)?;

    // ── Summary ───────────────────────────────────────────────────────────────
    let panel_port = read_env_port(&script_dir).unwrap_or_else(|| "3000".to_string());
    println!();
    println!("\x1b[1m\x1b[32m╔══════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1m\x1b[32m║            Setup complete!               ║\x1b[0m");
    println!("\x1b[1m\x1b[32m╚══════════════════════════════════════════╝\x1b[0m");
    println!();
    println!("  Panel URL  : \x1b[1mhttp://localhost:{}\x1b[0m", panel_port);
    println!("  Service    : \x1b[1mrc-service yunexal-panel status\x1b[0m");
    println!("  Logs       : \x1b[1mtail -f /var/log/yunexal-panel.log\x1b[0m");
    println!();

    Ok(())
}

// ── Step implementations ──────────────────────────────────────────────────────

async fn step_reset(dir: &Path) {
    info!("Stopping yunexal-panel service (if running)…");
    let _ = std::process::Command::new("rc-service")
        .args(["yunexal-panel", "stop"])
        .status();

    for f in &["yunexal.db", "yunexal.db-shm", "yunexal.db-wal", ".env"] {
        let p = dir.join(f);
        if p.exists() {
            let _ = std::fs::remove_file(&p);
            info!("Removed {}", f);
        }
    }
    ok!("Reset complete.");
}

async fn step_docker(real_user: &str) -> Result<()> {
    if !command_exists("docker") {
        info!("Docker not found. Installing Docker packages with apk…");
        // docker-cli-compose may be unavailable in some mirrors, fallback to core package set.
        if ensure_apk_packages(&["docker", "docker-cli-compose"]).is_err() {
            warn!("Failed to install docker-cli-compose; retrying with core Docker package only.");
            ensure_apk_packages(&["docker"])?;
        }
        ok!("Docker packages installed.");
    } else {
        info!("Docker CLI detected.");
    }

    if real_user != "root" {
        let user_group_add = std::process::Command::new("addgroup")
            .args([real_user, "docker"])
            .status();
        if !user_group_add.map(|s| s.success()).unwrap_or(false) {
            warn!("Could not add '{}' to docker group automatically (continue manually if needed).", real_user);
        } else {
            ok!("User '{}' is in docker group.", real_user);
        }
    }

    let _ = std::process::Command::new("rc-update")
        .args(["add", "docker", "default"])
        .status();

    let running = std::process::Command::new("rc-service")
        .args(["docker", "status"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !running {
        info!("Starting Docker daemon with OpenRC…");
        let _ = std::process::Command::new("rc-service")
            .args(["docker", "start"])
            .status();
    }

    let docker_version = std::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if let Some(ref ver) = docker_version {
        ok!("Docker detected: v{}", ver);
    } else {
        warn!("Docker server version could not be detected yet.");
    }

    // Quick reachability test
    let reachable = std::process::Command::new("docker")
        .args(["pull", "alpine:latest", "-q"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if reachable {
        ok!("Docker daemon reachable.");
    } else {
        warn!("Docker pull test failed — verify Docker is working before continuing.");
    }

    Ok(())
}

fn step_env(dir: &Path, real_user: &str) -> Result<()> {
    let env_path = dir.join(".env");

    let write_env = |port: &str| -> Result<()> {
        let secret = gen_secret()?;
        let now = {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            // Simple ISO-8601 from epoch (UTC, no leap-second correction)
            let s = secs % 60;
            let m = (secs / 60) % 60;
            let h = (secs / 3600) % 24;
            let days = secs / 86400;
            fn civil(d: u64) -> (u64, u64, u64) {
                let z = d + 719468;
                let era = z / 146097;
                let doe = z - era * 146097;
                let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                let y = yoe + era * 400;
                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                let mp = (5 * doy + 2) / 153;
                let dd = doy - (153 * mp + 2) / 5 + 1;
                let mo = if mp < 10 { mp + 3 } else { mp - 9 };
                let y = if mo <= 2 { y + 1 } else { y };
                (y, mo, dd)
            }
            let (y, mo, dd) = civil(days);
            format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, dd, h, m, s)
        };

        let content = format!(
            "# Yunexal Panel — auto-generated by yunexal-setup on {now}\n\
             PANEL_PORT={port}\n\
             COOKIE_SECRET={secret}\n\
             DATABASE_URL=sqlite:yunexal.db\n"
        );
        std::fs::write(&env_path, content).context("Failed to write .env")?;

        // Set ownership and permissions
        let _ = std::process::Command::new("chown")
            .args([&format!("{}:{}", real_user, real_user), env_path.to_str().unwrap()])
            .status();
        let _ = std::process::Command::new("chmod").args(["600", env_path.to_str().unwrap()]).status();

        ok!(".env written (port {}, fresh COOKIE_SECRET).", port);
        Ok(())
    };

    if env_path.exists() {
        warn!(".env already exists.");
        if prompt_yn("Overwrite .env with a new secret?", false)? {
            let port = prompt("Panel port", Some("3000"))?;
            write_env(&port)?;
        } else {
            info!("Keeping existing .env.");
        }
    } else {
        let port = prompt("Panel port", Some("3000"))?;
        write_env(&port)?;
    }

    Ok(())
}

async fn step_admin_user(non_interactive: bool, dir: &Path, real_user: &str) -> Result<()> {
    let (username, pass) = if non_interactive {
        let u = std::env::var("PANEL_USERNAME")
            .context("PANEL_USERNAME env var required with --non-interactive")?;
        let p = std::env::var("PANEL_PASSWORD")
            .context("PANEL_PASSWORD env var required with --non-interactive")?;
        (u, p)
    } else {
        let username = loop {
            let u = prompt("Admin username", None)?;
            if !u.is_empty() { break u; }
            eprintln!("\x1b[31m[ERROR]\x1b[0m Username cannot be empty.");
        };

        let pass = loop {
            let p = prompt_password("Admin password (min 8 chars)")?;
            if p.len() < 8 {
                eprintln!("\x1b[31m[ERROR]\x1b[0m Password too short (minimum 8 characters).");
                continue;
            }
            let p2 = prompt_password("Confirm password")?;
            if p != p2 {
                eprintln!("\x1b[31m[ERROR]\x1b[0m Passwords do not match.");
                continue;
            }
            break p;
        };

        (username, pass)
    };

    let pool = db::init_db().await.context("Database initialization failed")?;
    let hash = password::hash(&pass).context("Failed to hash password")?;
    db::seed_root_user(&pool, &username, &hash, "root").await?;
    ok!("Root user '{}' created/updated.", username);

    // Fix ownership: DB files were created by root, but the service runs as real_user.
    let owner_arg = format!("{}:{}", real_user, real_user);
    for f in &["yunexal.db", "yunexal.db-shm", "yunexal.db-wal"] {
        let p = dir.join(f);
        if p.exists() {
            let _ = std::process::Command::new("chown")
                .args([&owner_arg, p.to_str().unwrap_or(f)])
                .status();
            info!("chown {} → {}", f, real_user);
        }
    }

    Ok(())
}

async fn step_import_containers(dir: &Path) {
    if !prompt_yn("Import existing Docker containers into the panel?", false).unwrap_or(false) {
        info!("Skipping container import.");
        return;
    }

    // Connect to Docker
    let docker = match yunexal_panel::docker::get_docker_client().await {
        Ok(d) => d,
        Err(e) => { warn!("Cannot connect to Docker daemon: {}", e); return; }
    };

    // List all containers (not just managed ones — for import we want all)
    let containers = match list_all_containers(&docker).await {
        Ok(c) if !c.is_empty() => c,
        Ok(_) => { info!("No Docker containers found."); return; }
        Err(e) => { warn!("Failed to list containers: {}", e); return; }
    };

    println!();
    println!("\x1b[1mDocker containers:\x1b[0m");
    println!("  {:<4} {:<14} {:<28} {:<24} {}", "#", "ID", "Name", "Image", "Status");
    println!("  {}", "─".repeat(78));
    for (i, c) in containers.iter().enumerate() {
        println!("  {:<4} {:<14} {:<28} {:<24} {}", i + 1, &c.0[..12.min(c.0.len())], c.1, &c.2[..24.min(c.2.len())], c.3);
    }
    println!();

    let selection = match prompt("Enter numbers to import (e.g. 1 3 4) or 'all'", None) {
        Ok(s) => s,
        Err(_) => return,
    };

    let panel_pool = match db::init_db().await {
        Ok(p) => p,
        Err(e) => { warn!("DB init failed: {}", e); return; }
    };

    let selected_indices: Vec<usize> = if selection.trim().eq_ignore_ascii_case("all") {
        (0..containers.len()).collect()
    } else {
        selection.split_whitespace()
            .filter_map(|s| s.parse::<usize>().ok())
            .filter(|&n| n >= 1 && n <= containers.len())
            .map(|n| n - 1)
            .collect()
    };

    let db_path = dir.join("yunexal.db");
    if !db_path.exists() {
        warn!("Database not found at {:?} — run setup again after first start, or the DB was just created above.", db_path);
    }

    for idx in selected_indices {
        let (cid, cname, _cimage, _) = &containers[idx];
        match db::register_server(&panel_pool, cid, cname, 0).await {
            Ok(_) => {
                match yunexal_panel::docker::ensure_management_labels(&docker, cid).await {
                    Ok(true) => ok!("Imported: {} ({}) [labels normalized]", cname, &cid[..12.min(cid.len())]),
                    Ok(false) => ok!("Imported: {} ({})", cname, &cid[..12.min(cid.len())]),
                    Err(e) => {
                        warn!("Imported: {} ({}), but label migration failed: {}", cname, &cid[..12.min(cid.len())], e);
                    }
                }
            }
            Err(e) => warn!("Failed to import {}: {}", cname, e),
        }
    }
}

/// Lists ALL Docker containers (not just yunexal-managed), returns (id, name, image, status).
async fn list_all_containers(docker: &bollard::Docker) -> Result<Vec<(String, String, String, String)>> {
    use bollard::query_parameters::ListContainersOptions;

    let containers = docker
        .list_containers(Some(ListContainersOptions { all: true, ..Default::default() }))
        .await
        .context("Failed to list containers")?;

    let result = containers.into_iter().map(|c| {
        let id = c.id.unwrap_or_default();
        let name = c.names.as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let image = c.image.unwrap_or_default();
        let status = c.status.unwrap_or_default();
        (id, name, image, status)
    }).collect();

    Ok(result)
}

fn sh_esc_single(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "'\\''")
}

fn step_openrc_service(dir: &Path, real_user: &str) -> Result<()> {
    let service_path = PathBuf::from("/etc/init.d/yunexal-panel");

    // Find binary
    let svc_bin = ["yunexal-panel", "target/release/yunexal-panel", "target/debug/yunexal-panel"]
        .iter()
        .map(|p| dir.join(p))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.join("target/release/yunexal-panel").to_string_lossy().to_string());

    let launcher_path = dir.join("yunexal-panel-launcher.sh");
    let launcher_content = format!(
        "#!/bin/sh\n\
         set -a\n\
         [ -f '{workdir}/.env' ] && . '{workdir}/.env'\n\
         set +a\n\
         exec '{bin}' \"$@\"\n",
        workdir = sh_esc_single(&dir.to_string_lossy()),
        bin = sh_esc_single(&svc_bin),
    );
    std::fs::write(&launcher_path, launcher_content)
        .context("Failed to write launcher script")?;
    let _ = std::process::Command::new("chmod")
        .args(["755", launcher_path.to_str().unwrap_or_default()])
        .status();

    let service_content = format!(
        "#!/sbin/openrc-run\n\
         name=\"yunexal-panel\"\n\
         description=\"Yunexal Panel service\"\n\
         command=\"{launcher}\"\n\
         command_user=\"{real_user}:{real_user}\"\n\
         directory=\"{workdir}\"\n\
         pidfile=\"/run/yunexal-panel.pid\"\n\
         command_background=\"yes\"\n\
         start_stop_daemon_args=\"--make-pidfile --pidfile ${{pidfile}} --stdout /var/log/yunexal-panel.log --stderr /var/log/yunexal-panel.log\"\n\
         \n\
         depend() {{\n\
             need net docker\n\
             after firewall\n\
         }}\n\
         \n\
         start_pre() {{\n\
             checkpath --file --mode 0644 --owner {real_user}:{real_user} /var/log/yunexal-panel.log\n\
             checkpath --directory --mode 0755 /run\n\
         }}\n",
        real_user = real_user,
        workdir = dir.display(),
        launcher = launcher_path.display(),
    );

    std::fs::write(&service_path, service_content)
        .context("Failed to write OpenRC service file")?;
    let _ = std::process::Command::new("chmod")
        .args(["755", service_path.to_str().unwrap_or_default()])
        .status();

    let _ = std::process::Command::new("rc-update")
        .args(["add", "yunexal-panel", "default"])
        .status();
    ok!("OpenRC service installed and enabled: {}", service_path.display());

    if prompt_yn("Start yunexal-panel now?", true)? {
        if Path::new(&svc_bin).exists() {
            let _ = std::process::Command::new("rc-service")
                .args(["yunexal-panel", "start"])
                .status();
            std::thread::sleep(std::time::Duration::from_secs(1));
            let active = std::process::Command::new("rc-service")
                .args(["yunexal-panel", "status"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if active {
                ok!("yunexal-panel is running.");
            } else {
                warn!("Service did not start cleanly — check: rc-service yunexal-panel status");
            }
        } else {
            warn!("Binary not found at {} — build the project first.", svc_bin);
        }
    } else {
        info!("Service not started. Run: rc-service yunexal-panel start");
    }

    Ok(())
}

// ── nginx WebSocket proxy ─────────────────────────────────────────────────────

/// Builds an nginx virtual-host config with WebSocket proxy headers.
fn build_nginx_config(domain: &str, port: &str, ssl: Option<(&str, &str)>) -> String {
    // The location block — identical for HTTP and HTTPS servers.
    // proxy_http_version 1.1 + Upgrade/Connection headers are required for
    // wss:// (WebSocket over TLS) to work through nginx.
    let location = format!(
        "    server_tokens off;\n    proxy_hide_header X-Powered-By;\n\n    # WebSocket + HTTP reverse proxy (required for console)\n    location / {{\n        proxy_pass http://127.0.0.1:{port};\n        proxy_http_version 1.1;\n        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection \"upgrade\";\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n        proxy_read_timeout 3600s;\n        proxy_send_timeout 3600s;\n    }}\n",
        port = port,
    );

    if let Some((cert, key)) = ssl {
        format!(
            "server {{\n    listen 80;\n    server_name {d};\n    return 301 https://$host$request_uri;\n}}\n\nserver {{\n    listen 443 ssl;\n    server_name {d};\n\n    ssl_certificate {cert};\n    ssl_certificate_key {key};\n\n{location}}}\n",
            d = domain, cert = cert, key = key, location = location,
        )
    } else {
        format!(
            "server {{\n    listen 80;\n    server_name {d};\n\n{location}}}\n",
            d = domain, location = location,
        )
    }
}

fn step_nginx(dir: &Path) -> Result<()> {
    let mut nginx_installed = command_exists("nginx");

    if !nginx_installed {
        warn!("nginx not found.");
        if prompt_yn("Install nginx now via apk?", true)? {
            ensure_apk_packages(&["nginx"])?;
            nginx_installed = true;
        }
    }

    if !nginx_installed {
        info!("Skipping nginx configuration.");
        return Ok(());
    }

    if !prompt_yn("Configure nginx reverse proxy? (required for wss:// console WebSocket)", true)? {
        info!("Skipping nginx configuration.");
        warn!("WebSocket consoles will NOT work if nginx is not configured with WebSocket headers.");
        return Ok(());
    }

    let panel_port = read_env_port(dir).unwrap_or_else(|| "3000".to_string());

    let domain = prompt("Domain name (e.g. panel.example.com)", None)?;
    if domain.is_empty() {
        warn!("No domain entered — skipping nginx configuration.");
        return Ok(());
    }

    let cert_path = format!("/etc/letsencrypt/live/{}/fullchain.pem", domain);
    let key_path  = format!("/etc/letsencrypt/live/{}/privkey.pem",   domain);
    let has_ssl   = Path::new(&cert_path).exists();

    let config = build_nginx_config(
        &domain,
        &panel_port,
        if has_ssl { Some((cert_path.as_str(), key_path.as_str())) } else { None },
    );

    let config_path  = PathBuf::from("/etc/nginx/http.d/yunexal-panel.conf");

    std::fs::write(&config_path, &config).context("Failed to write nginx configuration")?;

    let _ = std::process::Command::new("rc-update")
        .args(["add", "nginx", "default"])
        .status();

    // Test and reload
    let test = std::process::Command::new("nginx").arg("-t").output();
    match test {
        Ok(o) if o.status.success() => {
            let is_running = std::process::Command::new("rc-service")
                .args(["nginx", "status"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if is_running {
                let _ = std::process::Command::new("rc-service").args(["nginx", "reload"]).status();
            } else {
                let _ = std::process::Command::new("rc-service").args(["nginx", "start"]).status();
            }

            ok!("nginx configured and reloaded: {}", config_path.display());
            if !has_ssl {
                info!("To enable HTTPS with Let's Encrypt:");
                info!("  apk add --no-cache certbot certbot-nginx");
                info!("  sudo certbot --nginx -d {}", domain);
            }
        }
        Ok(o) => {
            warn!("nginx config test failed — please fix {}:", config_path.display());
            warn!("{}", String::from_utf8_lossy(&o.stderr).trim());
        }
        Err(e) => {
            warn!("Could not verify nginx config: {}", e);
            ok!("Config written to {} — reload nginx manually with OpenRC.", config_path.display());
        }
    }

    Ok(())
}

fn read_env_port(dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join(".env")).ok()?;
    for line in content.lines() {
        if line.starts_with("PANEL_PORT=") {
            return Some(line["PANEL_PORT=".len()..].to_string());
        }
    }
    None
}
