'use strict';
/*
  epic.js — single source of truth for all UI logic
*/

/* ── Tauri invoke (falls back to console.log in browser) ── */
const invoke = window.__TAURI__?.core?.invoke
    ?? (async (cmd, args) => { console.log('[DEV invoke]', cmd, args); return null; });

/* ── DOM helpers ── */
const $       = id  => document.getElementById(id);
const $$      = sel => document.querySelectorAll(sel);
const setText = (id, v) => { const el = $(id); if (el) el.textContent = v; };

/* ══════════════════════════════════════════════════════════════════
   COMPONENT LOADER
   Returns a Promise — critical so DOMContentLoaded can await them
══════════════════════════════════════════════════════════════════ */
function loadComponent(selector, path) {
    return fetch(path)
        .then(r => {
            if (!r.ok) throw new Error(`HTTP ${r.status} for ${path}`);
            return r.text();
        })
        .then(html => { document.querySelector(selector).innerHTML = html; })
        .catch(err => console.warn('[loadComponent]', path, err));
}

/* ══════════════════════════════════════════════════════════════════
   BANNER  (toast notification)
══════════════════════════════════════════════════════════════════ */
function showBanner(msg, type = 'info', dur = 5000) {
    const old = $('_epic_banner');
    if (old) old.remove();
    const el = document.createElement('div');
    el.id    = '_epic_banner';
    const bg = { success: '#107c10', error: '#c4314b', warn: '#d07000', info: '#6264a7' };
    Object.assign(el.style, {
        position: 'fixed', top: '58px', left: '50%',
        transform: 'translateX(-50%)',
        padding: '10px 24px', borderRadius: '4px',
        color: '#fff', fontWeight: '600', zIndex: '9999',
        boxShadow: '0 4px 12px rgba(0,0,0,.2)',
        fontSize: '13px', fontFamily: 'var(--font)',
        background: bg[type] ?? bg.info,
        whiteSpace: 'nowrap', transition: 'opacity .4s',
        pointerEvents: 'none',
    });
    el.textContent = msg;
    document.body.appendChild(el);
    if (dur > 0) {
        setTimeout(() => {
            el.style.opacity = '0';
            setTimeout(() => el.remove(), 400);
        }, dur);
    }
}

/* ══════════════════════════════════════════════════════════════════
   NAVIGATION
══════════════════════════════════════════════════════════════════ */
const NAV_META = {
    productivity: { title: 'Productivity Session', icon: 'fas fa-clock'          },
    breaks:       { title: 'Break Timers',         icon: 'fas fa-hourglass-half' },
    apps:         { title: 'App & URL Tracking',   icon: 'fas fa-th-large'       },
    calls:        { title: 'Calls',                icon: 'fas fa-phone'          },
    settings:     { title: 'Settings',             icon: 'fas fa-cog'            },
};

function navigate(page) {
    /* update sidebar active state */
    $$('.sn-item').forEach(b => b.classList.remove('active'));
    const navBtn = document.querySelector(`.sn-item[data-page="${page}"]`);
    if (navBtn) navBtn.classList.add('active');

    /* show correct page panel */
    $$('.page').forEach(p => p.classList.remove('active'));
    const pg = $(`page-${page}`);
    if (pg) pg.classList.add('active');

    /* update page-bar title + icon */
    const m = NAV_META[page];
    if (m) {
        setText('pb-title', m.title);
        const icon = $('pb-icon');
        if (icon) icon.className = `${m.icon} page-bar__icon`;
    }

    /* extra re-renders for pages with dynamic content */
    if (page === 'breaks') {
        wireBreakPage();   /* safe to call multiple times — guarded */
        renderTimers();
    }
    if (page === 'apps') {
        renderApps();
        renderUrls();
    }

    /* close mobile sidenav */
    document.querySelector('.sidenav')?.classList.remove('open');
    $('nav-overlay')?.classList.remove('show');
}

function wireNav() {
    $$('.sn-item[data-page]').forEach(btn => {
        btn.addEventListener('click', () => navigate(btn.dataset.page));
    });
}

/* ══════════════════════════════════════════════════════════════════
   MOBILE HAMBURGER
══════════════════════════════════════════════════════════════════ */
function wireMobileMenu() {
    $('mob-menu-btn')?.addEventListener('click', () => {
        document.querySelector('.sidenav')?.classList.toggle('open');
        $('nav-overlay')?.classList.toggle('show');
    });
    $('nav-overlay')?.addEventListener('click', () => {
        document.querySelector('.sidenav')?.classList.remove('open');
        $('nav-overlay')?.classList.remove('show');
    });
}

/* ══════════════════════════════════════════════════════════════════
   WINDOW CONTROLS  (Tauri decorations=false)
══════════════════════════════════════════════════════════════════ */
function wireWindowControls() {
    const getWin = () => window.__TAURI__?.window?.getCurrentWindow?.() ?? null;

    async function syncMaxIcon() {
        try {
            const w = getWin(); if (!w) return;
            const icon = $('btn-maximize')?.querySelector('i');
            if (icon) icon.className = (await w.isMaximized())
                ? 'far fa-window-restore'
                : 'far fa-window-maximize';
        } catch { /* ignore */ }
    }

    $('btn-minimize')?.addEventListener('click', async () => {
        try { await getWin()?.minimize(); } catch { }
    });

    $('btn-maximize')?.addEventListener('click', async () => {
        try {
            const w = getWin(); if (!w) return;
            (await w.isMaximized()) ? await w.unmaximize() : await w.maximize();
            syncMaxIcon();
        } catch { }
    });

    $('btn-close')?.addEventListener('click', async () => {
        try { await getWin()?.close(); } catch { }
    });

    window.addEventListener('resize', syncMaxIcon);
    syncMaxIcon();
}

/* ══════════════════════════════════════════════════════════════════
   PROFILE POPOVER
══════════════════════════════════════════════════════════════════ */
function wirePopover() {
    const pop  = $('popover');
    const prof = $('btn-profile');
    if (!pop || !prof) {
        console.warn('[wirePopover] elements not found — header may not have loaded');
        return;
    }

    function place() {
        const r = prof.getBoundingClientRect();
        pop.style.top   = (r.bottom + 4) + 'px';
        pop.style.right = (window.innerWidth - r.right) + 'px';
        pop.style.left  = 'auto';
    }

    prof.addEventListener('click', e => {
        e.stopPropagation();
        const opening = !pop.classList.contains('open');
        pop.classList.toggle('open', opening);
        prof.setAttribute('aria-expanded', String(opening));
        if (opening) place();
    });

    document.addEventListener('click', e => {
        if (pop.classList.contains('open') && !pop.contains(e.target) && e.target !== prof) {
            pop.classList.remove('open');
            prof.setAttribute('aria-expanded', 'false');
        }
    });

    document.addEventListener('keydown', e => {
        if (e.key === 'Escape' && pop.classList.contains('open')) {
            pop.classList.remove('open');
            prof.setAttribute('aria-expanded', 'false');
        }
    });

    window.addEventListener('resize', () => { if (pop.classList.contains('open')) place(); });

    /* ── Popover actions ── */
    async function doSignout() {
        try { await invoke('logout'); } catch { }
        sessionStorage.clear();
        window.location.replace('find_your_organization.html');
    }

    $('pop-signout')     ?.addEventListener('click', doSignout);
    $('pop-signout-org') ?.addEventListener('click', doSignout);
    $('settings-signout')?.addEventListener('click', doSignout);

    $('pop-about')?.addEventListener('click', () => {
        pop.classList.remove('open');
        showBanner('EPIC v1.0.0 — Aiprus Software', 'info', 4000);
    });

    $('pop-analytics')?.addEventListener('click', async () => {
        pop.classList.remove('open');
        try { await invoke('send_analytics'); showBanner('Analytics sent ✓', 'success'); }
        catch (e) { showBanner('Failed: ' + e, 'error'); }
    });

    $('pop-refresh')?.addEventListener('click', async () => {
        pop.classList.remove('open');
        try { await invoke('refresh_data'); showBanner('Refreshed ✓', 'success'); } catch { }
    });

    $('pop-logs')?.addEventListener('click', async () => {
        pop.classList.remove('open');
        try { await invoke('send_logs'); showBanner('Logs sent ✓', 'success'); } catch { }
    });
}

/* ══════════════════════════════════════════════════════════════════
   STOPWATCH  (session timer — reads from Tauri every second)
══════════════════════════════════════════════════════════════════ */
let _swInterval = null;
let activeSec   = 0;
let goalH       = 8;

const fmtClock = s => [
    Math.floor(s / 3600),
    Math.floor((s % 3600) / 60),
    s % 60,
].map(v => String(v).padStart(2, '0')).join(':');

async function tickSW() {
    try {
        const v = await invoke('get_total_active_seconds');
        if (v !== null && v !== undefined) activeSec = Number(v);
    } catch { /* keep last value */ }
    setText('stopwatchDisplay', fmtClock(activeSec));
    updateRing();
    updateGlance();
}

function startSW() {
    tickSW();
    _swInterval = setInterval(tickSW, 1000);
}

function stopSW() {
    clearInterval(_swInterval);
    _swInterval = null;
    activeSec   = 0;
    setText('stopwatchDisplay', '00:00:00');
    updateRing();
    updateGlance();
}

function setSessionUI(active) {
    const btnStart = $('startStopwatch');
    const btnEnd   = $('endStopwatch');
    const pill     = $('sess-pill');

    if (btnStart) {
        btnStart.disabled   = active;
        btnStart.innerHTML  = '<i class="fas fa-play"></i> Start session';
    }
    if (btnEnd) {
        btnEnd.disabled = !active;
    }
    if (pill) {
        pill.classList.toggle('running', active);
        setText('sess-pill-txt', active ? 'Session active' : 'Not started');
    }
}

/* ── Wire the Productivity page ── */
function wireProductivityPage() {
    /* Start session */
    $('startStopwatch')?.addEventListener('click', async () => {
        const btn = $('startStopwatch');
        try {
            const already = await invoke('get_status') ?? false;
            if (already) { showBanner('Already checked in!', 'warn'); return; }
            if (btn) {
                btn.disabled  = true;
                btn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Starting…';
            }
            await invoke('checkin');
            setSessionUI(true);
            startSW();
            showBanner('✅ Session started', 'success');
            addDailyEvent('checkin', 'Check-in',
                new Date().toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' }));
        } catch (err) {
            if (btn) {
                btn.disabled  = false;
                btn.innerHTML = '<i class="fas fa-play"></i> Start session';
            }
            showBanner('Check-in failed: ' + err, 'error');
        }
    });

    /* End session */
    $('endStopwatch')?.addEventListener('click', async () => {
        try {
            await invoke('checkout');
            setSessionUI(false);
            stopSW();
            showBanner('✅ Session ended — data saved', 'success');
            addDailyEvent('checkout', 'Check-out',
                new Date().toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' }));
        } catch (e) { showBanner('Check-out failed: ' + e, 'error'); }
    });

    /* Goal selector */
    $('goal-sel')?.addEventListener('change', function () {
        goalH = Number(this.value);
        setText('stat-goal', this.value);
        updateRing();
    });

    $('edit-goal-btn')?.addEventListener('click', () => navigate('settings'));

    /* Task panel */
    $('task-add-btn')?.addEventListener('click', () => {
        $('task-input-row')?.classList.toggle('show');
        $('task-input')?.focus();
    });
    $('task-cancel')?.addEventListener('click', () => {
        $('task-input-row')?.classList.remove('show');
    });
    $('task-save')?.addEventListener('click', addTask);
    $('task-input')?.addEventListener('keydown', e => {
        if (e.key === 'Enter')  addTask();
        if (e.key === 'Escape') $('task-input-row')?.classList.remove('show');
    });

    renderTasks();
    renderWeek();
    updateRing();
    updateGlance();
}

/* ══════════════════════════════════════════════════════════════════
   DAILY PROGRESS RING  +  BARS  +  GLANCE
══════════════════════════════════════════════════════════════════ */
const CIRCUMFERENCE = 364;   /* 2πr where r=58 in the SVG */
let idleSec         = 0;
let breakSec        = 0;
let keystrokesToday = 0;
let clicksToday     = 0;

function fmtDur(s) {
    if (!s || s <= 0) return '—';
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

function updateRing() {
    const progress = Math.min(activeSec / (goalH * 3600), 1);

    /* active arc */
    const ringEl = $('ring-circle');
    if (ringEl) ringEl.style.strokeDashoffset = CIRCUMFERENCE - (progress * CIRCUMFERENCE);

    /* idle arc */
    const total   = activeSec + idleSec;
    const idleFrac = total > 0 ? Math.min(idleSec / total, 1) : 0;
    const idleEl  = $('ring-idle-circle');
    if (idleEl) idleEl.style.strokeDashoffset = CIRCUMFERENCE - (idleFrac * CIRCUMFERENCE);

    /* centre label */
    const mins = Math.floor(activeSec / 60);
    const hrs  = activeSec / 3600;
    setText('ring-num',  mins >= 60 ? hrs.toFixed(1) : mins);
    setText('ring-unit', mins >= 60 ? 'hours' : 'min');

    /* score badge */
    const pct     = Math.round(progress * 100);
    const badge   = $('score-badge');
    const badgeTxt = $('score-badge-txt');
    if (badge && badgeTxt) {
        badgeTxt.textContent = pct + '% of daily goal';
        badge.className = 'score-badge ' + (
            pct >= 80 ? 'score-badge--high' :
            pct >= 40 ? 'score-badge--mid'  : 'score-badge--low'
        );
    }

    /* active / idle / break bars */
    const allTime = Math.max(activeSec + idleSec + breakSec, 1);
    const ap = Math.round((activeSec / allTime) * 100);
    const ip = Math.round((idleSec   / allTime) * 100);
    const bp = Math.round((breakSec  / allTime) * 100);
    const ab = $('dp-active-bar'), ib = $('dp-idle-bar'), bb = $('dp-break-bar');
    if (ab) ab.style.width = ap + '%';
    if (ib) ib.style.width = ip + '%';
    if (bb) bb.style.width = bp + '%';
    setText('dp-active-label', fmtDur(activeSec));
    setText('dp-idle-label',   fmtDur(idleSec));
    setText('dp-break-label',  fmtDur(breakSec));
}

function addDailyEvent(type, label, time, dur) {
    const list = $('dp-event-list'); if (!list) return;
    const ph = list.querySelector('.dp-break-item');
    if (ph && ph.textContent.includes('No events')) ph.remove();
    const dot  = type === 'break' ? 'dp-break-dot--break' : '';
    const item = document.createElement('div');
    item.className = 'dp-break-item';
    item.innerHTML = `
        <span class="dp-break-dot ${dot}"></span>
        <span>${label}</span>
        <span style="font-size:11px;color:var(--text-muted);margin-left:4px;">${time}</span>
        ${dur ? `<span class="dp-break-time">${dur}</span>` : ''}`;
    list.appendChild(item);
}

function updateGlance() {
    const totalMins = Math.floor(activeSec / 60);
    const h = Math.floor(totalMins / 60), m = totalMins % 60;
    setText('g-active', h > 0 ? `${h}h ${m}m` : `${totalMins}m`);
    setText('g-idle',   fmtDur(idleSec));
    setText('g-break',  fmtDur(breakSec));
    const score = Math.min(Math.round((activeSec / (goalH * 3600)) * 100), 100);
    setText('g-score',  activeSec > 0 ? score + '%' : '—');
    setText('g-keys',   keystrokesToday > 0 ? keystrokesToday.toLocaleString() : '—');
    setText('g-clicks', clicksToday     > 0 ? clicksToday.toLocaleString()     : '—');
}

/* ══════════════════════════════════════════════════════════════════
   WEEKLY CHART
══════════════════════════════════════════════════════════════════ */
function renderWeek() {
    const el = $('week-chart'); if (!el) return;

    const now    = new Date();
    const dow    = now.getDay();  /* 0=Sun */
    const monday = new Date(now);
    monday.setDate(now.getDate() - (dow === 0 ? 6 : dow - 1));
    const sunday = new Date(monday);
    sunday.setDate(monday.getDate() + 6);
    const fmt = d => d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short' });
    setText('week-range', `${fmt(monday)} – ${fmt(sunday)}`);

    const DAYS   = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
    const hrs    = [7.2, 6.8, 8.0, 5.5, 0, 0, 0]; /* TODO: replace with invoke('get_weekly_hours') */
    const maxH   = Math.max(...hrs, goalH, 1);
    const todayI = dow === 0 ? 6 : dow - 1;

    el.innerHTML = DAYS.map((d, i) => {
        const today   = (i === todayI);
        const hasData = hrs[i] > 0;
        const pct     = Math.round((hrs[i] / maxH) * 100);
        return `<div class="wc-bar">
            <div class="wc-fill${today ? ' today' : ''}${hasData ? ' has-data' : ''}"
                 style="height:${Math.max(pct, hasData ? 6 : 3)}%"
                 title="${d}: ${hasData ? hrs[i].toFixed(1) + 'h' : 'No session'}"></div>
            <div class="wc-lbl${today ? ' today-lbl' : ''}">${d}</div>
        </div>`;
    }).join('');

    const worked    = hrs.filter(v => v > 0).length;
    const totalHrs  = hrs.reduce((a, b) => a + b, 0);
    const avgHrs    = worked > 0 ? totalHrs / worked : 0;
    const bestIdx   = hrs.indexOf(Math.max(...hrs));
    setText('ws-total', totalHrs.toFixed(1) + 'h');
    setText('ws-avg',   avgHrs.toFixed(1) + 'h');
    setText('ws-best',  hrs[bestIdx] > 0 ? DAYS[bestIdx] : '—');
    setText('ws-days',  worked + '/5');
}

/* ══════════════════════════════════════════════════════════════════
   TASKS
══════════════════════════════════════════════════════════════════ */
let tasks = [];

function addTask() {
    const inp = $('task-input');
    const txt = inp?.value?.trim();
    if (!txt) return;
    tasks.push({ id: Date.now(), text: txt, done: false });
    inp.value = '';
    $('task-input-row')?.classList.remove('show');
    renderTasks();
}

function renderTasks() {
    const list  = $('task-list');
    const empty = $('task-empty');
    if (!list) return;
    if (!tasks.length) {
        list.innerHTML = '';
        if (empty) empty.style.display = '';
        return;
    }
    if (empty) empty.style.display = 'none';
    list.innerHTML = tasks.map(t => `
        <div class="task-item${t.done ? ' done' : ''}" data-id="${t.id}">
            <div class="task-check"></div>
            <span class="task-txt">${t.text}</span>
        </div>`).join('');
    list.querySelectorAll('.task-item').forEach(el => {
        el.addEventListener('click', () => {
            const t = tasks.find(x => x.id === Number(el.dataset.id));
            if (t) { t.done = !t.done; renderTasks(); }
        });
    });
}

/* ══════════════════════════════════════════════════════════════════
   BREAK TIMERS
   ──────────────────────────────────────────────────────────────────
   CSS classes used (must match epic.css exactly):
     .timer-card  .tc-info  .tc-name  .tc-time  .tc-controls
     .icon-btn  .icon-btn--primary  .icon-btn--del

   HTML structure in break.html:
     id="t-h"  id="t-m"  id="t-s"   (inputs, data-f on buttons)
     id="t-name"  id="add-timer-btn"
     id="timer-list"  id="timer-empty"
     buttons: data-f="t-h" data-s="1"  etc.
══════════════════════════════════════════════════════════════════ */
let _timers         = [];   /* array of timer objects               */
let _timerSeq       = 1;    /* auto-name counter                    */
let _ticks          = {};   /* { id: intervalHandle }               */
let _breakWired     = false; /* prevent double-wiring on re-nav     */

const fmtTimer = s => [
    Math.floor(s / 3600),
    Math.floor((s % 3600) / 60),
    s % 60,
].map(v => String(v).padStart(2, '0')).join(':');

/* ── Render the active-timers list ── */
function renderTimers() {
    const list  = $('timer-list');
    const empty = $('timer-empty');
    if (!list) return;   /* break page not visible yet — skip */

    if (!_timers.length) {
        list.innerHTML = '';
        if (empty) empty.style.display = '';
        return;
    }
    if (empty) empty.style.display = 'none';

    /* Build HTML using the exact class names from epic.css */
    list.innerHTML = _timers.map(t => `
        <div class="timer-card" data-id="${t.id}">
            <div class="tc-info">
                <div class="tc-name">${escHtml(t.name)}</div>
                <div class="tc-time">${fmtTimer(t.rem)}</div>
            </div>
            <div class="tc-controls">
                <button class="icon-btn icon-btn--primary" data-action="tog"
                        aria-label="${t.running ? 'Pause' : 'Start'} timer">
                    <i class="fas fa-${t.running ? 'pause' : 'play'}"></i>
                </button>
                <button class="icon-btn" data-action="rst" aria-label="Reset timer">
                    <i class="fas fa-undo"></i>
                </button>
                <button class="icon-btn icon-btn--del" data-action="del" aria-label="Delete timer">
                    <i class="fas fa-trash"></i>
                </button>
            </div>
        </div>`).join('');

    /* Event delegation per card */
    list.querySelectorAll('.timer-card').forEach(card => {
        const id = Number(card.dataset.id);
        card.querySelectorAll('[data-action]').forEach(btn => {
            btn.addEventListener('click', e => {
                e.stopPropagation();
                const a = btn.dataset.action;
                if (a === 'tog') timerToggle(id);
                if (a === 'rst') timerReset(id);
                if (a === 'del') timerDelete(id);
            });
        });
    });
}

/* ── Step a time field up or down ── */
function stepField(fieldId, dir) {
    const el = $(fieldId);
    if (!el) return;
    const max = fieldId === 't-h' ? 23 : 59;
    let   val = (parseInt(el.value, 10) || 0) + dir;
    if (val < 0)   val = max;
    if (val > max) val = 0;
    el.value = String(val).padStart(2, '0');
}

/* ── Wire break page — called once after break.html is in DOM ── */
function wireBreakPage() {
    if (_breakWired) return;
    _breakWired = true;

    /* ▲▼ step buttons — data-f holds field id, data-s holds direction */
    $$('[data-f][data-s]').forEach(btn => {
        btn.addEventListener('click', () => {
            stepField(btn.dataset.f, Number(btn.dataset.s));
        });
    });

    /* Also allow typing directly — clamp on blur */
    ['t-h', 't-m', 't-s'].forEach(id => {
        const el = $(id); if (!el) return;
        el.addEventListener('blur', () => {
            const max = id === 't-h' ? 23 : 59;
            let val = parseInt(el.value, 10) || 0;
            val = Math.max(0, Math.min(max, val));
            el.value = String(val).padStart(2, '0');
        });
        el.addEventListener('keydown', e => {
            if (e.key === 'Enter') $('add-timer-btn')?.click();
        });
    });

    /* Add timer button */
    $('add-timer-btn')?.addEventListener('click', () => {
        const h   = parseInt($('t-h')?.value,  10) || 0;
        const m   = parseInt($('t-m')?.value,  10) || 0;
        const s   = parseInt($('t-s')?.value,  10) || 0;
        const tot = h * 3600 + m * 60 + s;

        if (tot <= 0) {
            showBanner('Set a duration first (hours / mins / secs)', 'warn');
            return;
        }

        const rawName = $('t-name')?.value?.trim();
        const name    = rawName || `Timer ${_timerSeq}`;
        _timerSeq++;

        _timers.push({ id: Date.now(), name, tot, rem: tot, running: false });

        /* reset inputs */
        ['t-h', 't-m', 't-s'].forEach(id => { const e = $(id); if (e) e.value = '00'; });
        const tn = $('t-name'); if (tn) tn.value = '';

        renderTimers();
        showBanner(`Timer "${name}" added ✓`, 'success', 3000);
    });

    /* Render any timers already in the array (e.g. user navigated away and back) */
    renderTimers();
}

/* ── Toggle start / pause ── */
function timerToggle(id) {
    const t = _timers.find(x => x.id === id);
    if (!t) return;

    if (t.running) {
        /* PAUSE */
        clearInterval(_ticks[id]);
        delete _ticks[id];
        t.running = false;
    } else {
        /* START */
        if (t.rem <= 0) {
            showBanner('Timer is at zero — reset it first', 'warn');
            return;
        }
        t.running = true;
        _ticks[id] = setInterval(() => {
            t.rem -= 1;
            if (t.rem <= 0) {
                t.rem     = 0;
                t.running = false;
                clearInterval(_ticks[id]);
                delete _ticks[id];
                showBanner(`⏰ Timer "${t.name}" finished!`, 'info', 8000);
            }
            renderTimers();
        }, 1000);
    }
    renderTimers();
}

/* ── Reset to full duration ── */
function timerReset(id) {
    const t = _timers.find(x => x.id === id);
    if (!t) return;
    clearInterval(_ticks[id]);
    delete _ticks[id];
    t.running = false;
    t.rem     = t.tot;
    renderTimers();
}

/* ── Delete a timer ── */
function timerDelete(id) {
    clearInterval(_ticks[id]);
    delete _ticks[id];
    _timers = _timers.filter(x => x.id !== id);
    renderTimers();
}

/* ── Tiny HTML escaper ── */
function escHtml(s) {
    return String(s)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
}

/* ══════════════════════════════════════════════════════════════════
   CALL CONTROLS
══════════════════════════════════════════════════════════════════ */
function wireCallPage() {
    function toggleMute(btnId, iconOn, iconOff) {
        const btn = $(btnId); if (!btn) return;
        const muted = btn.classList.toggle('muted');
        const i     = btn.querySelector('i');
        if (i) i.className = `fas ${muted ? iconOff : iconOn}`;
        btn.setAttribute('aria-pressed', String(muted));
    }

    $('btn-call-video')?.addEventListener('click', () =>
        toggleMute('btn-call-video', 'fa-video', 'fa-video-slash'));

    $('btn-call-audio')?.addEventListener('click', () =>
        toggleMute('btn-call-audio', 'fa-microphone', 'fa-microphone-slash'));

    $('btn-call-end')?.addEventListener('click', () => {
        if (!confirm('End the call?')) return;
        ['btn-call-video', 'btn-call-audio'].forEach(id => {
            const btn = $(id); if (!btn) return;
            btn.classList.remove('muted');
            const i = btn.querySelector('i');
            if (i) i.className = `fas ${id === 'btn-call-video' ? 'fa-video' : 'fa-microphone'}`;
            btn.setAttribute('aria-pressed', 'false');
        });
    });
}

/* ══════════════════════════════════════════════════════════════════
   APP & URL TRACKING  (sample data — replace with invoke() when ready)
══════════════════════════════════════════════════════════════════ */
function renderApps() {
    const tb = $('app-tbody'); if (!tb) return;
    const rows = [
        { n: 'Visual Studio Code', ic: 'fa-code',    dur: '2h 14m', pct: 67, cat: 'work'   },
        { n: 'Google Chrome',      ic: 'fa-chrome',  dur: '1h 02m', pct: 31, cat: 'work'   },
        { n: 'Slack',              ic: 'fa-slack',   dur: '28m',    pct: 14, cat: 'work'   },
        { n: 'Microsoft Teams',    ic: 'fa-users',   dur: '15m',    pct:  8, cat: 'work'   },
        { n: 'YouTube',            ic: 'fa-youtube', dur: '11m',    pct:  5, cat: 'other'  },
    ];
    tb.innerHTML = rows.map(a => `<tr>
        <td><span class="app-ico"><i class="fab ${a.ic}"></i></span>${a.n}</td>
        <td>${a.dur}</td>
        <td><div class="bar-cell">
            <div class="bar-mini"><div class="bar-fill" style="width:${a.pct}%"></div></div>
            <span style="font-size:11px;color:var(--text-muted)">${a.pct}%</span>
        </div></td>
        <td><span class="tag tag-${a.cat}">${a.cat[0].toUpperCase() + a.cat.slice(1)}</span></td>
    </tr>`).join('');
}

function renderUrls() {
    const tb = $('url-tbody'); if (!tb) return;
    const rows = [
        { u: 'github.com',        dur: '48m', v: 12, cat: 'work'   },
        { u: 'stackoverflow.com', dur: '22m', v:  8, cat: 'work'   },
        { u: 'docs.rs',           dur: '18m', v:  5, cat: 'work'   },
        { u: 'youtube.com',       dur: '11m', v:  3, cat: 'other'  },
        { u: 'twitter.com',       dur: '6m',  v:  4, cat: 'social' },
    ];
    tb.innerHTML = rows.map(u => `<tr>
        <td><i class="fas fa-globe" style="color:var(--text-muted);margin-right:8px;font-size:11px"></i>${u.u}</td>
        <td>${u.dur}</td>
        <td>${u.v}</td>
        <td><span class="tag tag-${u.cat}">${u.cat[0].toUpperCase() + u.cat.slice(1)}</span></td>
    </tr>`).join('');
}

/* ══════════════════════════════════════════════════════════════════
   USER INFO  (from sessionStorage, set by login.html)
══════════════════════════════════════════════════════════════════ */
function loadUser() {
    const first = sessionStorage.getItem('firstName') || 'U';
    const last  = sessionStorage.getItem('lastName')  || '';
    const email = sessionStorage.getItem('userEmail') || '';
    const ini   = ((first[0] || '') + (last[0] || '')).toUpperCase() || 'U';

    ['tb-avatar', 'pop-av', 'sn-av'].forEach(id => setText(id, ini));
    setText('pop-name',       `${first} ${last}`.trim() || 'User');
    setText('sn-name',        `${first} ${last}`.trim() || 'User');
    setText('pop-email',      email);
    setText('settings-email', email);
}

/* ══════════════════════════════════════════════════════════════════
   STARTUP  (resume Tauri session if active when app was last closed)
══════════════════════════════════════════════════════════════════ */
async function startup() {
    try {
        const st = await invoke('get_startup_status');
        if (!st) return;

        if (st.has_active_session) {
            setSessionUI(true);
            startSW();

            if (st.checkin_time) {
                const t = new Date(st.checkin_time).toLocaleTimeString('en-GB',
                    { hour: '2-digit', minute: '2-digit' });
                addDailyEvent('checkin', 'Check-in (resumed)', t);
            }

            if (st.offline_minutes > 5) {
                showBanner(`⚠️ ${st.offline_minutes} min offline — marked as break`, 'warn');
                addDailyEvent('break',
                    `Offline · ${st.offline_minutes} min`,
                    new Date().toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' }),
                    st.offline_minutes + 'm');
                breakSec += st.offline_minutes * 60;
            } else {
                showBanner('Session resumed', 'success', 3000);
            }

            try { await invoke('resume_tracking'); } catch { }
        }
    } catch (e) {
        console.warn('[startup]', e);
    }
}

/* ══════════════════════════════════════════════════════════════════
   MAIN ENTRY POINT
   ──────────────────────────────────────────────────────────────────
   Sequence:
   1. Wait for DOMContentLoaded (HTML shell ready)
   2. Fetch ALL 7 components in parallel
   3. Await Promise.allSettled — every component is now in the DOM
   4. Wire every listener (safe because DOM is complete)
   5. Populate data + start session if needed
══════════════════════════════════════════════════════════════════ */
document.addEventListener('DOMContentLoaded', async () => {

    /* ── Step 1: load all HTML fragments ── */
    await Promise.allSettled([
        loadComponent('#header',            '/components/header.html'),
        loadComponent('.sidenav',           '/components/navbar.html'),
        loadComponent('#page-productivity', '/pages/productivity.html'),
        loadComponent('#page-breaks',       '/pages/break.html'),
        loadComponent('#page-apps',         '/pages/app_url.html'),
        loadComponent('#page-calls',        '/pages/call.html'),
        loadComponent('#page-settings',     '/pages/settings.html'),
    ]);

    /* ── Step 2: wire all listeners (DOM is fully populated now) ── */
    wireWindowControls();
    wireMobileMenu();
    wireNav();
    wirePopover();

    /* productivity page — already the active page on first load */
    wireProductivityPage();

    /* break page is loaded above so wire it immediately too */
    wireBreakPage();

    /* call page controls */
    wireCallPage();

    /* ── Step 3: populate user info ── */
    loadUser();

    /* ── Step 4: initial renders ── */
    renderWeek();
    renderTasks();
    renderTimers();
    renderApps();
    renderUrls();
    updateRing();
    updateGlance();

    /* ── Step 5: check Tauri for an active session ── */
    startup();
});