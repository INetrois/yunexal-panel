// ── Row templates ────────────────────────────────────────────────────────────

// Backward-compatible no-op for optional integrations.
if (typeof window !== 'undefined' && typeof window._refreshSrvPortSelect !== 'function') {
    window._refreshSrvPortSelect = () => {};
}

function getPortRowHtml(host='', container='', proto='tcp') {
    const opt = v => `<option value="${v}"${proto===v?' selected':''}>`;
    return `
    <div class="entry-row" style="display:grid;grid-template-columns:1fr 28px 1fr 84px 26px;gap:.45rem;align-items:center;">
        <input type="number" min="1" max="65535" class="form-control host-input" placeholder="20000" value="${host}" oninput="updateYaml()">
        <span class="row-sep" style="text-align:center;">:</span>
        <input type="number" min="1" max="65535" class="form-control container-input" placeholder="20000" value="${container}" oninput="updateYaml()">
        <select class="form-select proto-sel proto-input" onchange="updateYaml()">
            ${opt('tcp')}TCP</option>
            ${opt('udp')}UDP</option>
            ${opt('tcp+udp')}Both</option>
        </select>
        <button type="button" class="row-del" onclick="this.closest('.entry-row').remove();updateYaml()" title="Remove">
            <i class="bi bi-trash3"></i>
        </button>
    </div>`;
}

function getEnvRowHtml(key='', val='') {
    return `
    <div class="entry-row" style="display:grid;grid-template-columns:1fr 20px 1fr 26px;gap:.45rem;align-items:center;">
        <input type="text" class="form-control key-input" placeholder="VARIABLE" value="${key}" oninput="updateYaml()">
        <span class="row-sep" style="text-align:center;">=</span>
        <input type="text" class="form-control val-input" placeholder="value" value="${val}" oninput="updateYaml()">
        <button type="button" class="row-del" onclick="this.closest('.entry-row').remove();updateYaml()" title="Remove">
            <i class="bi bi-trash3"></i>
        </button>
    </div>`;
}

// ── Add helpers ──────────────────────────────────────────────────────────────

function addPortRow(h, c, p) {
    document.getElementById('ports-container').insertAdjacentHTML('beforeend', getPortRowHtml(h, c, p));
    if (h === undefined) updateYaml();
}
function addEnvRow(k, v) {
    document.getElementById('env-container').insertAdjacentHTML('beforeend', getEnvRowHtml(k, v));
    if (k === undefined) updateYaml();
}

// ── YAML generation ──────────────────────────────────────────────────────────

function updateYaml() {
    const config = {
        image:       document.getElementById('image').value || undefined,
        restart:     document.getElementById('gui_restart').value,
        ports:       [],
        environment: []
    };

    const cpus = document.getElementById('gui_cpus').value;
    if (cpus) config.cpus = parseFloat(cpus);

    const memVal = document.getElementById('gui_mem_val').value;
    if (memVal) config.mem_limit = memVal + document.getElementById('gui_mem_unit').value;

    const diskVal = document.getElementById('gui_disk_val').value;
    if (diskVal) config.disk_limit = diskVal + document.getElementById('gui_disk_unit').value;

    document.querySelectorAll('#ports-container .entry-row').forEach(row => {
        const h = row.querySelector('.host-input').value;
        const c = row.querySelector('.container-input').value;
        const p = row.querySelector('.proto-input').value;
        if (h && c) config.ports.push(`${h}:${c}${p ? '/'+p : ''}`);
    });

    document.querySelectorAll('#env-container .entry-row').forEach(row => {
        const k = row.querySelector('.key-input').value;
        const v = row.querySelector('.val-input').value;
        if (k) config.environment.push(`${k}=${v}`);
    });

    if (!config.ports.length)       delete config.ports;
    if (!config.environment.length) delete config.environment;
    if (!config.image)              delete config.image;

    const yamlStr = jsyaml.dump(config, { indent: 2, lineWidth: -1 });
    document.getElementById('config').value = yamlStr;
    if (window.yamlEditor) {
        const pos = window.yamlEditor.getPosition();
        window.yamlEditor.setValue(yamlStr);
        if (pos) window.yamlEditor.setPosition(pos);
    }
    saveFormState();
    // Keep optional port select integrations in sync.
    if (typeof window._refreshSrvPortSelect === 'function') {
        window._refreshSrvPortSelect();
    }
}

function copyYaml(btn) {
    const val = document.getElementById('config').value;
    if (!val) return;
    navigator.clipboard.writeText(val).then(() => {
        btn.innerHTML = '<i class="bi bi-check2"></i> Copied!';
        btn.style.color = 'var(--success)';
        setTimeout(() => { btn.innerHTML = '<i class="bi bi-clipboard"></i> Copy'; btn.style.color = ''; }, 2000);
    });
}

// ── Local image picker (dropdown inside input) ───────────────────────────────

const _localImageTags = new Set();

function _sortedLocalImageTags() {
    return Array.from(_localImageTags).sort((a, b) => a.localeCompare(b));
}

function _addLocalImageTag(tag) {
    const t = String(tag || '').trim();
    if (!t || _localImageTags.has(t)) return;
    _localImageTags.add(t);

    const dl = document.getElementById('local-images-list');
    if (dl) {
        const opt = document.createElement('option');
        opt.value = t;
        dl.appendChild(opt);
    }
}

function renderImageDropdown() {
    const box = document.getElementById('image-dropdown');
    const imageEl = document.getElementById('image');
    if (!box || !imageEl) return;

    const q = imageEl.value.trim().toLowerCase();
    const tags = _sortedLocalImageTags()
        .filter(t => !q || t.toLowerCase().includes(q))
        .slice(0, 80);

    box.innerHTML = '';
    if (!tags.length) {
        const empty = document.createElement('div');
        empty.className = 'image-dropdown-empty';
        empty.textContent = 'No local images found';
        box.appendChild(empty);
        return;
    }

    tags.forEach(t => {
        const item = document.createElement('button');
        item.type = 'button';
        item.className = 'image-dropdown-item';
        item.textContent = t;
        item.dataset.value = t;
        box.appendChild(item);
    });
}

function openImageDropdown() {
    const box = document.getElementById('image-dropdown');
    if (!box) return;
    renderImageDropdown();
    box.classList.add('open');
}

function closeImageDropdown() {
    const box = document.getElementById('image-dropdown');
    if (box) box.classList.remove('open');
}

function toggleImageDropdown(event) {
    event.preventDefault();
    event.stopPropagation();
    const box = document.getElementById('image-dropdown');
    if (!box) return;
    if (box.classList.contains('open')) {
        closeImageDropdown();
    } else {
        openImageDropdown();
        document.getElementById('image')?.focus();
    }
}

function pickLocalImage(value) {
    if (!value) return;
    const imageEl = document.getElementById('image');
    if (imageEl) imageEl.value = value;
    closeImageDropdown();
    updateYaml();
    saveFormState();
}

function onImageInputChanged() {
    const box = document.getElementById('image-dropdown');
    if (box?.classList.contains('open')) renderImageDropdown();
    updateYaml();
}

function onImageInputKeydown(e) {
    if (e.key === 'ArrowDown') {
        e.preventDefault();
        openImageDropdown();
        return;
    }
    if (e.key === 'Escape') {
        closeImageDropdown();
    }
}

function loadLocalImages() {
    fetch('/api/image/local')
        .then(r => r.json())
        .then(d => {
            const tags = Array.from(new Set(d.tags || []))
                .map(v => String(v || '').trim())
                .filter(Boolean)
                .sort((a, b) => a.localeCompare(b));
            tags.forEach(_addLocalImageTag);
            renderImageDropdown();
        })
        .catch(() => {});
}

document.addEventListener('click', e => {
    const wrap = document.querySelector('.image-combo-wrap');
    if (wrap && !wrap.contains(e.target)) closeImageDropdown();
});

document.getElementById('image-dropdown')?.addEventListener('click', e => {
    const item = e.target.closest('.image-dropdown-item');
    if (!item) return;
    pickLocalImage(item.dataset.value || '');
});

loadLocalImages();

// ── Filesystem quota preflight ───────────────────────────────────────────────

let _quotaCreationBlocked = false;
let _quotaCreationReason = '';

function _setCreateButtonsBlocked(blocked) {
    const openBtn = document.getElementById('create-server-open-btn');
    const confirmBtn = document.getElementById('create-server-confirm-btn');
    [openBtn, confirmBtn].forEach((btn) => {
        if (!btn) return;
        btn.disabled = blocked;
        btn.style.opacity = blocked ? '.55' : '';
        btn.style.cursor = blocked ? 'not-allowed' : '';
        if (blocked) {
            btn.setAttribute('title', _quotaCreationReason || 'Quota must be configured before server creation');
        } else {
            btn.removeAttribute('title');
        }
    });
}

function _guardQuotaBeforeCreate() {
    if (!_quotaCreationBlocked) return true;
    const banner = document.getElementById('quota-warn-banner');
    if (banner) {
        banner.style.display = '';
        banner.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
    return false;
}

function renderQuotaPreflightBanner(d) {
    const banner = document.getElementById('quota-warn-banner');
    const text = document.getElementById('quota-warn-text');
    if (!banner || !text) return;

    if (d && d.unsafe_override) {
        _quotaCreationBlocked = false;
        _quotaCreationReason = '';
        _setCreateButtonsBlocked(false);
        text.textContent = 'Unsafe storage override is enabled in Admin Settings. Quota/safety checks are bypassed at your own risk.';
        banner.style.display = '';
        return;
    }

    if (d && d.ok) {
        _quotaCreationBlocked = false;
        _quotaCreationReason = '';
        _setCreateButtonsBlocked(false);
        banner.style.display = 'none';
        return;
    }

    let msg = 'No quota-capable filesystem detected (ext4 with prjquota). Server creation is blocked until quota support is configured.';
    if (d && d.ext4_without_prjquota) {
        msg = 'ext4 detected without prjquota/prjjquota. Server creation is blocked. Enable prjquota in mount options and remount before creating containers.';
    }

    _quotaCreationBlocked = true;
    _quotaCreationReason = msg;
    _setCreateButtonsBlocked(true);
    text.textContent = msg;
    banner.style.display = '';
}

async function loadQuotaPreflight() {
    try {
        const d = await fetch('/api/quota-check').then(r => r.json());
        renderQuotaPreflightBanner(d);
    } catch {
        renderQuotaPreflightBanner({ ok: false });
    }
}

loadQuotaPreflight();

// ── Custom storage selector ──────────────────────────────────────────────────

let _storMounts = []; // populated after fetch
let _storIdx    = 0;  // currently selected index
let _storUnsafeOverride = false;
let _storPosListenersAttached = false;
let _storPosRaf = null;

function _storPositionPanel() {
    const el = document.getElementById('stor-sel');
    const panel = document.getElementById('stor-sel-panel');
    if (!el || !panel || !el.classList.contains('open')) return;

    const rect = el.getBoundingClientRect();
    const gap = 5;
    const pad = 8;
    const width = Math.max(240, Math.round(rect.width));
    panel.style.width = `${width}px`;

    const panelHeight = panel.offsetHeight || 280;
    const spaceBelow = window.innerHeight - rect.bottom - pad;
    const spaceAbove = rect.top - pad;
    const placeAbove = spaceBelow < Math.min(220, panelHeight) && spaceAbove > spaceBelow;
    const maxHeight = Math.max(140, placeAbove ? spaceAbove : spaceBelow);

    let top = placeAbove ? (rect.top - panelHeight - gap) : (rect.bottom + gap);
    top = Math.max(pad, Math.min(top, window.innerHeight - Math.min(panelHeight, maxHeight) - pad));

    let left = rect.left;
    left = Math.max(pad, Math.min(left, window.innerWidth - width - pad));

    panel.style.maxHeight = `${Math.round(maxHeight)}px`;
    panel.style.top = `${Math.round(top)}px`;
    panel.style.left = `${Math.round(left)}px`;
}

function _storSchedulePosition() {
    if (_storPosRaf) cancelAnimationFrame(_storPosRaf);
    _storPosRaf = requestAnimationFrame(() => {
        _storPosRaf = null;
        _storPositionPanel();
    });
}

function _storAttachPositionListeners() {
    if (_storPosListenersAttached) return;
    window.addEventListener('scroll', _storSchedulePosition, true);
    window.addEventListener('resize', _storSchedulePosition);
    if (window.visualViewport) {
        window.visualViewport.addEventListener('scroll', _storSchedulePosition);
        window.visualViewport.addEventListener('resize', _storSchedulePosition);
    }
    _storPosListenersAttached = true;
}

function _storDetachPositionListeners() {
    if (!_storPosListenersAttached) return;
    window.removeEventListener('scroll', _storSchedulePosition, true);
    window.removeEventListener('resize', _storSchedulePosition);
    if (window.visualViewport) {
        window.visualViewport.removeEventListener('scroll', _storSchedulePosition);
        window.visualViewport.removeEventListener('resize', _storSchedulePosition);
    }
    if (_storPosRaf) {
        cancelAnimationFrame(_storPosRaf);
        _storPosRaf = null;
    }
    _storPosListenersAttached = false;
}

function _storIconClass(m) {
    if (m.is_default) return 'bi-folder2';
    const d = (m.device || '').toLowerCase();
    if (d.includes('nvme')) return 'bi-device-ssd-fill';
    if (d.includes('sd') || d.includes('hd')) return 'bi-hdd-fill';
    return 'bi-hdd-fill';
}

function _storBarColor(pct) {
    return pct > 80 ? '#f87171' : pct > 55 ? '#fbbf24' : '#34d399';
}

function _storRenderTrigger() {
    const m = _storMounts[_storIdx];
    if (!m) return;
    const iconEl   = document.getElementById('stor-sel-icon');
    const deviceEl = document.getElementById('stor-sel-device');
    const subEl    = document.getElementById('stor-sel-sub');
    const hidden   = document.getElementById('storage-path-hidden');
    if (iconEl)  { iconEl.className = 'bi ' + _storIconClass(m); }
    if (deviceEl) deviceEl.textContent = m.device;
    if (subEl)    subEl.textContent    = m.sub;
    if (hidden)   hidden.value         = m.value;
    _storRenderNote(m);
}

function _storRenderPanel() {
    const panel = document.getElementById('stor-sel-panel');
    if (!panel) return;
    panel.innerHTML = _storMounts.map((m, i) => {
        const icon    = _storIconClass(m);
        const badge = m.is_default
            ? ''
            : m.has_ext4
            ? (m.has_prjquota
                ? `<span class="stor-opt-badge ext4">ext4+prjquota</span>`
                : `<span class="stor-opt-badge nq">ext4 no quota</span>`)
            : `<span class="stor-opt-badge nq">no quota</span>`;
        const row2    = m.path2 ? `<div class="stor-opt-row2">${esc(m.path2)}</div>` : '';
        const barFill = m.total > 0
            ? `<div class="stor-opt-usage">
                 <div class="stor-opt-bar"><div class="stor-opt-bar-fill" style="width:${Math.min(m.pct,100)}%;background:${_storBarColor(m.pct)};"></div></div>
                 <span class="stor-opt-free">${m.free} free</span>
               </div>` : '';
        const check = i === _storIdx ? '<i class="bi bi-check2"></i>' : '';
        return `<div class="stor-opt${i === _storIdx ? ' active' : ''}" onclick="storSelPick(${i})">
            <div class="stor-opt-icon"><i class="bi ${icon}"></i></div>
            <div class="stor-opt-body">
                <div class="stor-opt-row1">
                    <span class="stor-opt-device">${esc(m.device)}</span>
                    <span class="stor-opt-mount">${esc(m.mount)}</span>
                    ${badge}
                </div>
                ${row2}
            </div>
            ${barFill}
            <div class="stor-opt-check">${check}</div>
        </div>`;
    }).join('');
}

function _storRenderNote(m) {
    const note = document.getElementById('storage-path-note');
    if (!note) return;
    if (!m || m.is_default) { note.innerHTML = ''; return; }
    if (m.has_ext4 && m.has_prjquota) {
        note.innerHTML = `<span style="color:#fb923c;"><i class="bi bi-check-circle"></i> ext4 prjquota active on <code style="font-size:.78em;">${esc(m.device)}</code> — disk limits enforced.</span>`;
    } else if (m.has_ext4) {
        if (_storUnsafeOverride) {
            note.innerHTML = `<span style="color:#fbbf24;"><i class="bi bi-exclamation-triangle"></i> ext4 without prjquota on <code style="font-size:.78em;">${esc(m.device)}</code> — allowed only because unsafe override is enabled.</span>`;
        } else {
            note.innerHTML = `<span style="color:#f87171;"><i class="bi bi-exclamation-triangle"></i> ext4 without prjquota on <code style="font-size:.78em;">${esc(m.device)}</code> — creation is blocked until <strong>prjquota</strong> is enabled and remounted.</span>`;
        }
    } else {
        if (_storUnsafeOverride) {
            note.innerHTML = `<span style="color:#fbbf24;"><i class="bi bi-exclamation-triangle"></i> Quotas are unavailable on <code style="font-size:.78em;">${esc(m.device)}</code> — allowed via unsafe override (at your own risk).</span>`;
        } else {
            note.innerHTML = `<span style="color:#f87171;"><i class="bi bi-exclamation-triangle"></i> Quotas are unavailable on <code style="font-size:.78em;">${esc(m.device)}</code> — disk limits will <strong>not</strong> be enforced.</span>`;
        }
    }
}

function storSelToggle(e) {
    if (e) e.stopPropagation();
    const el    = document.getElementById('stor-sel');
    const panel = document.getElementById('stor-sel-panel');
    if (!el || !panel) return;
    if (el.classList.contains('open')) {
        el.classList.remove('open');
        _storDetachPositionListeners();
    } else {
        _storRenderPanel(); // refresh marks
        el.classList.add('open');
        _storPositionPanel();
        _storAttachPositionListeners();
    }
}

function storSelClose() {
    const el = document.getElementById('stor-sel');
    if (el) el.classList.remove('open');
    _storDetachPositionListeners();
}

function storSelPick(idx) {
    _storIdx = idx;
    _storRenderTrigger();
    storSelClose();
}

// Close on outside click
document.addEventListener('click', e => {
    const el = document.getElementById('stor-sel');
    if (el && !el.contains(e.target)) storSelClose();
});

// ── Fetch and initialise ─────────────────────────────────────────────────────
(function loadStorMounts() {
    // Default-only fallback
    function initDefault(hint) {
        _storUnsafeOverride = false;
        _storMounts = [{
            device: 'Default', mount: '', sub: hint || 'volumes/ in panel directory',
            path2: '', value: '', is_default: true, has_prjquota: false,
            has_ext4: false,
            free: '--', total: 0, pct: 0,
        }];
        _storIdx = 0;
        _storRenderTrigger();
    }

    initDefault('loading…');

    fetch('/api/admin/storage/mounts', { credentials: 'same-origin' })
        .then(r => r.ok ? r.json() : null)
        .then(d => {
            if (!d || !d.ok) { initDefault(); return; }
            _storUnsafeOverride = !!d.unsafe_override;
            const cp = d.current_path || '';
            _storMounts = [{
                device: 'Default', mount: '',
                sub: cp ? `panel default → ${cp}` : 'panel default  (volumes/)',
                path2: '', value: '', is_default: true, has_prjquota: false,
                has_ext4: false,
                free: '--', total: 0, pct: 0,
            }];
            (d.mounts || []).forEach(m => {
                const sp = m.suggested_path || '';
                _storMounts.push({
                    device:       m.device || 'disk',
                    mount:        m.mount  || '',
                    sub:          sp || m.mount,
                    path2:        sp ? `→ ${sp}` : '',
                    value:        sp,
                    is_default:   false,
                    has_prjquota: !!m.has_prjquota,
                    has_ext4:     !!m.has_ext4,
                    free:         `${m.free_gib} GiB`,
                    total:        parseFloat(m.total_gib) || 0,
                    pct:          m.used_pct || 0,
                });
            });
            _storIdx = 0;
            _storRenderTrigger();
        })
        .catch(() => initDefault());
})();

// ── Fetch ENV ────────────────────────────────────────────────────────────────

async function fetchImageEnv() {
    const image = document.getElementById('image').value.trim();
    if (!image) { alert('Enter a Docker image name first.'); return; }
    const btn    = document.getElementById('fetch-env-btn');
    const status = document.getElementById('fetch-env-status');
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" style="width:.8rem;height:.8rem;"></span> Loading…';
    status.style.color = '';
    status.textContent = '';
    try {
        const enc = encodeURIComponent(image);
        const overridesRes = await fetch(`/api/image/env-overrides?image=${enc}`).then(r => r.json()).catch(() => ({ ok: false, env: '' }));
        const dbEnv = (overridesRes.ok && overridesRes.env) ? overridesRes.env.trim() : '';

        const map = new Map();
        if (dbEnv) {
            for (const line of dbEnv.split('\n')) {
                const t = line.trim(); if (!t) continue;
                const eq = t.indexOf('=');
                map.set(eq >= 0 ? t.slice(0, eq) : t, eq >= 0 ? t.slice(eq + 1) : '');
            }
        } else {
            btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" style="width:.8rem;height:.8rem;"></span> Pulling image…';
            const nativeRes = await fetch(`/api/image/env?image=${enc}`).then(r => r.json());
            if (!nativeRes.ok) throw new Error(nativeRes.error || 'Unknown error');
            for (const pair of (nativeRes.env || [])) {
                const eq = pair.indexOf('=');
                map.set(eq >= 0 ? pair.slice(0, eq) : pair, eq >= 0 ? pair.slice(eq + 1) : '');
            }
        }

        const existing = new Set(
            Array.from(document.querySelectorAll('#env-container .key-input')).map(el => el.value)
        );
        let added = 0;
        for (const [k, v] of map) {
            if (!existing.has(k)) { addEnvRow(k, v); existing.add(k); added++; }
        }
        status.style.color = 'var(--success)';
        status.textContent = added > 0
            ? `✓ Added ${added} var${added !== 1 ? 's' : ''}${dbEnv ? ' (from DB)' : ''}`
            : '✓ No new vars';
    } catch (e) {
        status.style.color = 'var(--danger)';
        status.textContent = `✗ ${e.message}`;
    } finally {
        btn.disabled = false;
        btn.innerHTML = '<i class="bi bi-cloud-download"></i> Fetch ENV';
        setTimeout(() => { status.textContent = ''; }, 4000);
    }
}

async function buildImageFromDockerfile() {
    const imageEl = document.getElementById('image');
    const customTagEl = document.getElementById('dockerfile-image-tag');
    const fileEl = document.getElementById('dockerfile-upload');
    const btn = document.getElementById('build-dockerfile-btn');
    const status = document.getElementById('dockerfile-build-status');

    const customTag = (customTagEl?.value || '').trim();
    const image = customTag || (imageEl?.value || '').trim();
    const file = fileEl?.files?.[0];

    if (!image) {
        if (status) {
            status.style.color = 'var(--danger)';
            status.textContent = '✗ Enter a Docker image tag first (e.g. my-custom:latest).';
        }
        imageEl?.focus();
        return;
    }
    if (!file) {
        if (status) {
            status.style.color = 'var(--danger)';
            status.textContent = '✗ Choose a Dockerfile to upload.';
        }
        return;
    }

    const form = new FormData();
    form.append('image', image);
    form.append('dockerfile', file, file.name || 'Dockerfile');

    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" style="width:.8rem;height:.8rem;"></span> Building…';
    if (status) {
        status.style.color = '';
        status.textContent = '';
    }

    try {
        const resp = await fetch('/api/image/build-dockerfile', {
            method: 'POST',
            credentials: 'same-origin',
            body: form,
        });
        const data = await resp.json().catch(() => ({}));
        if (!resp.ok || !data.ok) {
            throw new Error(data.error || 'Build failed');
        }

        // Ensure the freshly-built image appears in local autocomplete + dropdown immediately.
        _addLocalImageTag(image);

        if (status) {
            status.style.color = 'var(--success)';
            status.textContent = `✓ Built local image ${image}`;
        }
        if (imageEl && imageEl.value.trim() !== image) {
            imageEl.value = image;
        }
        renderImageDropdown();
        if (fileEl) fileEl.value = '';
        updateYaml();
        saveFormState();
    } catch (e) {
        if (status) {
            status.style.color = 'var(--danger)';
            status.textContent = `✗ ${e.message || 'Build failed'}`;
        }
    } finally {
        btn.disabled = false;
        btn.innerHTML = '<i class="bi bi-hammer"></i> Build Local Image';
    }
}

// ── Monaco init ──────────────────────────────────────────────────────────────

if (!window.__monacoEditorReady) {
    window.__monacoEditorReady = true;
    require.config({ paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs' } });
    require(['vs/editor/editor.main'], function () {
        window.yamlEditor = monaco.editor.create(document.getElementById('yaml-editor-container'), {
            value: '',
            language: 'yaml',
            theme: 'vs-dark',
            automaticLayout: true,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
            fontSize: 12,
            lineHeight: 19,
            lineNumbers: 'on',
            renderLineHighlight: 'gutter',
            roundedSelection: true,
            scrollbar: { verticalScrollbarSize: 4, horizontalScrollbarSize: 4 },
            padding: { top: 10, bottom: 10 },
            wordWrap: 'off',
            overviewRulerLanes: 0,
            hideCursorInOverviewRuler: true,
            overviewRulerBorder: false,
            glyphMargin: false,
            folding: true,
            renderWhitespace: 'none',
            bracketPairColorization: { enabled: true },
        });
        window.yamlEditor.onDidChangeModelContent(() => {
            document.getElementById('config').value = window.yamlEditor.getValue();
        });
        updateYaml();
    });
}

// ── Helpers ──────────────────────────────────────────────────────────────────
function esc(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;')
        .replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
}

const _adjectives = ['fast','cool','epic','dark','blue','red','gold','iron','neon','soft','wild','bold','calm','lazy','tiny'];
const _nouns      = ['server','node','box','host','core','unit','base','hub','rack','cloud','forge','block','spark','tower','realm'];
function randomServerName() {
    const a = _adjectives[Math.floor(Math.random() * _adjectives.length)];
    const n = _nouns[Math.floor(Math.random() * _nouns.length)];
    const d = Math.floor(1000 + Math.random() * 9000);
    return `${a}-${n}-${d}`;
}
function autoFillName() {
    const el = document.getElementById('name');
    if (!el.value.trim()) el.value = randomServerName();
}

// ── Form persistence (sessionStorage) ───────────────────────────────────
const FORM_KEY = 'yunexal_new_server_draft';

function saveFormState() {
    const ports = [], envs = [];
    document.querySelectorAll('#ports-container .entry-row').forEach(r => {
        ports.push({ h: r.querySelector('.host-input').value, c: r.querySelector('.container-input').value, p: r.querySelector('.proto-input').value });
    });
    document.querySelectorAll('#env-container .entry-row').forEach(r => {
        envs.push({ k: r.querySelector('.key-input').value, v: r.querySelector('.val-input').value });
    });
    const collapsed = {};
    document.querySelectorAll('.sec-card').forEach((card, i) => {
        const bd = card.querySelector('.sec-bd');
        if (bd) collapsed[i] = bd.style.display === 'none';
    });
    sessionStorage.setItem(FORM_KEY, JSON.stringify({
        name:          document.getElementById('name')?.value || '',
        owner_id:      document.getElementById('owner_id')?.value || '0',
        image:         document.getElementById('image')?.value || '',
        dockerfile_image_tag: document.getElementById('dockerfile-image-tag')?.value || '',
        gui_cpus:      document.getElementById('gui_cpus')?.value || '',
        gui_mem_val:   document.getElementById('gui_mem_val')?.value || '',
        gui_mem_unit:  document.getElementById('gui_mem_unit')?.value || 'mb',
        gui_disk_val:  document.getElementById('gui_disk_val')?.value || '',
        gui_disk_unit: document.getElementById('gui_disk_unit')?.value || 'gb',
        bandwidth_mbit:document.getElementById('bandwidth_mbit')?.value || '',
        gui_restart:   document.getElementById('gui_restart')?.value || 'unless-stopped',
        ports, envs, collapsed,
    }));
}

function restoreFormState() {
    const raw = sessionStorage.getItem(FORM_KEY);
    if (!raw) return;
    let s; try { s = JSON.parse(raw); } catch { return; }
    const sv = (id, val) => { const el = document.getElementById(id); if (el && val !== undefined && val !== '') el.value = val; };
    sv('name', s.name);
    sv('owner_id', s.owner_id);
    sv('image', s.image);
    renderImageDropdown();
    sv('dockerfile-image-tag', s.dockerfile_image_tag);
    sv('gui_cpus', s.gui_cpus);
    sv('gui_mem_val', s.gui_mem_val);
    sv('gui_mem_unit', s.gui_mem_unit);
    sv('gui_disk_val', s.gui_disk_val);
    sv('gui_disk_unit', s.gui_disk_unit);
    sv('bandwidth_mbit', s.bandwidth_mbit);
    sv('gui_restart', s.gui_restart);
    (s.ports || []).forEach(r => addPortRow(r.h, r.c, r.p));
    (s.envs  || []).forEach(r => addEnvRow(r.k, r.v));
    if (s.collapsed) {
        document.querySelectorAll('.sec-card').forEach((card, i) => {
            if (!(i in s.collapsed)) return;
            const bd   = card.querySelector('.sec-bd');
            const chev = card.querySelector('.sec-chev');
            if (!bd) return;
            bd.style.display = s.collapsed[i] ? 'none' : '';
            if (chev) chev.className = 'bi ' + (s.collapsed[i] ? 'bi-chevron-down' : 'bi-chevron-up') + ' sec-chev';
        });
    }
}

function clearFormState() {
    sessionStorage.removeItem(FORM_KEY);
}

// ── Sec-card generic collapse ───────────────────────────────────────────────
function toggleSecCard(hd) {
    const body = hd.closest('.sec-card').querySelector('.sec-bd');
    if (!body) return;
    const open = body.style.display !== 'none';
    body.style.display = open ? 'none' : '';
    const chev = hd.querySelector('.sec-chev');
    if (chev) chev.className = 'bi ' + (open ? 'bi-chevron-down' : 'bi-chevron-up') + ' sec-chev';
    if (chev) chev.style.cssText = 'margin-left:auto;color:var(--muted);transition:transform .2s;';
    saveFormState();
}

// ── Confirm Create Modal ────────────────────────────────────────────────────
function _cfmSection(icon, title, content) {
    return `<div style="background:var(--surface2);border:1px solid var(--bdr);border-radius:12px;overflow:hidden;">
        <div style="display:flex;align-items:center;gap:.5rem;padding:.6rem 1rem;border-bottom:1px solid var(--bdr);background:var(--surface3);">
            <i class="bi ${icon}" style="color:var(--accent-l);font-size:.8rem;"></i>
            <span style="font-size:.69rem;font-weight:600;text-transform:uppercase;letter-spacing:.07em;color:var(--muted);">${title}</span>
        </div>
        <div style="padding:.75rem 1rem;">${content}</div>
    </div>`;
}
function _cfmKV(label, value, accent) {
    return `<div style="display:flex;align-items:baseline;justify-content:space-between;gap:.5rem;padding:.22rem 0;border-bottom:1px solid rgba(255,255,255,.035);min-width:0;">
        <span style="font-size:.76rem;color:var(--muted);white-space:nowrap;flex-shrink:0;">${label}</span>
        <span style="font-size:.8rem;font-weight:500;color:${accent||'var(--txt)'};text-align:right;word-break:break-all;overflow-wrap:anywhere;min-width:0;">${value}</span>
    </div>`;
}
function _cfmBadge(text, color) {
    return `<span style="display:inline-block;background:${color||'rgba(124,58,237,.15)'};border:1px solid ${color ? color.replace('.15',',.35') : 'rgba(124,58,237,.25)'};color:${color ? '#fff' : 'var(--accent-l)'};font-size:.72rem;font-family:monospace;padding:.15rem .55rem;border-radius:5px;margin:.15rem .15rem 0 0;">${text}</span>`;
}
function _cfmEmpty(msg) {
    return `<span style="font-size:.75rem;color:var(--muted);font-style:italic;">${msg}</span>`;
}

function showCreateConfirm() {
    if (!_guardQuotaBeforeCreate()) return;

    // Auto-fill name if empty
    const nameEl = document.getElementById('name');
    if (!nameEl.value.trim()) nameEl.value = randomServerName();

    // Require docker image
    const imageEl = document.getElementById('image');
    if (!imageEl.value.trim()) {
        imageEl.focus();
        imageEl.style.borderColor = 'rgba(239,68,68,.6)';
        imageEl.style.boxShadow   = '0 0 0 3px rgba(239,68,68,.15)';
        const basicBd = imageEl.closest('.sec-bd');
        if (basicBd && basicBd.style.display === 'none') {
            basicBd.style.display = '';
            const chev = basicBd.closest('.sec-card')?.querySelector('.sec-chev');
            if (chev) chev.className = 'bi bi-chevron-up sec-chev';
        }
        return;
    }
    imageEl.style.borderColor = '';
    imageEl.style.boxShadow   = '';

    updateYaml();

    const name     = document.getElementById('name').value.trim() || '—';
    const ownerSel = document.getElementById('owner_id');
    const ownerTxt = ownerSel.options[ownerSel.selectedIndex]?.text || '—';
    const image    = document.getElementById('image').value.trim() || '—';
    const restart  = document.getElementById('gui_restart').value;
    const cpus     = document.getElementById('gui_cpus').value;
    const memVal   = document.getElementById('gui_mem_val').value;
    const memUnit  = document.getElementById('gui_mem_unit').value.toUpperCase();
    const diskVal  = document.getElementById('gui_disk_val').value;
    const diskUnit = document.getElementById('gui_disk_unit').value.toUpperCase();
    const bw       = document.getElementById('bandwidth_mbit').value;

    const restartColor = { 'always':'rgba(34,197,94,.15)', 'unless-stopped':'rgba(234,179,8,.15)', 'on-failure':'rgba(239,68,68,.15)', 'no':'rgba(107,114,128,.15)' }[restart] || 'rgba(124,58,237,.15)';
    const restartBorder= { 'always':'rgba(34,197,94,.35)', 'unless-stopped':'rgba(234,179,8,.35)', 'on-failure':'rgba(239,68,68,.35)', 'no':'rgba(107,114,128,.35)' }[restart] || 'rgba(124,58,237,.35)';
    const restartTxt   = { 'always':'#86efac',            'unless-stopped':'#fde047',             'on-failure':'#fca5a5',            'no':'#9ca3af'            }[restart] || 'var(--accent-l)';
    const restartBadge = `<span style="display:inline-block;background:${restartColor};border:1px solid ${restartBorder};color:${restartTxt};font-size:.72rem;padding:.15rem .55rem;border-radius:5px;">${esc(restart)}</span>`;

    const sections = [];

    // ── Basic Info ──
    sections.push(_cfmSection('bi-tag-fill', 'Basic Info',
        _cfmKV('Server Name', `<strong style="color:var(--txt);font-family:monospace;letter-spacing:.02em;">${esc(name)}</strong>`) +
        _cfmKV('Owner', esc(ownerTxt)) +
        _cfmKV('Docker Image', `<code style="color:#a78bfa;font-size:.78rem;">${esc(image)}</code>`)
    ));

    // ── Resources ──
    sections.push(_cfmSection('bi-cpu-fill', 'Resources & Limits',
        _cfmKV('CPU',       cpus    ? `<span style="color:var(--success);">${esc(cpus)} core${parseFloat(cpus)!==1?'s':''}</span>` : _cfmEmpty('Unlimited')) +
        _cfmKV('RAM',       memVal  ? `<span style="color:var(--success);">${esc(memVal)}\u202f${memUnit}</span>` : _cfmEmpty('Unlimited')) +
        _cfmKV('Disk',      diskVal ? `<span style="color:var(--success);">${esc(diskVal)}\u202f${diskUnit}</span>` : _cfmEmpty('Unlimited')) +
        _cfmKV('Bandwidth', bw      ? `<span style="color:var(--success);">${esc(bw)} Mbit/s</span>` : _cfmEmpty('Unlimited')) +
        _cfmKV('Restart',   restartBadge)
    ));

    // ── Ports ──
    const ports = [];
    document.querySelectorAll('#ports-container .entry-row').forEach(r => {
        const h = r.querySelector('.host-input').value;
        const c = r.querySelector('.container-input').value;
        const p = r.querySelector('.proto-input').value;
        if (h && c) ports.push({ h, c, p });
    });
    {
        const inner = ports.length
            ? `<div style="display:grid;grid-template-columns:auto auto auto;gap:.3rem .6rem;align-items:center;font-size:.78rem;font-family:monospace;">
                <span style="font-size:.67rem;font-weight:600;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);">Host</span>
                <span style="font-size:.67rem;font-weight:600;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);">Container</span>
                <span style="font-size:.67rem;font-weight:600;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);">Proto</span>
                ${ports.map(({h,c,p}) =>
                    `<span style="color:#a78bfa;">${esc(h)}</span><span style="color:var(--txt);">→ ${esc(c)}</span><span style="background:rgba(124,58,237,.15);border:1px solid rgba(124,58,237,.25);color:var(--accent-l);padding:.1rem .4rem;border-radius:4px;font-size:.7rem;">${esc(p)}</span>`
                ).join('')}
            </div>`
            : _cfmEmpty('No port bindings configured');
        sections.push(_cfmSection('bi-diagram-2-fill', `Port Bindings${ports.length ? ' ('+ports.length+')' : ''}`, inner));
    }

    // ── Environment ──
    const envs = [];
    document.querySelectorAll('#env-container .entry-row').forEach(r => {
        const k = r.querySelector('.key-input').value;
        const v = r.querySelector('.val-input').value;
        if (k) envs.push({ k, v });
    });
    {
        const inner = envs.length
            ? `<div style="display:flex;flex-direction:column;gap:.25rem;">${envs.map(({k,v}) =>
                `<div style="display:flex;align-items:baseline;gap:.4rem;padding:.25rem .4rem;background:var(--surface3);border-radius:6px;font-family:monospace;font-size:.77rem;overflow:hidden;">
                    <span style="color:#a78bfa;white-space:nowrap;flex-shrink:0;">${esc(k)}</span>
                    <span style="color:var(--muted);flex-shrink:0;">=</span>
                    <span style="color:var(--txt);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${esc(v)||'<em style="opacity:.5;">empty</em>'}</span>
                </div>`
            ).join('')}</div>`
            : _cfmEmpty('No environment variables');
        sections.push(_cfmSection('bi-code-square', `Environment${envs.length ? ' ('+envs.length+')' : ''}`, inner));
    }

    document.getElementById('confirm-summary').innerHTML = sections.join('');
    document.getElementById('confirmCreateModal').style.display = 'block';}

function hideCreateConfirm() {
    document.getElementById('confirmCreateModal').style.display = 'none';
}

function submitCreateForm() {
    if (!_guardQuotaBeforeCreate()) return;
    clearFormState();
    document.getElementById('createServerForm').submit();
}

document.getElementById('createServerForm')?.addEventListener('submit', (e) => {
    if (!_guardQuotaBeforeCreate()) {
        e.preventDefault();
    }
});
