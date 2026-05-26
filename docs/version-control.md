# Container Version Control

This document describes the planned Git-backed Version Control tab for Yunexal Panel containers.

## Goals

- Add a per-container **Version Control** tab available to the server owner, admins, and users with the `version_control` permission.
- Connect a container volume to a GitHub repository and selected branch.
- Support manual `status`, `pull`, `push`, `sync`, `checkout`, and `commit all` actions.
- Support optional auto-sync from the configured branch.
- Add GitHub OAuth for connecting a GitHub account/token instead of pasting a token manually.

## Environment variables

GitHub OAuth requires these variables on the panel host:

```env
GITHUB_CLIENT_ID=...
GITHUB_CLIENT_SECRET=...
GITHUB_REDIRECT_URL=https://panel.example.com/auth/github/callback
```

The OAuth flow should use GitHub scopes `repo` and `read:user` when private repositories must be supported. Public-only installations can reduce this to `public_repo read:user`.

## Security notes

- Tokens must be stored server-side only and never rendered back into HTML or JSON responses.
- Git commands must run only inside the resolved container volume root.
- Branch names and repository URLs must be validated before use.
- SSH URLs should be disabled unless host SSH keys are explicitly configured.
- Auto-sync should be opt-in per container.

## Backend routes

Planned authenticated routes:

- `GET /servers/{id}/version-control`
- `GET /api/servers/{id}/git/status`
- `POST /api/servers/{id}/git/connect`
- `POST /api/servers/{id}/git/pull`
- `POST /api/servers/{id}/git/push`
- `POST /api/servers/{id}/git/sync`
- `POST /api/servers/{id}/git/checkout`
- `POST /api/servers/{id}/git/commit-all`
- `POST /api/servers/{id}/git/autosync`
- `GET /auth/github/start`
- `GET /auth/github/callback`

## UI

The server sidebar should include:

```html
<a href="/servers/{{id}}/version-control" class="yu-nav-item{% if active_tab == "version_control" %} active{% endif %}">
  <i class="bi bi-git"></i> Version Control
</a>
```

The tab should show repository URL, branch, GitHub connection status, current branch, last commit, short status output, and action buttons.
