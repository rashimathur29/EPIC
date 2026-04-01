// epic.js — EPIC Desktop Agent
// Fix: All DOM queries are guarded. This file loads on index.html only;
//       if elements are missing, we bail gracefully (no TypeError).

'use strict';

/* ────────────────────────────────────────────────────────────────
   TAURI BRIDGE
   Works in both Tauri (window.__TAURI__) and plain browser dev.
──────────────────────────────────────────────────────────────── */
const { invoke } = window.__TAURI__?.core
    ?? { invoke: async (cmd, args) => { console.warn('[DEV] invoke:', cmd, args); return null; } };

/* ────────────────────────────────────────────────────────────────
   SAFE DOM HELPERS
──────────────────────────────────────────────────────────────── */
const $ = id => document.getElementById(id);
function on(id, evt, fn) {
    const el = $(id);
    if (el) el.addEventListener(evt, fn);
}

/* ────────────────────────────────────────────────────────────────
   GUARD — only run main logic when index.html DOM is present
──────────────────────────────────────────────────────────────── */
function isIndexPage() {
    return !!$('stopwatchDisplay');
}

/* ────────────────────────────────────────────────────────────────
   ACTIVE STOPWATCH (synced to Rust)
──────────────────────────────────────────────────────────────── */
let activeStopwatchInterval = null;
let activeSeconds = 0;

function formatActiveTime(seconds) {
    const h = Math.floor(seconds / 3600).toString().padStart(2, '0');
    const m = Math.floor((seconds % 3600) / 60).toString().padStart(2, '0');
    const s = (seconds % 60).toString().padStart(2, '0');
    return `${h}:${m}:${s}`;
}

async function updateActiveStopwatch() {
    try {
        activeSeconds = await invoke('get_total_active_seconds') ?? 0;
    } catch {
        /* keep last value */
    }
    const el = $('stopwatchDisplay');
    if (el) el.textContent = formatActiveTime(activeSeconds);
}

function startActiveStopwatch() {
    updateActiveStopwatch();
    activeStopwatchInterval = setInterval(updateActiveStopwatch, 1000);
}

function stopActiveStopwatch() {
    if (activeStopwatchInterval) {
        clearInterval(activeStopwatchInterval);
        activeStopwatchInterval = null;
    }
    const el = $('stopwatchDisplay');
    if (el) el.textContent = '00:00:00';
}

/* ────────────────────────────────────────────────────────────────
   BANNER
──────────────────────────────────────────────────────────────── */
function showBanner(message, type = 'info', duration = 8000) {
    const existing = $('appBanner');
    if (existing) existing.remove();

    const banner = document.createElement('div');
    banner.id = 'appBanner';
    banner.textContent = message;

    const colors = { success: '#107c10', warning: '#ffa500', error: '#c4314b', info: '#0078d4' };
    Object.assign(banner.style, {
        position: 'fixed', top: '58px', left: '50%',
        transform: 'translateX(-50%)',
        padding: '10px 24px', borderRadius: '4px',
        color: '#fff', fontWeight: '600', zIndex: '9999',
        boxShadow: '0 4px 16px rgba(0,0,0,.25)',
        transition: 'opacity .4s',
        fontSize: '13px', fontFamily: 'var(--font, "Segoe UI", sans-serif)',
        background: colors[type] ?? colors.info,
        minWidth: '260px', textAlign: 'center',
        whiteSpace: 'nowrap',
    });
    if (type === 'warning') banner.style.color = '#201f1e';

    document.body.appendChild(banner);

    if (duration > 0) {
        setTimeout(() => {
            banner.style.opacity = '0';
            setTimeout(() => banner.remove(), 400);
        }, duration);
    }
}

/* ────────────────────────────────────────────────────────────────
   NAVIGATION (mirrors app.js — epic.js owns the .active class)
──────────────────────────────────────────────────────────────── */
const PAGE_META = {
    productivity: { title: 'Productivity Sessions',  icon: 'fas fa-clock'         },
    breaks:       { title: 'Break Timers',           icon: 'fas fa-hourglass-half' },
    calls:        { title: 'Calls',                  icon: 'fas fa-phone'          },
    system:       { title: 'System Information',     icon: 'fas fa-chart-bar'      },
};

function goToPage(page) {
    document.querySelectorAll('.sidebar__nav-item').forEach(btn => {
        btn.classList.toggle('is-active', btn.dataset.page === page);
    });
    document.querySelectorAll('.page').forEach(panel => {
        panel.classList.toggle('is-active', panel.id === `page-${page}`);
    });
    const meta = PAGE_META[page];
    if (!meta) return;
    const title = $('page-header-title');
    const icon  = $('page-header-icon');
    if (title) title.textContent = meta.title;
    if (icon)  icon.className = `${meta.icon} page-header__icon`;
}

/* ────────────────────────────────────────────────────────────────
   BREAK TIMERS — local in-memory timers
──────────────────────────────────────────────────────────────── */
let timers = [];
let timerCounter = 1;
const timerTicks = {};

function fmtTimer(s) {
    return [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60]
        .map(v => String(v).padStart(2, '0')).join(':');
}

function renderTimers() {
    const emptyEl  = $('breaks-empty');
    const listEl   = $('timer-list');
    const editBtn  = $('btn-edit-timers');
    const isEmpty  = timers.length === 0;

    if (emptyEl) emptyEl.style.display = isEmpty ? 'flex' : 'none';
    if (listEl)  listEl.style.display  = isEmpty ? 'none' : 'flex';
    if (editBtn) editBtn.style.display  = isEmpty ? 'none' : 'flex';
    if (isEmpty || !listEl) return;

    listEl.innerHTML = timers.map(t => `
        <li class="timer-card" data-id="${t.id}">
            <div>
                <p class="timer-card__name">${escHtml(t.name)}</p>
                <p class="timer-card__time">${fmtTimer(t.remainingSeconds)}</p>
            </div>
            <div class="timer-card__controls">
                <button class="btn-icon btn-icon--play"
                        data-action="toggle" data-id="${t.id}"
                        aria-label="${t.running ? 'Pause' : 'Start'} timer">
                    <i class="fas fa-${t.running ? 'pause' : 'play'}" aria-hidden="true"></i>
                </button>
                <button class="btn-icon"
                        data-action="reset" data-id="${t.id}"
                        aria-label="Reset timer">
                    <i class="fas fa-redo" aria-hidden="true"></i>
                </button>
                <button class="btn-icon btn-icon--delete"
                        data-action="delete" data-id="${t.id}"
                        aria-label="Delete timer">
                    <i class="fas fa-trash" aria-hidden="true"></i>
                </button>
            </div>
        </li>`).join('');
}

function escHtml(str) {
    return str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

function toggleTimer(id) {
    const t = timers.find(x => x.id === id);
    if (!t) return;
    t.running = !t.running;
    if (t.running) {
        timerTicks[id] = setInterval(() => {
            t.remainingSeconds = Math.max(0, t.remainingSeconds - 1);
            if (t.remainingSeconds === 0) {
                clearInterval(timerTicks[id]);
                delete timerTicks[id];
                t.running = false;
                showBanner(`⏰ Timer "${t.name}" finished!`, 'info', 5000);
            }
            renderTimers();
        }, 1000);
    } else {
        clearInterval(timerTicks[id]);
        delete timerTicks[id];
    }
    renderTimers();
}

function resetTimer(id) {
    const t = timers.find(x => x.id === id);
    if (!t) return;
    clearInterval(timerTicks[id]);
    delete timerTicks[id];
    t.running = false;
    t.remainingSeconds = t.totalSeconds;
    renderTimers();
}

function deleteTimer(id) {
    clearInterval(timerTicks[id]);
    delete timerTicks[id];
    timers = timers.filter(x => x.id !== id);
    renderTimers();
}

/* ────────────────────────────────────────────────────────────────
   TIMER MODAL
──────────────────────────────────────────────────────────────── */
function openTimerModal() {
    ['modal-hours','modal-minutes','modal-seconds'].forEach(id => {
        const el = $(id); if (el) el.value = '00';
    });
    const nameEl = $('modal-timer-name');
    if (nameEl) nameEl.value = '';
    const modal = $('timer-modal');
    if (modal) modal.classList.add('is-open');
    if (nameEl) nameEl.focus();
}

function closeTimerModal() {
    const modal = $('timer-modal');
    if (modal) modal.classList.remove('is-open');
}

function saveTimer() {
    const get = id => parseInt($(id)?.value ?? '0', 10) || 0;
    const hours   = get('modal-hours');
    const minutes = get('modal-minutes');
    const seconds = get('modal-seconds');
    const name    = $('modal-timer-name')?.value?.trim() || `Timer ${timerCounter}`;
    const total   = hours * 3600 + minutes * 60 + seconds;
    if (total <= 0) return;
    timers.push({ id: Date.now(), name, totalSeconds: total, remainingSeconds: total, running: false });
    timerCounter++;
    renderTimers();
    closeTimerModal();
}

function stepTimerField(fieldId, dir) {
    const el = $(fieldId);
    if (!el) return;
    const max = fieldId === 'modal-hours' ? 23 : 59;
    el.value = String(((parseInt(el.value) || 0) + dir + max + 1) % (max + 1)).padStart(2, '0');
}

// Expose for inline onclick (if any)
window.incrementTime = stepTimerField;

/* ────────────────────────────────────────────────────────────────
   CALL CONTROLS
──────────────────────────────────────────────────────────────── */
function initCallControls() {
    function toggleMute(btnId, iconOn, iconOff) {
        const btn  = $(btnId);
        if (!btn) return;
        const icon = btn.querySelector('i');
        const muted = btn.classList.toggle('is-muted');
        if (icon) icon.className = `fas ${muted ? iconOff : iconOn}`;
        btn.setAttribute('aria-pressed', String(muted));
    }

    on('btn-call-video', 'click', () => toggleMute('btn-call-video', 'fa-video', 'fa-video-slash'));
    on('btn-call-audio', 'click', () => toggleMute('btn-call-audio', 'fa-microphone', 'fa-microphone-slash'));
    on('btn-call-end',   'click', () => {
        if (!confirm('End the call?')) return;
        ['btn-call-video','btn-call-audio'].forEach(id => {
            const btn  = $(id);
            const icon = btn?.querySelector('i');
            btn?.classList.remove('is-muted');
            btn?.setAttribute('aria-pressed', 'false');
            if (icon) icon.className = `fas ${id === 'btn-call-video' ? 'fa-video' : 'fa-microphone'}`;
        });
    });
}

/* ────────────────────────────────────────────────────────────────
   SYSTEM STATS — simulated until Rust pushes real data
──────────────────────────────────────────────────────────────── */
let _simInterval = null;

function setText(id, val) {
    const el = $(id); if (el) el.textContent = val;
}

function startStatsSim() {
    function sim() {
        setText('stat-cpu',    `${(Math.random() * 60 + 20).toFixed(1)}%`);
        setText('stat-memory', `${(Math.random() * 40 + 40).toFixed(1)}%`);
    }
    sim();
    _simInterval = setInterval(sim, 3000);
}

// Called by EpicCommands when Rust sends real metrics
function updateSystemStats(data = {}) {
    if (data.cpu)         setText('stat-cpu',          data.cpu);
    if (data.memory)      setText('stat-memory',        data.memory);
    if (data.connType)    setText('stat-conn-type',     data.connType);
    if (data.location)    setText('stat-location',      data.location);
    if (data.breakTime)   setText('stat-break-time',    data.breakTime);
    if (data.totalBreaks !== undefined) setText('stat-total-breaks', data.totalBreaks);
    if (data.avgBreak)    setText('stat-avg-break',     data.avgBreak);
    if (_simInterval) { clearInterval(_simInterval); _simInterval = null; }
}

/* ────────────────────────────────────────────────────────────────
   COLLAPSIBLE SECTIONS
──────────────────────────────────────────────────────────────── */
function initCollapsibles() {
    document.querySelectorAll('.sys-section__toggle').forEach(btn => {
        btn.addEventListener('click', () => {
            const section = btn.closest('.sys-section');
            if (!section) return;
            const open = section.classList.toggle('is-open');
            btn.setAttribute('aria-expanded', String(open));
        });
    });
}

/* ────────────────────────────────────────────────────────────────
   TAURI COMMANDS — EpicCommands exposed to app.js / popover.js
──────────────────────────────────────────────────────────────── */
window.EpicCommands = {
    async sendAnalytics() {
        try {
            await invoke('send_analytics');
            showBanner('Analytics sent ✓', 'success');
        } catch (e) {
            showBanner(`Analytics failed: ${e}`, 'error');
        }
    },
    async refreshData() {
        showBanner('Refreshing data…', 'info', 3000);
        try {
            await invoke('refresh_data');
            showBanner('Data refreshed ✓', 'success');
        } catch (e) {
            showBanner(`Refresh failed: ${e}`, 'error');
        }
    },
    async sendLogs() {
        try {
            await invoke('send_logs');
            showBanner('Logs sent ✓', 'success');
        } catch (e) {
            showBanner(`Log send failed: ${e}`, 'error');
        }
    },
    async signoutOrg() {
        try {
            await invoke('logout');
            // Also clear org config
            sessionStorage.clear();
            window.location.replace('find_your_organization.html');
        } catch (e) {
            showBanner(`Sign-out failed: ${e}`, 'error');
        }
    },
    async signout() {
        try {
            await invoke('logout');
            sessionStorage.removeItem('userEmail');
            window.location.replace('login.html');
        } catch (e) {
            showBanner(`Sign-out failed: ${e}`, 'error');
        }
    },
};

/* ────────────────────────────────────────────────────────────────
   CHECKIN / CHECKOUT
──────────────────────────────────────────────────────────────── */
async function handleCheckin() {
    const startBtn = $('startStopwatch');
    const endBtn   = $('endStopwatch');
    if (!startBtn) return;

    try {
        const isRunning = await invoke('get_status') ?? false;
        if (isRunning) { showBanner('Already checked in! Please check out first.', 'warning'); return; }

        startBtn.disabled = true;
        startBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i>';

        await invoke('checkin');
        showBanner('✅ Checked in — tracking active', 'success', 5000);

        if (endBtn) endBtn.disabled = false;
        startActiveStopwatch();

    } catch (err) {
        console.error('[CHECKIN]', err);
        startBtn.disabled = false;
        startBtn.innerHTML = '<i class="fas fa-play"></i>';

        const msg = String(err);
        if (msg.includes('Already checked in')) {
            showBanner('Already checked in! Please check out first.', 'warning');
        } else if (msg.includes('Failed to install') || msg.includes('system input hooks')) {
            showBanner('⚠️ Could not start input tracking. Close conflicting apps and retry.', 'error', 0);
        } else {
            showBanner(`Check-in failed: ${err}`, 'error');
        }
    }
}

async function handleCheckout() {
    const startBtn = $('startStopwatch');
    const endBtn   = $('endStopwatch');
    if (!endBtn) return;

    try {
        const isRunning = await invoke('get_status') ?? false;
        if (!isRunning) { showBanner('Not checked in.', 'warning'); return; }

        await invoke('checkout');
        showBanner('✅ Checked out — session saved', 'success', 5000);

        stopActiveStopwatch();
        if (startBtn) {
            startBtn.disabled = false;
            startBtn.innerHTML = '<i class="fas fa-play"></i>';
        }
        if (endBtn) endBtn.disabled = true;

    } catch (err) {
        console.error('[CHECKOUT]', err);
        showBanner(`Check-out failed: ${err}`, 'error');
    }
}

/* ────────────────────────────────────────────────────────────────
   STARTUP STATUS
──────────────────────────────────────────────────────────────── */
async function checkStartupStatus() {
    try {
        const status = await invoke('get_startup_status');
        if (!status) return;

        const startBtn = $('startStopwatch');
        const endBtn   = $('endStopwatch');

        if (status.has_active_session) {
            if (startBtn) startBtn.disabled = true;
            if (endBtn)   endBtn.disabled   = false;

            if (status.offline_minutes > 5) {
                showBanner(`⚠️ ${status.offline_minutes} min offline — marked as break`, 'warning');
            } else {
                showBanner('✅ Session resumed', 'success', 4000);
            }

            try {
                await invoke('resume_tracking');
            } catch (e) {
                showBanner('⚠️ Could not resume tracking hooks', 'error', 0);
            }

            startActiveStopwatch();
        } else {
            if (startBtn) startBtn.disabled = false;
            if (endBtn)   endBtn.disabled   = true;
            stopActiveStopwatch();
        }
    } catch (err) {
        console.warn('[STARTUP_STATUS]', err);
    }
}

/* ────────────────────────────────────────────────────────────────
   USER INFO — populate avatar + popover from session / Rust
──────────────────────────────────────────────────────────────── */
async function loadUserInfo() {
    try {
        // Try from session storage first (fast)
        const first = sessionStorage.getItem('firstName') || '';
        const last  = sessionStorage.getItem('lastName')  || '';
        const email = sessionStorage.getItem('userEmail') || '';
        const org   = sessionStorage.getItem('orgName')   || '';

        const initials = ((first[0] || '') + (last[0] || '')).toUpperCase() || 'U';

        // Update titlebar avatar
        const avatarEl = $('titlebar-avatar-initials');
        if (avatarEl) avatarEl.textContent = initials;

        // Update popover
        const popoverAvatar = $('popover-avatar-initials');
        if (popoverAvatar) popoverAvatar.textContent = initials;
        const popoverName = $('popover-user-name');
        if (popoverName) popoverName.textContent = `${first} ${last}`.trim() || 'User';
        const popoverRole = $('popover-user-role');
        if (popoverRole) popoverRole.textContent = org || 'Employee';
        const popoverEmail = $('popover-user-email');
        if (popoverEmail) popoverEmail.textContent = email;

        // Update version badges
        const ver = '1.0.0';
        ['popover-app-version','about-app-version'].forEach(id => {
            const el = $(id); if (el) el.textContent = ver;
        });

    } catch (e) {
        console.warn('[USER_INFO]', e);
    }
}

/* ────────────────────────────────────────────────────────────────
   ACTIVITY LOG  — append a row to the system log table
──────────────────────────────────────────────────────────────── */
function logActivity(message) {
    const tbody = $('activity-logs-body');
    if (!tbody) return;

    // Remove empty-state row
    const emptyRow = tbody.querySelector('.log-table__empty');
    if (emptyRow) emptyRow.closest('tr')?.remove();

    const now = new Date().toLocaleTimeString();
    const tr = document.createElement('tr');
    tr.innerHTML = `<td>${now}</td><td>${escHtml(message)}</td>`;
    tbody.prepend(tr);

    // Keep last 50 rows
    while (tbody.rows.length > 50) tbody.deleteRow(tbody.rows.length - 1);
}

/* ────────────────────────────────────────────────────────────────
   MAIN INIT
──────────────────────────────────────────────────────────────── */
document.addEventListener('DOMContentLoaded', async () => {
    if (!isIndexPage()) return; // ← THE KEY FIX: only run on index.html

    // Navigation
    document.querySelectorAll('.sidebar__nav-item[data-page]').forEach(btn => {
        btn.addEventListener('click', () => goToPage(btn.dataset.page));
    });

    // Check-in / out
    on('startStopwatch', 'click', handleCheckin);
    on('endStopwatch',   'click', handleCheckout);

    // Break timers — event delegation
    const timerList = $('timer-list');
    if (timerList) {
        timerList.addEventListener('click', e => {
            const btn = e.target.closest('[data-action][data-id]');
            if (!btn) return;
            const id = Number(btn.dataset.id);
            if (btn.dataset.action === 'toggle') toggleTimer(id);
            if (btn.dataset.action === 'reset')  resetTimer(id);
            if (btn.dataset.action === 'delete') deleteTimer(id);
        });
    }

    // Timer modal
    on('btn-add-timer',    'click', openTimerModal);
    on('btn-modal-cancel', 'click', closeTimerModal);
    on('btn-modal-save',   'click', saveTimer);

    const modal = $('timer-modal');
    if (modal) {
        modal.addEventListener('click', e => { if (e.target === modal) closeTimerModal(); });
        // Step buttons via data-field / data-step
        modal.addEventListener('click', e => {
            const btn = e.target.closest('[data-field][data-step]');
            if (btn) stepTimerField(btn.dataset.field, Number(btn.dataset.step));
        });
    }

    on('modal-timer-name', 'keydown', e => { if (e.key === 'Enter') saveTimer(); });

    document.addEventListener('keydown', e => {
        if (e.key === 'Escape' && modal?.classList.contains('is-open')) closeTimerModal();
    });

    // Render initial (empty) timer list
    renderTimers();

    // Call controls
    initCallControls();

    // System stats simulation
    startStatsSim();

    // Collapsibles
    initCollapsibles();

    // User info
    await loadUserInfo();

    // Check startup / resume session
    await checkStartupStatus();

    logActivity('EPIC agent started');
});