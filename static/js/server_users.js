const SU_PERMISSION_LABELS = {
    console: 'Console',
    files: 'Files',
    networking: 'Network',
    settings: 'Settings',
    audit: 'Audit',
    power: 'Power',
    members: 'Members',
};

let _suAvailableUsers = [];
let _suMembers = [];
let _suPermissions = [];
let _suSelectedUserId = 0;
let _suCanWrite = !!window.YU_CAN_MEMBERS_WRITE;
const _suUidCopyBadgeTimers = new WeakMap();

function suUserNickname(user) {
    const nickname = String(user?.nickname || '').trim();
    if (nickname) return nickname;
    return String(user?.username || '').trim();
}

function suUserUid(user) {
    return String(user?.uid || '').trim();
}

function suUserDisplayName(user) {
    const nickname = suUserNickname(user);
    const uid = suUserUid(user);
    if (!nickname) return uid || 'unknown';
    return uid ? `${nickname} ${uid}` : nickname;
}

function suEscHtml(value) {
    return String(value ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function suNotify(msg, isError) {
    const box = document.getElementById('su-feedback');
    if (!box) return;

    if (!msg) {
        box.hidden = true;
        box.textContent = '';
        box.classList.remove('error');
        return;
    }

    box.hidden = false;
    box.textContent = msg;
    box.classList.toggle('error', !!isError);
}

function suApplyWriteAccessMode() {
    const searchInput = document.getElementById('su-member-search');
    const addBtn = document.getElementById('su-add-member-btn');
    const note = document.getElementById('su-readonly-note');

    if (searchInput) searchInput.disabled = !_suCanWrite;
    if (addBtn) addBtn.disabled = !_suCanWrite;
    if (note) note.hidden = _suCanWrite;
}

function suShowUidCopiedBadge(el) {
    if (!el) return;

    const prevTimer = _suUidCopyBadgeTimers.get(el);
    if (prevTimer) {
        clearTimeout(prevTimer);
    }

    el.classList.add('copied');
    const timer = setTimeout(() => {
        el.classList.remove('copied');
        _suUidCopyBadgeTimers.delete(el);
    }, 1100);
    _suUidCopyBadgeTimers.set(el, timer);
}

async function suCopyUid(uid, sourceEl) {
    const value = String(uid || '').trim();
    if (!value) return;

    try {
        if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
            await navigator.clipboard.writeText(value);
        } else {
            const ta = document.createElement('textarea');
            ta.value = value;
            ta.setAttribute('readonly', 'readonly');
            ta.style.position = 'fixed';
            ta.style.opacity = '0';
            document.body.appendChild(ta);
            ta.select();
            document.execCommand('copy');
            document.body.removeChild(ta);
        }
        suShowUidCopiedBadge(sourceEl);
    } catch (e) {
        suNotify('Failed to copy UID.', true);
    }
}

function suUpdateCount(count) {
    const countEl = document.getElementById('su-member-count');
    if (!countEl) return;
    const n = Number(count || 0);
    countEl.textContent = `${n} member${n === 1 ? '' : 's'}`;
}

function suResolveUserByUid(inputValue) {
    const q = String(inputValue || '').trim();
    if (!q) {
        return { error: 'Enter a UID first.' };
    }

    if (!_suAvailableUsers.length) {
        return { error: 'No users available for adding.' };
    }

    const exact = _suAvailableUsers.find((u) => suUserUid(u) === q);
    if (exact) return { user: exact };

    return { error: 'User with this UID was not found.' };
}

function suMemberAccessSummary(member) {
    const perms = member?.permissions || {};
    let write = 0;
    let read = 0;
    for (const permission of _suPermissions) {
        const mode = String(perms[permission] || 'none');
        if (mode === 'write') write += 1;
        if (mode === 'read') read += 1;
    }
    if (write > 0) return `${write} write`;
    if (read > 0) return `${read} read`;
    return 'no access';
}

function suBindMemberRowEvents() {
    const editorGrid = document.getElementById('su-editor-grid');
    if (!editorGrid) return;

    editorGrid.querySelectorAll('.su-perm-select').forEach((sel) => {
        sel.addEventListener('change', () => {
            const userId = Number(sel.dataset.userId || 0);
            const permission = String(sel.dataset.permission || '');
            suSetMemberPermission(userId, permission, sel.value);
        });
    });
}

function suRenderMemberNav() {
    const list = document.getElementById('su-member-list');
    if (!list) return;

    suUpdateCount(_suMembers.length);

    if (!_suMembers.length) {
        list.innerHTML = '<div class="su-member-empty">No members yet.</div>';
        return;
    }

    list.innerHTML = _suMembers.map((member) => {
        const userId = Number(member.user_id || 0);
        const isOwner = !!member.is_owner;
        const isActive = userId === _suSelectedUserId;
        const nickname = suUserNickname(member) || 'unknown';
        const uid = suUserUid(member);
        const summary = suMemberAccessSummary(member);

        return `<button type="button" class="su-nav-item ${isActive ? 'active' : ''}" data-user-id="${userId}">
            <span class="su-nav-main">
                <span class="su-nav-dot ${isOwner ? 'owner' : ''}"></span>
                <span class="su-nav-copy">
                    <span class="su-nav-name">${suEscHtml(nickname)}</span>
                    ${uid ? `<span class="su-nav-uid su-copy-uid" data-uid="${suEscHtml(uid)}" title="Click to copy UID">${suEscHtml(uid)}</span>` : ''}
                    <span class="su-nav-meta">${isOwner ? 'owner - full access' : suEscHtml(summary)}</span>
                </span>
            </span>
            ${isOwner ? '<span class="su-nav-pill">owner</span>' : ''}
        </button>`;
    }).join('');

    list.querySelectorAll('.su-copy-uid').forEach((uidEl) => {
        uidEl.addEventListener('click', (ev) => {
            ev.preventDefault();
            ev.stopPropagation();
            suCopyUid(uidEl.dataset.uid, uidEl);
        });
    });

    list.querySelectorAll('.su-nav-item').forEach((btn) => {
        btn.addEventListener('click', () => {
            const userId = Number(btn.dataset.userId || 0);
            suSelectMember(userId);
        });
    });
}

function suRenderEditor() {
    const nameEl = document.getElementById('su-editor-name');
    const uidEl = document.getElementById('su-editor-uid');
    const subEl = document.getElementById('su-editor-subtitle');
    const emptyEl = document.getElementById('su-editor-empty');
    const gridEl = document.getElementById('su-editor-grid');
    const removeBtn = document.getElementById('su-remove-selected');
    if (!nameEl || !uidEl || !subEl || !emptyEl || !gridEl || !removeBtn) return;

    const member = _suMembers.find((m) => Number(m.user_id || 0) === _suSelectedUserId);
    if (!member) {
        nameEl.textContent = 'Select a member';
        uidEl.hidden = true;
        uidEl.textContent = '';
        uidEl.classList.remove('copied');
        delete uidEl.dataset.uid;
        subEl.textContent = 'Pick a user from the directory to edit permissions.';
        emptyEl.hidden = false;
        gridEl.hidden = true;
        gridEl.innerHTML = '';
        removeBtn.hidden = true;
        return;
    }

    const userId = Number(member.user_id || 0);
    const isOwner = !!member.is_owner;
    const nickname = suUserNickname(member) || 'unknown';
    const uid = suUserUid(member);
    const displayName = suUserDisplayName(member);
    const perms = member.permissions || {};
    const selectDisabled = isOwner || !_suCanWrite;

    nameEl.textContent = nickname;
    if (uid) {
        uidEl.hidden = false;
        uidEl.textContent = uid;
        uidEl.dataset.uid = uid;
    } else {
        uidEl.hidden = true;
        uidEl.textContent = '';
        uidEl.classList.remove('copied');
        delete uidEl.dataset.uid;
    }
    subEl.textContent = isOwner
        ? 'Owner has immutable full access.'
        : (_suCanWrite
            ? 'Edit per-category access policies for this member.'
            : 'Read-only mode: view permissions only.');

    emptyEl.hidden = true;
    gridEl.hidden = false;

    gridEl.innerHTML = _suPermissions.map((permission) => {
        const mode = String(perms[permission] || 'none');
        return `<div class="su-perm-row">
            <label>${suEscHtml(SU_PERMISSION_LABELS[permission] || permission)}</label>
            <select class="su-select su-perm-select" data-user-id="${userId}" data-permission="${suEscHtml(permission)}" ${selectDisabled ? 'disabled' : ''}>
                <option value="none" ${mode === 'none' ? 'selected' : ''}>none</option>
                <option value="read" ${mode === 'read' ? 'selected' : ''}>read</option>
                <option value="write" ${mode === 'write' ? 'selected' : ''}>write</option>
            </select>
        </div>`;
    }).join('');

    removeBtn.hidden = isOwner || !_suCanWrite;
    if (!isOwner && _suCanWrite) {
        removeBtn.dataset.userId = String(userId);
        removeBtn.dataset.username = displayName;
    }

    suBindMemberRowEvents();
}

function suSelectMember(userId) {
    _suSelectedUserId = Number(userId || 0);
    suRenderMemberNav();
    suRenderEditor();
}

function suRemoveSelected() {
    const removeBtn = document.getElementById('su-remove-selected');
    if (!removeBtn) return;
    const userId = Number(removeBtn.dataset.userId || 0);
    const username = String(removeBtn.dataset.username || 'this user');
    if (!userId) return;
    suRemoveMember(userId, username);
}

function suUpdateSearchSource(users) {
    const datalist = document.getElementById('su-member-user-list');
    if (!datalist) return;

    datalist.innerHTML = users.map((u) => {
        const uid = suEscHtml(suUserUid(u));
        const display = suEscHtml(suUserDisplayName(u));
        return `<option value="${uid}" label="${display}"></option>`;
    }).join('');
}

function suRenderMembersFromPayload(data, preferredUserId) {
    _suPermissions = Array.isArray(data.permissions) ? data.permissions : [];
    _suMembers = Array.isArray(data.members) ? data.members : [];

    if (!_suMembers.length) {
        _suSelectedUserId = 0;
        suRenderMemberNav();
        suRenderEditor();
        return;
    }

    const preferred = Number(preferredUserId || 0);
    const hasPreferred = preferred > 0 && _suMembers.some((m) => Number(m.user_id || 0) === preferred);
    const hasCurrent = _suSelectedUserId > 0 && _suMembers.some((m) => Number(m.user_id || 0) === _suSelectedUserId);

    if (hasPreferred) {
        _suSelectedUserId = preferred;
    } else if (!hasCurrent) {
        _suSelectedUserId = Number(_suMembers[0].user_id || 0);
    }

    suRenderMemberNav();
    suRenderEditor();
}

async function suLoadMembers(preferredUserId) {
    const list = document.getElementById('su-member-list');
    const emptyEl = document.getElementById('su-editor-empty');
    if (!list) return;

    list.innerHTML = '<div class="su-member-empty">Loading...</div>';
    if (emptyEl) {
        emptyEl.hidden = false;
        emptyEl.textContent = 'Loading members...';
    }
    suNotify('');

    try {
        const r = await fetch(`/api/servers/${YU_SERVER_ID}/members`, { credentials: 'same-origin' });
        const data = await r.json().catch(() => ({}));

        if (!r.ok || !data.ok) {
            list.innerHTML = '<div class="su-member-empty">Failed to load members.</div>';
            suNotify(data.error || 'Failed to load members.', true);
            return;
        }

        _suCanWrite = !!data.can_write;
        suApplyWriteAccessMode();

        _suAvailableUsers = Array.isArray(data.users) ? data.users : [];
        suUpdateSearchSource(_suAvailableUsers);
        suRenderMembersFromPayload(data, preferredUserId);
    } catch (e) {
        console.error('suLoadMembers failed', e);
        list.innerHTML = '<div class="su-member-empty">Network error.</div>';
        suNotify('Network error while loading members.', true);
    }
}

async function suAddMember() {
    if (!_suCanWrite) {
        suNotify('Read-only access. You cannot add members.', true);
        return;
    }

    const input = document.getElementById('su-member-search');
    if (!input) return;

    const resolved = suResolveUserByUid(input.value);
    if (resolved.error) {
        suNotify(resolved.error, true);
        return;
    }

    const user = resolved.user;
    const uid = suUserUid(user);
    if (!uid) {
        suNotify('UID is missing for this user.', true);
        return;
    }
    const userId = Number(user.id || 0);
    if (!userId) {
        suNotify('User not found.', true);
        return;
    }

    try {
        const r = await fetch(`/api/servers/${YU_SERVER_ID}/members/add`, {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ uid }),
        });
        const data = await r.json().catch(() => ({}));

        if (!r.ok || !data.ok) {
            suNotify(data.error || 'Failed to add member.', true);
            return;
        }

        input.value = '';
        await suLoadMembers(userId);
        suNotify(`Added ${suUserDisplayName(user)}.`, false);
    } catch (e) {
        console.error('suAddMember failed', e);
        suNotify('Network error while adding member.', true);
    }
}

async function suSetMemberPermission(userId, permission, mode) {
    if (!_suCanWrite) {
        suNotify('Read-only access. You cannot change permissions.', true);
        return;
    }

    if (!userId || !permission) return;

    try {
        const r = await fetch(`/api/servers/${YU_SERVER_ID}/members/${encodeURIComponent(String(userId))}/permissions`, {
            method: 'POST',
            credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ permission, mode }),
        });
        const data = await r.json().catch(() => ({}));

        if (!r.ok || !data.ok) {
            suNotify(data.error || 'Failed to update permission.', true);
            await suLoadMembers(userId);
            return;
        }

        const member = _suMembers.find((m) => Number(m.user_id || 0) === Number(userId));
        if (member && member.permissions) {
            member.permissions[permission] = mode;
            suRenderMemberNav();
            suRenderEditor();
        }

        suNotify(`Updated ${SU_PERMISSION_LABELS[permission] || permission} permission.`, false);
    } catch (e) {
        console.error('suSetMemberPermission failed', e);
        suNotify('Network error while updating permission.', true);
        await suLoadMembers(userId);
    }
}

async function suRemoveMember(userId, username) {
    if (!_suCanWrite) {
        suNotify('Read-only access. You cannot remove members.', true);
        return;
    }

    if (!userId) return;
    if (!window.confirm(`Remove ${username} from this container?`)) return;

    try {
        const r = await fetch(`/api/servers/${YU_SERVER_ID}/members/${encodeURIComponent(String(userId))}/remove`, {
            method: 'POST',
            credentials: 'same-origin',
        });
        const data = await r.json().catch(() => ({}));

        if (!r.ok || !data.ok) {
            suNotify(data.error || 'Failed to remove member.', true);
            return;
        }

        if (_suSelectedUserId === Number(userId)) {
            _suSelectedUserId = 0;
        }

        await suLoadMembers();
        suNotify(`Removed ${username}.`, false);
    } catch (e) {
        console.error('suRemoveMember failed', e);
        suNotify('Network error while removing member.', true);
    }
}

function suInit() {
    suApplyWriteAccessMode();

    const input = document.getElementById('su-member-search');
    if (input && !input.dataset.bound) {
        input.dataset.bound = '1';
        input.addEventListener('keydown', (ev) => {
            if (ev.key !== 'Enter') return;
            ev.preventDefault();
            suAddMember();
        });
    }

    const editorUid = document.getElementById('su-editor-uid');
    if (editorUid && !editorUid.dataset.bound) {
        editorUid.dataset.bound = '1';
        editorUid.addEventListener('click', (ev) => {
            ev.preventDefault();
            suCopyUid(editorUid.dataset.uid, editorUid);
        });
    }

    suLoadMembers();
}

window.suAddMember = suAddMember;
window.suSetMemberPermission = suSetMemberPermission;
window.suRemoveMember = suRemoveMember;
window.suRemoveSelected = suRemoveSelected;

suInit();

window.addEventListener('yu:page-shown', (ev) => {
    const path = String(ev?.detail?.path || '');
    if (/^\/servers\/\d+\/users$/.test(path)) {
        suLoadMembers();
    }
});
