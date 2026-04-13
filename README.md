# Yunexal Panel

> **v0.4.1** — Self-hosted server management platform built on Docker.

Built with **Rust + Axum**, **SQLite**, and **Bollard** (Docker SDK).  
Templates and static assets are compiled into a single binary — no external runtime files needed.

---

## Table of Contents

- [Roadmap](#roadmap)
- [Features](#features)
- [Installation](#installation)
- [Alpine Installer ISO](#alpine-installer-iso)
- [Reverse Proxy (nginx)](#reverse-proxy-nginx)
- [Requirements](#requirements)
- [Configuration](#configuration)
- [Building from Source](#building-from-source)
- [Project Structure](#project-structure)
- [Tech Stack](#tech-stack)
- [License](#license)
- [Contributors](CONTRIBUTORS.md)

---

## Roadmap

> The panel is in active development with a focus on stability and core features first.
> The following features are planned for the next few releases.
> Spoiler: it's a dont completed roadmap, because we has a dynamic and crazy ideas that we want to implement, but we will try to stick to this roadmap as much as possible.
> If you have any suggestions or want to contribute, feel free to open an issue or a pull request!

### General
| Status | Feature |
|---|---|
| ✅ | Overview — system stats, ZRAM, panel updates |
| ✅ | All Containers — manage any container across all users |
| ✅ | Images — pull, delete, duplicate, ENV overrides |

### Management
| Status | Feature |
|---|---|
| ✅ | Users — create, delete, roles (`root` / `admin` / `user`) |
| ✅ | DNS — multi-provider (Cloudflare, GoDaddy, DuckDNS, Namecheap, Generic), DDNS, SRV |
| 🔜 | Agents — automated task runners attached to containers |
| 🔜 | Firewall — global IP allow/block rules beyond per-port UFW |
| 🔜 | Backups — scheduled volume snapshots with retention policies |
| 🔜 | Tickets — built-in support ticket system for end users |

### Analytics
| Status | Feature |
|---|---|
| ✅ | Audit Log — immutable, 200 records, multi-select filter, Device column |
| 🔜 | Insights — historical resource usage charts and trend analysis |

### Configuration
| Status | Feature |
|---|---|
| ✅ | Panel Settings — UFW, bandwidth, Cloudflare UAM/L7, sidebar visibility, panel updates |
| 🔜 | Notifications — email / webhook alerts for events (container down, login, etc.) |
| 🔜 | Themes — custom colour schemes and branding per installation |
| 🔜 | API Keys — REST API access tokens for external integrations |

### Other
| Status | Feature |
|---|---|
| 🔜 | Support Windows as a host level (Yes, it's possible I think) |
| 🔜 | Mobile app (Flutter or React Native) |
| 🔜 | Support ARM servers |
| 🔜 | Marketplace — pre-configured server templates for popular games and applications |
| 🔜 | Community plugins — allow third-party extensions for additional features and integrations |
| 🔜 | Localization — multi-language support with user-selectable languages |
| 🔜 | Accessibility — ensure the panel is usable with screen readers and keyboard navigation |

And much more! The roadmap is flexible and will evolve based on user feedback and new ideas.
You can make a pull request to add your own features or upvote existing ones in the [Issues](https://github.com/nestorchurin/yunexal-panel/issues)
Or help to implement features by joining the development on the [Discussions](https://github.com/nestorchurin/yunexal-panel/discussions) page.

---

## Features

### Dashboard
- Live grid of all managed containers with CPU %, RAM, Network I/O, Disk I/O and uptime
- **In-place updates** — cards refresh state without DOM re-creation (no animation flicker)
- **"My only" toggle** — admins can filter to show only their own servers; placed in the topbar next to "New Server"
- Auto-polling every 5 s; status badges: Running / Stopped / Error
- Change own password directly from the dashboard

### Server Console
- WebSocket terminal attached to Docker container TTY via **xterm.js**
- Full ANSI colour support + HTML-tag converter for servers that emit `<b>`, `<span style="color:">`, etc.
- Dedicated command input field below the terminal (Enter sends to stdin)
- Live metric charts (1 s polling, 200-point history):
  - CPU % · RAM % (used / limit) · Network KB/s · Disk I/O KB/s
- **Storage card** — volume size (MB) fetched once on open
- Per-server DNS panel — view records linked to this server

### File Manager
- Folder/file browsing with breadcrumb navigation
- **150+ format icons** across 14 colour-coded categories — code, config, archive, image, audio, video,
  binary, lock, shell/scripts, Python, Java/JVM, HTML templates, CSS, data/CSV and more;
  special exact-name detection for `Makefile`, `Dockerfile`, `README`, `LICENSE`, etc.
- **Edit** text/config files in a full-screen Ace code editor
- **Create** new files and directories
- **Rename**, **Copy/Paste**, **Delete** (right-click context menu)
- **Drag-and-drop upload** with per-file progress (streamed to disk, root-permission safe via Alpine helper)
- **Archive & Extract** — create `.tar.gz` archives; extract `.tar.gz`, `.tar.bz2`, `.tar.xz`, `.zip`, `.jar`, `.rar`, `.7z`, `.gz`, `.bz2`, `.xz`
- Path traversal protection enforced on all backend endpoints

### Server Settings
- **Environment Variables** — row-based editor: each `KEY=VALUE` rendered as its own row
  - Regular users can edit values; only admins can add, delete or rename keys
  - "Save ENV" recreates the container with the new environment
- **Factory Reset** — wipes the volume and restarts the container; requires password confirmation
  - Redesigned modal: danger-styled border, eye-toggle on the password field
- **Danger Zone** — Delete Server (admin only)

### Networking
- View all port bindings (host ↔ container) with protocol (TCP / UDP / TCP+UDP)
- **Add / Remove** port mappings (admin only) with port conflict pre-check
- **Tag** ports with a friendly label (e.g. `Game`, `RCON`)
- **Enable / Disable** individual port mappings
- **UFW block** — per-port shield button blocks/unblocks traffic at OS level via `sudo -n ufw`
  - Visible only when UFW is enabled in Panel Settings
  - Permission-aware: shows a sudoers fix command if `sudo -n` is denied
- **Bandwidth limiting** via Linux `tc` TBF qdisc (Mbit/s) — persisted and reapplied on restart

### Container Creation (admin only)
- Create containers from any Docker Hub or local image
- Full **Docker Compose-style YAML** config via Monaco editor (live GUI ↔ YAML sync)
- Dynamic port-binding rows with host/container fields and protocol selector
- **"Fetch ENV"** — auto-detects environment variables from Docker image metadata
- **Image ENV overrides** — admin-configured DB defaults applied on top of image defaults
- Port conflict detection and duplicate name check before creation
- Owner assignment — assign any container to any user
- **DNS/SRV auto-record** — optionally create an SRV record on creation and delete it on removal

### DNS Management (admin only)
Full multi-provider DNS management:

| Provider | Zones | Record CRUD | DDNS | Proxy |
|---|---|---|---|---|
| **Cloudflare** | Full API zone list | All types | ✓ | ✓ |
| **GoDaddy** | Active domains | Full | ✓ | — |
| **DuckDNS** | Single domain | — | ✓ | — |
| **Namecheap** | Single domain | — | ✓ | — |
| **Generic** | Single domain | — | ✓ (templated URL) | — |

- Record types: A, AAAA, CNAME, MX, TXT, SRV, NS, CAA and more
- **DDNS** — per-record toggle with configurable interval; auto-updates A records with the server's public IP
- Container-linked records, TTL presets, type-coloured badges, search and filter chips

### Admin Panel
**Tabs:** Overview · Containers · Images · Users · DNS · Audit Log · Panel Settings

- **Users** — create, delete and set passwords; role-based access (`root` / `admin` / `user`);
  admins cannot delete other admins — only `root` can
- **Images** — pull, delete, duplicate, ENV override editor
- **Containers** — edit any container; stop all at once; per-row state updates without animation flicker
- **Audit Log** — immutable; 200 records per page; multi-select action filter; Device column (parsed User-Agent); full UA in tooltip
- **Panel Updates** — check for new releases (stable/unstable), one-click download & install with auto-restart

### Panel Settings (root only)
- **UFW toggle** — enable/disable UFW port-blocking globally
- **Bandwidth toggle** — show/hide the Bandwidth section on Networking pages
- **Cloudflare Under Attack Mode (auto)**
  - Brute-force trigger: auto-enables UAM when distinct failing IPs ≥ threshold
  - L7 HTTP-flood trigger: auto-enables UAM when ≥ N IPs exceed req/min threshold (60 s window)
  - Auto-disables after cooldown when no active attacks detected
  - Manual override button
- **Sidebar Visibility** — toggle SOON (upcoming feature) badges in the admin sidebar
- **ZRAM hint** — collapsible "How to enable ZRAM" block when ZRAM is inactive

### Authentication & Security
- Session-based login with **encrypted private cookies** (AES-GCM via axum-extra)
- **Argon2id** password hashing (random salt)
- Route-level middleware: unauthenticated → `/login`; non-admin on admin routes → 403
- **Rate limiting** — 5 failed logins per IP → 60 s lockout
- **Security headers** — CSP, X-Frame-Options, HSTS, Referrer-Policy, Permissions-Policy
- **SameSite=Strict** session cookies prevent CSRF
- XSS protection: Askama auto-escaping + `escHtml()` / `escAttr()` in JavaScript
- Path traversal protection on all file endpoints

### UI / UX
- Responsive **Bootstrap 5** dark-mode layout
- **AMOLED mode** — pure-black theme for mobile OLED screens with auto-fullscreen
- **PWA** — `manifest.json` + service worker for installable web app
- **HTMX** for partial page updates
- Load-time footer badge (seconds) on every page

---

## Installation

Download the latest binaries from the [Releases](https://github.com/nestorchurin/yunexal-panel/releases) page.

```bash
# 1. Download and extract
wget https://github.com/nestorchurin/yunexal-panel/releases/latest/download/yunexal-panel-linux-x86_64.tar.gz
tar -xzf yunexal-panel-linux-x86_64.tar.gz
cd yunex-release

# 2. Run the setup wizard
#    Auto-detects host flow:
#    - Alpine/OpenRC (apk + rc-service)
#    - Debian/Ubuntu/systemd (apt-get + systemctl)
sudo ./yunexal-setup

# 3. Run
./yunexal-panel
```

The SQLite database (`panel.db`) and `volumes/` directory are created automatically on first run.

---

## Alpine Installer ISO

Build a custom Alpine installer image with Yunexal integrated:

```bash
./scripts/iso/build-installer.sh
```

Outputs:

- `x86_64` installer ISO with Yunexal profile (`scripts/iso/mkimg.yunexal.sh`)
- Experimental ARM images (`aarch64`, `armv7`) via upstream profile

Inside the live installer shell, use safe partition workflow:

```bash
yunexal-install prepare --disk /dev/sdX --mode safe --root-size-gib 40
# then run setup-alpine targeting partition 2
yunexal-install finalize --disk /dev/sdX --target-root /mnt
```

This enforces a separate persistent data partition (`/var/lib/yunexal/volumes`) with `prjquota` mount options in installed `fstab`.

Detailed knobs and environment variables are in [scripts/iso/README.md](scripts/iso/README.md).

---

## Reverse Proxy (nginx)

The panel itself speaks plain HTTP. For production use with a domain and HTTPS (e.g. `https://panel.yunexal.com`), you need a reverse proxy in front of it.

> **Important:** The WebSocket console (`wss://`) will **not work** unless the reverse proxy is configured to forward WebSocket upgrade headers. Without this, Firefox shows *"can't establish a connection to the server at wss://…"*.

> **Cloudflare Proxy users:** Disable **Rocket Loader** for the panel domain.
> Rocket Loader rewrites `<script>` tags and injects its own scripts without a CSP nonce, which conflicts with the panel's Content Security Policy and breaks Monaco Editor, Chart.js and WebSocket initialization.
> **Cloudflare Dashboard → Your Domain → Speed → Settings → Rocket Loader → OFF**

The setup wizard (`yunexal-setup`) detects nginx and can generate this config automatically (Step 7). If you prefer to configure it manually:

**Config file path**

- Alpine/OpenRC: `/etc/nginx/http.d/yunexal-panel.conf`
- Debian/Ubuntu/systemd: `/etc/nginx/sites-available/yunexal-panel.conf` (enable via symlink in `/etc/nginx/sites-enabled/`)

```nginx
# HTTP → HTTPS redirect
server {
    listen 80;
    server_name panel.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    server_name panel.example.com;

    ssl_certificate     /etc/letsencrypt/live/panel.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/panel.example.com/privkey.pem;

    # WebSocket + HTTP reverse proxy (required for console)
    location / {
        proxy_pass http://127.0.0.1:3000;

        # These three lines are mandatory for WebSocket (wss://) to work:
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
```

Enable the site and reload nginx:

```bash
sudo nginx -t

# systemd hosts
sudo systemctl reload nginx

# OpenRC hosts
sudo rc-service nginx reload
```

To add SSL via Let's Encrypt:

```bash
# Alpine/OpenRC
sudo apk add --no-cache certbot certbot-nginx

# Debian/Ubuntu/systemd
sudo apt-get install -y certbot python3-certbot-nginx

sudo certbot --nginx -d panel.example.com
```

---

## Requirements

| Requirement | Notes | Minimum | Recommended |
|---|---|---|---|
| **OS** | Distribution for the panel | Alpine 3.20+ or Debian/Ubuntu with systemd | Alpine latest stable or Ubuntu LTS |
| **Docker Engine** | Must be running; socket at `/var/run/docker.sock` | 24.0 | 29.0 + |
| **Docker image `alpine`** | Pulled automatically by `yunexal-setup` | latest | latest |
| **RAM** | For the panel process | 64 MB if using minimal features with containers | 2 GB if using full features with containers |
| **CPU** | For the panel process | 1 vCPU | 2 vCPU |
| **GPU** | For hardware acceleration (optional) | None | Recommended if using GPU-intensive features |
| **Disk space** | For the panel binary, database, and volumes | 100 MB | 500 MB |
| **Filesystem** | For volume management and quotas | Any without disk quotas | XFS or ZFS with pquota/prjquota/refquota for disk quotas |
| **Ports** | Panel port (default: 3000) + container ports | 1 free port for the panel + container ports | Multiple free ports for the panel and containers |
| **Reverse proxy** | For production use with a domain and HTTPS | Optional (HTTP-only access) | Recommended (HTTPS + SSL with WebSocket support) |
| **Ethernet** | For network connectivity | 100 Mbps | 1 Gbps or higher |


> **Docker socket access** — add your user to the `docker` group:
> ```bash
> # Debian/Ubuntu/systemd
> sudo usermod -aG docker $USER && newgrp docker
>
> # Alpine/OpenRC
> sudo addgroup $USER docker
> ```

> **UFW sudo access** — to use per-port UFW blocking without a password prompt, add a sudoers rule
> (shown by the panel automatically if access is denied):
> ```bash
> echo "www-data ALL=(ALL) NOPASSWD: /usr/sbin/ufw" | sudo tee /etc/sudoers.d/yunexal-ufw
> ```

---

## Configuration

All configuration is read from a `.env` file in the **same directory as the binary**, or from environment variables directly.
`yunexal-setup` generates this file interactively.

```dotenv
# Panel port (default: 3000)
PANEL_PORT=3000

# 128-character hex string (64 random bytes) — signs and encrypts session cookies.
# Changing this value invalidates all active sessions.
# Generate with:  openssl rand -hex 64
COOKIE_SECRET=<128 hex chars>
```

Initial credentials are set by `yunexal-setup`.
Additional users and all panel settings are managed from the Admin Panel at `/admin`.

---

## Building from Source

Requires **Rust 1.78+** — install via [rustup.rs](https://rustup.rs).

```bash
git clone https://github.com/nestorchurin/yunexal-panel.git
cd yunexal-panel
cargo build --release

# Musl-only release artifacts (no glibc loader)
./scripts/release/build-musl.sh

# Interactive setup (Docker, .env, root user, optional OpenRC/systemd service)
sudo ./target/release/yunexal-setup

./target/release/yunexal-panel
```

---

## Project Structure

```
src/
├── main.rs               # Entry point, router, middleware
├── lib.rs                # Library crate (shared between binaries)
├── state.rs              # AppState — DB pool, Docker client, CF UAM state, L7 counters
├── auth.rs               # Session helpers, admin guard, rate limiter, CF UAM/L7 triggers
├── cloudflare.rs         # Cloudflare API wrapper (security level, UAM enable/disable)
├── compose.rs            # Docker Compose YAML parser
├── password.rs           # Argon2id hash / verify
├── dns.rs                # DNS provider API clients
├── db/
│   ├── mod.rs            # Schema init, migrations, seed defaults
│   ├── users.rs          # User CRUD
│   ├── servers.rs        # Server CRUD
│   ├── ports.rs          # Port mappings + UFW state
│   ├── dns.rs            # DNS records & providers
│   ├── images.rs         # Image ENV overrides
│   ├── audit.rs          # Audit log (immutable, user-agent)
│   └── settings.rs       # panel_settings key/value store
├── docker/
│   ├── mod.rs            # Docker client, ContainerInfo
│   ├── containers.rs     # Lifecycle, attach, list
│   ├── stats.rs          # CPU/RAM/network/disk I/O stats
│   ├── images.rs         # Pull, delete, duplicate, ENV fetch
│   ├── files.rs          # Volume file operations
│   ├── network.rs        # Bandwidth limiting (tc TBF), isolated networks
│   └── edit.rs           # Inspect & recreate containers
├── bin/
│   └── setup.rs          # yunexal-setup: interactive wizard
└── handlers/
    ├── mod.rs            # Router, embedded assets, track_requests middleware
    ├── auth.rs           # Login / logout
    ├── dashboard.rs      # Dashboard + server list fragment
    ├── servers.rs        # Console, Settings, Stats, lifecycle, ENV update, Factory Reset
    ├── files.rs          # File manager API
    ├── network.rs        # Networking + port / bandwidth / UFW API
    ├── create.rs         # Container creation
    ├── admin.rs          # Admin panel — users, images, containers, panel settings
    ├── dns.rs            # DNS management API
    ├── ws.rs             # WebSocket console
    └── templates.rs      # Askama template structs

templates/                # Askama HTML templates — compiled into binary
static/                   # CSS, JS, icons — compiled into binary via rust-embed
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| Web framework | [Axum](https://github.com/tokio-rs/axum) 0.8 |
| Async runtime | [Tokio](https://tokio.rs) |
| Docker SDK | [Bollard](https://github.com/fussybeaver/bollard) 0.20 |
| Database | SQLite via [SQLx](https://github.com/launchbadge/sqlx) 0.8 (WAL mode) |
| HTTP client | [reqwest](https://github.com/seanmonstar/reqwest) |
| Templates | [Askama](https://github.com/djc/askama) 0.15 — compiled into binary |
| Static assets | [rust-embed](https://github.com/pyros2097/rust-embed) — compiled into binary |
| Password hashing | [Argon2](https://github.com/RustCrypto/password-hashes) (Argon2id) |
| Session cookies | [axum-extra](https://docs.rs/axum-extra) private cookies (AES-GCM) |
| Concurrent maps | [DashMap](https://github.com/xacrimon/dashmap) — L7 per-IP counters |
| Frontend | Bootstrap 5 · [HTMX](https://htmx.org) · vanilla JS |
| Terminal | [xterm.js](https://xtermjs.org) with FitAddon |
| Charts | [Chart.js](https://www.chartjs.org) |
| Code editors | [Ace](https://ace.c9.io) (file editor) · [Monaco](https://microsoft.github.io/monaco-editor/) (YAML / compose) |

---

## License

[MIT](LICENSE)
