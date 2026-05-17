# Changelog

## [0.5.0] — 2026-05-17

> This release consolidates all development from v0.4.1 through the v0.5.0 unstable branch.
> It covers the period from April 5, 2026 to May 17, 2026 across seven snapshot builds.

### Highlights

- **RBAC** — full role-based access control with tri-state permissions and Role Studio UI
- **Container-scoped user access** — add users to specific containers without granting global admin
- **Session management** — device tracking, per-device logout, password-change session invalidation
- **Server Audit Log** — per-server immutable event log with filters and download
- **SPA navigation** — server sidebar tabs load without full page reloads
- **Alpine Linux / OpenRC** — setup wizard now supports Alpine alongside Debian/Ubuntu
- **Service API key** — third-party integrations can authenticate with an API key
- **Storage simplified** — ext4 + prjquota is the sole quota-capable backend; cross-disk migration via Admin Settings
- **New Server UI** — local-first image resolution, in-input image picker, build from Dockerfile

---

### RBAC — Roles & Permissions

- New `roles` + `role_permissions` schema with tri-state policy (`read / none / write`) per capability
- **Role Studio UI** in Admin Panel: create roles, edit permission matrix, assign colour, directory + focused editor layout
- `root` role is immutable — policy changes are blocked at the backend
- Role colour picker for all roles including `root` (colour persists across restarts)
- Topbar badge shows the current user's role name and colour from DB
- `require_admin` middleware converted to permission-aware access control with route → permission mapping
- `/admin` and sub-routes now respect granular role permissions; users are redirected to their first permitted tab

### Users — UID + Nickname Identity

- Users schema extended with `uid` (9–16 chars, unique) and `nickname` (≤24 chars)
- DB migration backfills `uid`/`nickname` for legacy installations; validation triggers enforce constraints
- Login accepts `username` or `uid`
- Create-user form generates UID automatically; Admin Users table displays `UID` and `Nickname` columns
- `update_user_role(...)` helper added for in-table role assignment across all users

### Container-Scoped User Access (Server Members)

- New `server_user_permissions` table: per-server per-user capability policy (`none / read / write`)
- Members added by UID only; copyable UID badge in the member list
- Capability categories: `console · files · networking · settings · audit · power`
- Server sidebar **Users tab** — visible only when the container has members
- `can_access_server_permission(...)` used for capability checks across all server handlers
- Dashboard and stats show containers where a user is a member, not only owned containers
- Member cleanup on user/server deletion

### Session Management & Device Tracking

- `user_sessions` table tracks active device sessions with `last_seen_at`
- Session cookie upgraded to `v2` format: carries `session_id` + password-hash stamp
- Password change invalidates all sessions (all devices) — clients are redirected to login automatically
- Auth middleware validates active session record and updates last-seen timestamp
- **Devices modal** in Dashboard Settings: lists all active sessions; per-device logout; current session is protected
- Global fetch wrapper redirects to `/login` on expired or invalid session responses

### Server Audit Log

- Per-server audit log page at `/servers/{id}/audit`
- API `GET /api/servers/{id}/audit` with pagination, `action` / `search` / `actor` filters
- Export `GET /api/servers/{id}/audit/download` — full log as `.log` file with current filters
- Dedicated UI: table, search, multi-select action filter dropdown, paginated list, Device/UA column
- SPA-safe DOM binding; `server_id` resolved from URL to avoid stale IDs after tab switches

### SPA Navigation — Server Sidebar

- Server sidebar tabs navigate via `fetch + history.pushState` — only `.yu-main` is replaced
- Missing head assets and page-scripts are loaded on first visit to each tab
- `yu:page-shown` event allows page scripts to re-initialise polling and WebSocket on revisit
- Clicking the active tab re-triggers `yu:page-shown` refresh (analogous to browser refresh)
- Back/forward navigation works via `popstate`
- Lifecycle hooks added to Console (WS reconnect + terminal fit), Files (polling restart), Networking (re-init)

### Alpine Linux / OpenRC Support

- `yunexal-setup` auto-detects init system (`OpenRC` / `systemd`) and package manager (`apk` / `apt-get`)
- Generates OpenRC service at `/etc/init.d/yunexal-panel` or systemd unit at `/etc/systemd/system/yunexal-panel.service`
- nginx config written to `/etc/nginx/http.d/` (Alpine) or `/etc/nginx/sites-available/` + symlink (Debian/Ubuntu)
- Docker / UFW / storage privileged commands use host-aware tool paths via `src/host.rs`
- `host::privileged_command` detects root sessions and skips `sudo -n` when not needed

### Service API Key

- `service_api_key` stored in `panel_settings` (or `YUNEXAL_API_KEY` / `PANEL_API_KEY` env vars)
- `POST /api/auth/service-login` — validates API key and issues a standard session cookie
- Third-party services can call all panel API endpoints using that cookie without browser login

### Storage — ext4 + prjquota Only

- Quota backend simplified to **ext4 + prjquota** exclusively (XFS, ZFS, Btrfs quota handlers removed)
- `quota.rs` now exposes only ext4 project-quota flow: `ext4_pquota_mount`, `apply_ext4_quota`, `remove_ext4_quota`
- `remove_ext4_quota` has fallback cleanup across all ext4 mounts to prevent stale project quota on deletion
- Container creation hard-blocks if `disk_limit` is set and the storage path is not quota-capable
- `validate_storage_base_path(...)` blocks critical mount points, system root disk, and ext4 without `prjquota`
- **Unsafe override mode** in Admin Settings bypasses storage policy for advanced setups (explicit confirm required)
- Cross-disk/filesystem container migration via `POST /api/admin/storage/migrate`:
  - stop → prepare target → rsync data → recreate container → update DB → reapply bandwidth/quota → restart
  - preserves volume root owner/mode to prevent post-migration permission drift
- `/api/admin/storage/mounts` supports `include_all=1`: blocked mounts are visible with reason, not selectable
- Storage settings moved to categorical **Admin Settings** layout (Storage / Security / Operations / Interface)

### New Server UI

- **Local-first image resolution** — `docker::get_image_info` checked before pull
- **In-input image picker** — dropdown with local image list, live filter on keystroke, open/close on focus
- **Build from Dockerfile** — multipart upload (`POST /api/image/build-dockerfile`); built tag auto-fills the image field
- `valid_image_ref(...)` server-side image tag validation
- Quota preflight converted to hard-block: Create button disabled until quota check passes
- Volume Mounts section removed from the form

### Admin Edit — Parity with New Server

- `admin_edit` page redesigned to match `new_server` style (topbar, card sections, two-column layout, YAML preview)
- Monaco YAML preview synced with form fields; Ace file editor parity
- Port editor supports `tcp+udp`; startup `ports/env` data read from hidden `<textarea>` to avoid script breakage on special chars
- **Disk Limit** and **Bandwidth Limit** fields added; disk label updated via recreate on change
- Storage & Migration block with quota preflight, storage selector, and per-container migration trigger

### Security Hardening

- `client_ip()` trusts `X-Forwarded-For` / `X-Real-IP` only when the direct peer is a local/private proxy address
- `resolve_path()` in file handlers canonicalizes root and target to block symlink-escape outside the volume
- Self-update archive download: 128 MiB max size guard, empty-download rejection, symlink-skipping binary discovery
- CSP hardening: nonce + SRI on HTMX script include in Files page

### Dashboard Settings

- `Change Password` replaced with **Settings** dropdown entry
- Password change handles `force_logout` response and redirects to `/login`
- **Devices** entry in Settings modal: active sessions list with per-device logout

### Admin UI Polish

- Admin Settings redesigned with categorical navigation (Storage · Security · Operations · Interface)
- Category selection persists in `sessionStorage` across tab switches
- Storage selector dropdown repositions live on scroll/resize (including mobile `visualViewport`)
- `btn-yu-primary / ghost / danger / success / sm` alias classes added; `prefers-reduced-motion` fallback

### Bug Fixes

- Server tabs blank until manual reload — SPA lifecycle now keeps current `.yu-main` visible until target is ready
- New Server JavaScript crash (`_refreshSrvPortSelect is not defined`) — guarded as optional integration hook
- Files modal Bootstrap instance crash (`this._config is undefined`) — fallback-safe `fbShowModal/fbHideModal` wrappers
- Dashboard `My only` filter regression after nickname-first owner display — owner filter now applied only for admin toggle
- Hash navigation side effects removed; global route-hash auto-generation replaced with explicit `yuRouteUrl`/`yuGo` helpers
- Files hash parser accepts only valid path-hash (decoded string must start with `/`)
- Archive extract ownership: `chown` applied to extracted files to prevent root ownership after extraction
- Quota deletion on server removal now calls `remove_ext4_quota` before deleting volume data
- Legacy containers (v0.4.1) management labels normalised at startup via `auto_migrate_legacy_labels`

---

## [0.4.1] and earlier

See the [Releases](https://github.com/nestorchurin/yunexal-panel/releases) page for previous release notes.
