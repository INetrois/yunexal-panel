// Console page: terminal, WebSocket, controls, metrics charts
// Requires YU_SERVER_ID to be set inline in the template before this script loads.

// ── Sidebar toggle (mobile) ──────────────────────────────────────────────────
function openSidebar() {
    document.getElementById('sidebar').classList.add('open');
    document.getElementById('sbOverlay').classList.add('open');
    setTimeout(() => { if (window.fitAddonRef) window.fitAddonRef.fit(); }, 280);
}
function closeSidebar() {
    document.getElementById('sidebar').classList.remove('open');
    document.getElementById('sbOverlay').classList.remove('open');
    setTimeout(() => { if (window.fitAddonRef) window.fitAddonRef.fit(); }, 280);
}

// ── Terminal setup ────────────────────────────────────────────────────────────

// ── HTML → ANSI converter (for servers like Vintage Story that output HTML) ───
function htmlToAnsi(text) {
    // Quick exit: no HTML tags at all
    if (!/<[a-zA-Z\/]/.test(text)) return text;

    const colorMap = {
        'red':     '\x1b[31m', 'green':   '\x1b[32m', 'yellow':  '\x1b[33m',
        'blue':    '\x1b[34m', 'magenta': '\x1b[35m', 'cyan':    '\x1b[36m',
        'white':   '\x1b[37m', 'black':   '\x1b[30m', 'gray':    '\x1b[90m',
        'grey':    '\x1b[90m', 'orange':  '\x1b[33m', 'pink':    '\x1b[35m',
        'lime':    '\x1b[92m', 'aqua':    '\x1b[96m', 'silver':  '\x1b[37m',
        '#ff0000': '\x1b[31m', '#00ff00': '\x1b[32m', '#0000ff': '\x1b[34m',
        '#ffff00': '\x1b[33m', '#ff00ff': '\x1b[35m', '#00ffff': '\x1b[36m',
    };

    let out = text;
    // <b> / <strong> → bold
    out = out.replace(/<(b|strong)\b[^>]*>/gi, '\x1b[1m');
    out = out.replace(/<\/(b|strong)>/gi, '\x1b[22m');
    // <i> / <em> → italic
    out = out.replace(/<(i|em)\b[^>]*>/gi, '\x1b[3m');
    out = out.replace(/<\/(i|em)>/gi, '\x1b[23m');
    // <u> → underline
    out = out.replace(/<u\b[^>]*>/gi, '\x1b[4m');
    out = out.replace(/<\/u>/gi, '\x1b[24m');
    // <span style="color:..."> → ANSI color
    out = out.replace(/<span\b[^>]*style\s*=\s*["'][^"']*color\s*:\s*([^;"']+)[^"']*["'][^>]*>/gi,
        function (_, c) {
            const col = c.trim().toLowerCase();
            return colorMap[col] || '';
        });
    out = out.replace(/<\/span>/gi, '\x1b[0m');
    // <font color="..."> → ANSI color
    out = out.replace(/<font\b[^>]*color\s*=\s*["']?\s*([^"'\s>]+)\s*["']?[^>]*>/gi,
        function (_, c) {
            return colorMap[c.trim().toLowerCase()] || '';
        });
    out = out.replace(/<\/font>/gi, '\x1b[0m');
    // <br> / <br/> → newline
    out = out.replace(/<br\s*\/?>/gi, '\n');
    // Strip all remaining HTML tags
    out = out.replace(/<[^>]+>/g, '');
    // Decode common HTML entities
    out = out.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>')
             .replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&nbsp;/g, ' ');
    return out;
}

const term = new Terminal({
    cursorBlink: false,
    theme: { background: '#000000', foreground: '#f0f0f0', cursor: '#000000', cursorAccent: '#000000' },
    fontFamily: 'Menlo, Monaco, "Courier New", monospace',
    fontSize: window.innerWidth <= 575 ? 12 : 14,
    convertEol: true,
    scrollback: 200,
});
const fitAddon = new FitAddon.FitAddon();
window.fitAddonRef = fitAddon;
term.loadAddon(fitAddon);
term.open(document.getElementById('terminal'));
setTimeout(() => fitAddon.fit(), 100);
const _resizeHandler = () => fitAddon.fit();
window.addEventListener('resize', _resizeHandler);

// ── WebSocket ─────────────────────────────────────────────────────────────────
let ws = null;
let reconnectTimer = null;
let hasConnectedOnce = false;
let _wsRetryCount = 0;

function connectConsole() {
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    ws = new WebSocket(`${protocol}://${window.location.host}/api/servers/${YU_SERVER_ID}/ws`);
    ws.binaryType = 'arraybuffer';

    ws.onopen = () => {
        _wsRetryCount = 0;
        if (!hasConnectedOnce) term.clear();
        hasConnectedOnce = true;
        term.writeln('\x1b[32m[Connected to Server Console]\x1b[0m');
        if (reconnectTimer) { clearInterval(reconnectTimer); reconnectTimer = null; }
    };

    ws.onmessage = (ev) => {
        if (ev.data instanceof ArrayBuffer) {
            try { handleStats(JSON.parse(new TextDecoder().decode(ev.data))); } catch (_) {}
        } else if (ev.data && !document.hidden) {
            term.write(htmlToAnsi(ev.data));
        }
    };

    ws.onclose = () => {
        if (!reconnectTimer) {
            term.writeln('\x1b[33m[Disconnected — reconnecting…]\x1b[0m');
            reconnectTimer = setInterval(() => {
                _wsRetryCount++;
                // Try to reconnect directly; fall back to stats check after several failures
                if (_wsRetryCount <= 3) {
                    connectConsole();
                } else {
                    fetch(`/api/servers/${YU_SERVER_ID}/stats`)
                        .then(r => r.json())
                        .then(stats => { if (stats.state === 'running') connectConsole(); })
                        .catch(() => {});
                }
            }, 2000);
        }
    };

    ws.onerror = (e) => { console.error('WS Error', e); ws.close(); };
}

// Disable all direct keyboard input into xterm — use cmd-input field only
term.attachCustomKeyEventHandler(function () { return false; });

connectConsole();

// ── Disk space (one-shot fetch on page load) ──────────────────────────────────
fetch(`/api/servers/${YU_SERVER_ID}/disk`)
    .then(r => r.ok ? r.json() : null)
    .then(d => {
        if (!d) return;
        const volMB   = (d.volume_used / 1048576).toFixed(0);
        const totalGB = d.disk_total > 0 ? (d.disk_total / 1073741824).toFixed(1) : null;
        document.getElementById('disk-space-val').textContent =
            totalGB ? `${volMB} MB / ${totalGB} GB` : `${volMB} MB`;
        const fsEl = document.getElementById('disk-space-fs');
        if (fsEl) fsEl.textContent = totalGB ? 'volume used / disk total' : '';
    }).catch(() => {});

const _cmdInput = document.getElementById('cmd-input');
if (_cmdInput) {
    _cmdInput.addEventListener('keydown', function (e) {
        if (e.key === 'Enter') {
            e.preventDefault();
            const cmd = _cmdInput.value;
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(cmd + '\n');
            }
            _cmdInput.value = '';
        }
    });
}

// ── Controls ──────────────────────────────────────────────────────────────────
function updateControls(state) {
    const btnStart   = document.getElementById('btn-start');
    const btnRestart = document.getElementById('btn-restart');
    const btnStop    = document.getElementById('btn-stop');
    const btnKill    = document.getElementById('btn-kill');
    const badge      = document.getElementById('server-status-badge');

    document.getElementById('status-text').textContent = state;

    if (state === 'running') {
        btnStart.disabled = true; btnRestart.disabled = false;
        btnStop.disabled = false; btnKill.disabled = false;
        badge.className = 'sb-status sb-running';
    } else if (state === 'restarting') {
        btnStart.disabled = true; btnRestart.disabled = true;
        btnStop.disabled = false; btnKill.disabled = false;
        badge.className = 'sb-status sb-other';
    } else {
        btnStart.disabled = false; btnRestart.disabled = true;
        btnStop.disabled = true; btnKill.disabled = true;
        badge.className = 'sb-status sb-stopped';
    }
}

function sendAction(action) {
    fetch(`/api/servers/${YU_SERVER_ID}/${action}`, { method: 'POST' })
        .then(r => console.log(action, r.status))
        .catch(e => console.error(e));
    if (action === 'start') updateControls('container starting...');
}

function confirmKill() {
    new bootstrap.Modal(document.getElementById('killModal')).show();
}

// ── Metrics charts ────────────────────────────────────────────────────────────
const _cs = getComputedStyle(document.documentElement);
const _C_ACCENT = _cs.getPropertyValue('--accent').trim() || '#7c3aed';
const _C_OK     = _cs.getPropertyValue('--ok').trim()     || '#10b981';
const _C_WARN   = _cs.getPropertyValue('--warn').trim()   || '#f59e0b';
const _C_ERR    = _cs.getPropertyValue('--err').trim()    || '#ef4444';
const commonOptions = {
    responsive: true, maintainAspectRatio: false, animation: false,
    plugins: { legend: { display: false } },
    scales: {
        x: { display: false },
        y: { grid: { color: '#333' }, ticks: { color: '#888' }, beginAtZero: true }
    },
    elements: { point: { radius: 0 }, line: { tension: 0.3, borderWidth: 2 } }
};

const mkChart = (id, color, scaleOverrides = {}) => new Chart(
    document.getElementById(id),
    {
        type: 'line',
        data: {
            labels: Array(200).fill(''),
            datasets: [{ data: Array(200).fill(0), borderColor: color, backgroundColor: color + '33', fill: true }]
        },
        options: { ...commonOptions, scales: { ...commonOptions.scales, y: { ...commonOptions.scales.y, ...scaleOverrides } } }
    }
);

const cpuChart  = mkChart('cpuChart',  _C_ACCENT, { max: 100 });
const ramChart  = mkChart('ramChart',  _C_OK,     { max: 100 });
const netChart  = mkChart('netChart',  _C_WARN,   { beginAtZero: true, suggestedMax: 100 });
const diskChart = mkChart('diskChart', _C_ERR,    { beginAtZero: true, suggestedMax: 100 });

function updateChart(chart, value) {
    const data = chart.data.datasets[0].data;
    data.shift(); data.push(value);
    chart.update();
}

// ── Metrics via WebSocket (Binary frames pushed every 1 s from server) ─────────
let _prevRx = null;
let _prevTx = null;
let _prevBlkRead  = null;
let _prevBlkWrite = null;

function handleStats(stats) {
    updateControls(stats.state);

    // CPU
    updateChart(cpuChart, stats.cpu);
    document.getElementById('cpu-val').innerText = `${stats.cpu.toFixed(1)}%`;

    // RAM: percentage + absolute
    const ramMB   = (stats.ram / 1024 / 1024).toFixed(0);
    const limitMB = (stats.ram_limit / 1024 / 1024).toFixed(0);
    const ramPct  = stats.ram_limit > 0 ? ((stats.ram / stats.ram_limit) * 100) : 0;
    updateChart(ramChart, ramPct);
    document.getElementById('ram-val').innerText =
        stats.ram_limit > 0
            ? `${ramPct.toFixed(1)}% (${ramMB} / ${limitMB} MB)`
            : `${ramMB} MB`;

    // Network I/O delta (KB/s)
    let rxRate = 0, txRate = 0;
    if (_prevRx !== null && stats.rx >= _prevRx) rxRate = (stats.rx - _prevRx) / 1024;
    if (_prevTx !== null && stats.tx >= _prevTx) txRate = (stats.tx - _prevTx) / 1024;
    _prevRx = stats.rx;
    _prevTx = stats.tx;
    updateChart(netChart, rxRate + txRate);
    document.getElementById('net-val').innerText =
        `\u2193 ${rxRate.toFixed(1)}  \u2191 ${txRate.toFixed(1)} KB/s`;

    // Disk I/O delta (KB/s)
    let diskRd = 0, diskWr = 0;
    if (_prevBlkRead  !== null && stats.blk_read  >= _prevBlkRead)  diskRd = (stats.blk_read  - _prevBlkRead)  / 1024;
    if (_prevBlkWrite !== null && stats.blk_write >= _prevBlkWrite) diskWr = (stats.blk_write - _prevBlkWrite) / 1024;
    _prevBlkRead  = stats.blk_read;
    _prevBlkWrite = stats.blk_write;
    updateChart(diskChart, diskRd + diskWr);
    document.getElementById('disk-val').innerText = `\u2193 ${diskRd.toFixed(1)}  \u2191 ${diskWr.toFixed(1)} KB/s`;
}

// ── Cleanup (called by SPA navigation before leaving this page) ───────────────
window._yuPageCleanup = function () {
    if (reconnectTimer) { clearInterval(reconnectTimer); reconnectTimer = null; }
    ws?.close(); ws = null;
    try { cpuChart.destroy(); ramChart.destroy(); netChart.destroy(); diskChart.destroy(); } catch (_) {}
    window.removeEventListener('resize', _resizeHandler);
    window._yuPageCleanup = undefined;
};
