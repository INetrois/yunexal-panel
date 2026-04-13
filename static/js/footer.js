(function () {
    // ── Global URL helpers (no implicit route hashes) ───────────────────────
    const _FILES_ROUTE_RE = /^\/servers\/\d+\/files(?:\/edit)?\/?$/;
    const _FILES_HASH_RE = /^(?:path=|%2f|servers-\d+-files(?:@|$))/i;
    const _LEGACY_ROUTE_HASH_RE = /^(?:route:)?[a-z0-9_-]+(?:-[a-z0-9_-]+)*$/i;

    function _shouldStripLegacyRouteHash(pathname, hashValue) {
        if (!hashValue) return false;
        if (_FILES_ROUTE_RE.test(pathname)) return false;
        if (_FILES_HASH_RE.test(hashValue)) return false;
        return _LEGACY_ROUTE_HASH_RE.test(hashValue);
    }

    function _toRouteHashUrl(rawUrl) {
        try {
            const url = new URL(rawUrl, window.location.origin);
            if (url.origin !== window.location.origin) return rawUrl;
            const hashValue = (url.hash || '').replace(/^#/, '');
            if (_shouldStripLegacyRouteHash(url.pathname, hashValue)) {
                return `${url.pathname}${url.search}`;
            }
            return `${url.pathname}${url.search}${url.hash}`;
        } catch {
            return rawUrl;
        }
    }

    function _cleanupCurrentLegacyRouteHash() {
        const hashValue = (window.location.hash || '').replace(/^#/, '');
        if (!_shouldStripLegacyRouteHash(window.location.pathname, hashValue)) return;
        history.replaceState(
            history.state,
            '',
            `${window.location.pathname}${window.location.search}`,
        );
    }

    window.yuRouteUrl = function (rawUrl) {
        return _toRouteHashUrl(rawUrl);
    };

    window.yuGo = function (rawUrl, replace) {
        const next = _toRouteHashUrl(rawUrl);
        if (replace) window.location.replace(next);
        else window.location.href = next;
    };

    function _annotateAnchorHref(a) {
        if (!a || !a.getAttribute) return;
        // Sidebar navigation should keep original clean route URLs.
        if (a.closest && a.closest('.yu-sidebar')) return;
        const raw = a.getAttribute('href');
        if (!raw) return;
        if (raw.startsWith('#') || raw.startsWith('mailto:') || raw.startsWith('tel:') || raw.startsWith('javascript:')) return;
        const next = _toRouteHashUrl(raw);
        if (next !== raw) a.setAttribute('href', next);
    }

    function _annotateAnchors(root) {
        if (!root || !root.querySelectorAll) return;
        root.querySelectorAll('a[href]').forEach(_annotateAnchorHref);
    }

    function _initRouteHashAnnotation() {
        _annotateAnchors(document);
        const mo = new MutationObserver((mutations) => {
            for (const m of mutations) {
                for (const n of m.addedNodes) {
                    if (!n || n.nodeType !== Node.ELEMENT_NODE) continue;
                    if (n.matches && n.matches('a[href]')) _annotateAnchorHref(n);
                    _annotateAnchors(n);
                }
            }
        });
        mo.observe(document.body, { childList: true, subtree: true });
    }

    // ── Manifest + Service Worker ──
    const _mlink = document.createElement('link');
    _mlink.rel = 'manifest';
    _mlink.href = '/manifest.json';
    document.head.appendChild(_mlink);

    if ('serviceWorker' in navigator) {
        navigator.serviceWorker.register('/sw.js').catch(() => {});
    }

    _cleanupCurrentLegacyRouteHash();

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', _initRouteHashAnnotation, { once: true });
    } else {
        _initRouteHashAnnotation();
    }

    // ── Global confirm dialog ─────────────────────────────────────────────────
    // Inject once into body (available on every page since footer.js is universal)
    const _cm = document.createElement('div');
    _cm.innerHTML = [
        '<div class="modal fade" id="yu-confirm-modal" tabindex="-1" aria-hidden="true">',
        '  <div class="modal-dialog modal-dialog-centered modal-sm">',
        '    <div id="yu-confirm-content" class="modal-content">',
        '      <div class="modal-body p-4">',
        '        <div class="d-flex align-items-start gap-3 mb-2">',
        '          <div id="yu-confirm-icon" style="font-size:1.25rem;line-height:1.2;flex-shrink:0;"></div>',
        '          <p class="mb-0" id="yu-confirm-msg" style="font-size:.88rem;line-height:1.55;"></p>',
        '        </div>',
        '        <p id="yu-confirm-sub" style="font-size:.75rem;margin:.4rem 0 1.1rem;"></p>',
        '        <div class="d-flex justify-content-end gap-2">',
        '          <button type="button" id="yu-confirm-cancel" class="btn-yu btn-yu-ghost" style="font-size:.82rem;padding:.3rem .9rem;">Cancel</button>',
        '          <button type="button" id="yu-confirm-ok" style="border-radius:7px;padding:.3rem .9rem;font-size:.82rem;font-weight:500;cursor:pointer;transition:background .12s;"></button>',
        '        </div>',
        '      </div>',
        '    </div>',
        '  </div>',
        '</div>',
    ].join('');
    document.body.appendChild(_cm.firstElementChild);

    window.yuConfirm = function (msg, opts) {
        opts = Object.assign({
            icon: 'bi-trash3-fill', iconColor: '#f87171',
            subtitle: 'This cannot be undone.',
            okLabel: 'Delete',
            okColor: 'rgba(239,68,68,.15)', okBorder: 'rgba(239,68,68,.3)',
            okText:  '#fca5a5',            okHover: 'rgba(239,68,68,.28)',
        }, opts || {});
        return new Promise(function (resolve) {
            const modal     = document.getElementById('yu-confirm-modal');
            const content   = document.getElementById('yu-confirm-content');
            const iconEl    = document.getElementById('yu-confirm-icon');
            const msgEl     = document.getElementById('yu-confirm-msg');
            const subEl     = document.getElementById('yu-confirm-sub');
            const okBtn     = document.getElementById('yu-confirm-ok');
            const cancelBtn = document.getElementById('yu-confirm-cancel');
            if (!modal || typeof bootstrap === 'undefined') { resolve(window.confirm(msg)); return; }
            if (msgEl)    { msgEl.textContent = msg; msgEl.style.color = 'var(--txt,#e2e8f0)'; }
            if (iconEl)   { iconEl.innerHTML = '<i class="bi ' + opts.icon + '"></i>'; iconEl.style.color = opts.iconColor; }
            if (subEl)    { subEl.textContent = opts.subtitle || ''; subEl.style.display = opts.subtitle ? '' : 'none'; subEl.style.color = 'var(--muted,#94a3b8)'; }
            if (content)  { content.style.border = '1px solid ' + opts.okBorder; }
            if (okBtn) {
                okBtn.textContent = opts.okLabel;
                okBtn.style.cssText = 'border-radius:7px;padding:.3rem .9rem;font-size:.82rem;font-weight:500;cursor:pointer;transition:background .12s;background:' + opts.okColor + ';color:' + opts.okText + ';border:1px solid ' + opts.okBorder + ';';
                okBtn.onmouseover = function () { okBtn.style.background = opts.okHover; };
                okBtn.onmouseout  = function () { okBtn.style.background = opts.okColor; };
            }
            var bs = new bootstrap.Modal(modal, { backdrop: 'static', keyboard: false });
            var _done = false;
            function settle(v) { if (_done) return; _done = true; bs.hide(); resolve(v); }
            okBtn.addEventListener('click',    function () { settle(true);  }, { once: true });
            cancelBtn.addEventListener('click', function () { settle(false); }, { once: true });
            modal.addEventListener('hidden.bs.modal', function () { settle(false); }, { once: true });
            bs.show();
        });
    };

    // ── Load time footer ──
    window.addEventListener('load', function () {
        const ms = performance.now();
        const label = (ms / 1000).toFixed(3) + 's';

        const el = document.createElement('div');
        el.id = 'yu-footer';
        el.title = ms.toFixed(0) + ' ms';
        el.textContent = label;
        el.style.cssText = [
            'position:fixed',
            'bottom:.85rem',
            'right:1rem',
            'z-index:9000',
            'font-family:\'Inter\',\'Menlo\',monospace',
            'font-size:.7rem',
            'font-weight:600',
            'letter-spacing:.02em',
            'color:rgba(160,140,220,.45)',
            'background:rgba(255,255,255,.03)',
            'border:1px solid rgba(255,255,255,.06)',
            'border-radius:6px',
            'padding:.18rem .55rem',
            'pointer-events:none',
            'user-select:none',
            'transition:opacity .3s',
        ].join(';');

        document.body.appendChild(el);
    });
})();
