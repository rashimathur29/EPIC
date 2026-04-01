/**
 * js/app.js
 *
 * Application UI controller. Modules:
 *
 *   Navigation      — sidebar page switching + header update
 *   BreakTimers     — add / toggle / reset / delete timers
 *   TimerModal      — add-timer dialog lifecycle
 *   CallControls    — mute/unmute video & audio, end call
 *   SystemStats     — stat card display (real data via epic.js)
 *   Collapsible     — sys-section expand/collapse
 *   PopoverBridge   — listens for CustomEvents from popover.js
 *                     and routes them to window.EpicCommands
 *                     (defined at the bottom of epic.js)
 *
 * epic.js owns ALL Tauri invoke() calls.
 * This file never calls invoke() directly.
 *
 * Loaded by: index.html (defer, after popover.js)
 */

'use strict';

/* ================================================================
   Navigation
================================================================ */
const Navigation = (() => {

    const META = {
        productivity: { title: 'Productivity Sessions',  icon: 'fas fa-clock'        },
        breaks:       { title: 'Break Timers',           icon: 'fas fa-hourglass-half'},
        calls:        { title: 'Calls',                  icon: 'fas fa-phone'         },
        system:       { title: 'System Information',     icon: 'fas fa-chart-bar'     },
    };

    function goTo(page) {
        /* Nav items — epic.js uses .nav-item and .active */
        document.querySelectorAll('.nav-item').forEach((btn) => {
            btn.classList.toggle('active', btn.dataset.page === page);
            btn.classList.toggle('is-active', btn.dataset.page === page);
        });

        /* Page panels — epic.js uses .content and id = page name directly */
        document.querySelectorAll('.page').forEach((panel) => {
            panel.classList.toggle('active', panel.id === page);
            panel.classList.toggle('is-active', panel.id === page);
        });

        /* Sub-header — epic.js uses #headerTitle and #headerIcon */
        const m = META[page];
        if (!m) return;
        const title = document.getElementById('headerTitle');
        const icon  = document.getElementById('headerIcon');
        if (title) title.textContent = m.title;
        if (icon)  icon.className    = `${m.icon} page-header__icon`;
    }

    function init() {
        document.querySelectorAll('.nav-item[data-page]').forEach((btn) => {
            btn.addEventListener('click', () => goTo(btn.dataset.page));
        });
    }

    return { init, goTo };

})();


/* ================================================================
   BreakTimers
================================================================ */
const BreakTimers = (() => {

    /** @type {Array<{id:number, name:string, totalSeconds:number,
     *               remainingSeconds:number, running:boolean}>} */
    let _list = [];
    let _seq  = 1;

    /** @type {Record<number, ReturnType<typeof setInterval>>} */
    const _ticks = {};

    /* Helpers */
    function _fmt(s) {
        return [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60]
            .map((v) => String(v).padStart(2, '0')).join(':');
    }

    function _esc(str) {
        return str.replace(/&/g,'&amp;').replace(/</g,'&lt;')
                  .replace(/>/g,'&gt;').replace(/"/g,'&quot;');
    }

    function _stopTick(id) {
        if (_ticks[id] !== undefined) { clearInterval(_ticks[id]); delete _ticks[id]; }
    }

    /* Render */
    function _render() {
        const emptyEl = document.getElementById('breaks-empty');
        const listEl  = document.getElementById('timer-list');
        const editBtn = document.getElementById('btn-edit-timers');
        const isEmpty = _list.length === 0;

        emptyEl.style.display = isEmpty ? 'flex'  : 'none';
        listEl.style.display  = isEmpty ? 'none'  : 'flex';
        if (editBtn) editBtn.style.display = isEmpty ? 'none' : 'flex';
        if (isEmpty) return;

        listEl.innerHTML = _list.map((t) => `
            <li class="timer-card" data-id="${t.id}">
                <div>
                    <p class="timer-card__name">${_esc(t.name)}</p>
                    <p class="timer-card__time">${_fmt(t.remainingSeconds)}</p>
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

    /* Operations */
    function _toggle(id) {
        const t = _list.find((x) => x.id === id);
        if (!t) return;
        t.running = !t.running;

        if (t.running) {
            _ticks[id] = setInterval(() => {
                t.remainingSeconds -= 1;
                if (t.remainingSeconds <= 0) {
                    t.remainingSeconds = 0;
                    t.running = false;
                    _stopTick(id);
                    document.dispatchEvent(
                        new CustomEvent('epic:timer-done', { detail: { id, name: t.name } })
                    );
                }
                _render();
            }, 1000);
        } else {
            _stopTick(id);
        }
        _render();
    }

    function _reset(id) {
        const t = _list.find((x) => x.id === id);
        if (!t) return;
        _stopTick(id);
        t.running = false;
        t.remainingSeconds = t.totalSeconds;
        _render();
    }

    function _delete(id) {
        _stopTick(id);
        _list = _list.filter((x) => x.id !== id);
        _render();
    }

    /* Public */
    function add({ name, hours, minutes, seconds }) {
        const total = hours * 3600 + minutes * 60 + seconds;
        if (total <= 0) return false;
        _list.push({ id: Date.now(), name: name || `Timer ${_seq}`,
                     totalSeconds: total, remainingSeconds: total, running: false });
        _seq++;
        _render();
        return true;
    }

    function init() {
        /* Event delegation — one listener for all timer button clicks */
        document.getElementById('timer-list')?.addEventListener('click', (e) => {
            const btn = e.target.closest('[data-action][data-id]');
            if (!btn) return;
            const id = Number(btn.dataset.id);
            if (btn.dataset.action === 'toggle') _toggle(id);
            if (btn.dataset.action === 'reset')  _reset(id);
            if (btn.dataset.action === 'delete') _delete(id);
        });
        _render();
    }

    return { init, add };

})();


/* ================================================================
   TimerModal
================================================================ */
const TimerModal = (() => {

    let _el = null;

    function _val(id)        { return parseInt(document.getElementById(id)?.value, 10) || 0; }
    function _set(id, v)     { const el = document.getElementById(id); if (el) el.value = String(v).padStart(2,'0'); }

    function open() {
        _set('modal-hours', 0); _set('modal-minutes', 0); _set('modal-seconds', 0);
        const name = document.getElementById('modal-timer-name');
        if (name) name.value = '';
        _el?.classList.add('is-open');
        document.getElementById('modal-timer-name')?.focus();
    }

    function close() { _el?.classList.remove('is-open'); }

    function _save() {
        const ok = BreakTimers.add({
            name:    document.getElementById('modal-timer-name')?.value?.trim() ?? '',
            hours:   _val('modal-hours'),
            minutes: _val('modal-minutes'),
            seconds: _val('modal-seconds'),
        });
        if (ok) close();
    }

    /* Step a time field up or down — called via data-attributes */
    function step(fieldId, direction) {
        const el  = document.getElementById(fieldId);
        if (!el) return;
        const max = fieldId === 'modal-hours' ? 23 : 59;
        let   val = (_val(fieldId) + direction + max + 1) % (max + 1);
        el.value  = String(val).padStart(2, '0');
    }

    function init() {
        _el = document.getElementById('timer-modal');
        if (!_el) return;

        document.getElementById('btn-add-timer')   ?.addEventListener('click', open);
        document.getElementById('btn-modal-cancel') ?.addEventListener('click', close);
        document.getElementById('btn-modal-save')   ?.addEventListener('click', _save);

        /* Backdrop click */
        _el.addEventListener('click', (e) => { if (e.target === _el) close(); });

        /* Escape */
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && _el.classList.contains('is-open')) close();
        });

        /* Enter inside name field */
        document.getElementById('modal-timer-name')
            ?.addEventListener('keydown', (e) => { if (e.key === 'Enter') _save(); });

        /* Step buttons via event delegation (data-field + data-step) */
        _el.addEventListener('click', (e) => {
            const btn = e.target.closest('[data-field][data-step]');
            if (btn) step(btn.dataset.field, Number(btn.dataset.step));
        });
    }

    return { init, open, close };

})();


/* ================================================================
   CallControls
================================================================ */
const CallControls = (() => {

    function _toggleMute(btnId, iconOn, iconOff) {
        const btn  = document.getElementById(btnId);
        const icon = btn?.querySelector('i');
        if (!btn || !icon) return;
        const muted = btn.classList.toggle('is-muted');
        icon.className = `fas ${muted ? iconOff : iconOn}`;
        btn.setAttribute('aria-pressed', String(muted));
    }

    function init() {
        document.getElementById('btn-call-video')
            ?.addEventListener('click', () => _toggleMute('btn-call-video','fa-video','fa-video-slash'));

        document.getElementById('btn-call-audio')
            ?.addEventListener('click', () => _toggleMute('btn-call-audio','fa-microphone','fa-microphone-slash'));

        document.getElementById('btn-call-end')
            ?.addEventListener('click', () => {
                if (!confirm('End the call?')) return;
                ['btn-call-video','btn-call-audio'].forEach((id) => {
                    const btn  = document.getElementById(id);
                    const icon = btn?.querySelector('i');
                    if (!btn || !icon) return;
                    btn.classList.remove('is-muted');
                    btn.setAttribute('aria-pressed','false');
                    icon.className = `fas ${id === 'btn-call-video' ? 'fa-video' : 'fa-microphone'}`;
                });
            });
    }

    return { init };

})();


/* ================================================================
   SystemStats
   Real data injected by epic.js via SystemStats.update().
   Simulation runs only until the first real update arrives.
================================================================ */
const SystemStats = (() => {

    let _simTimer = null;

    function _set(id, val) {
        const el = document.getElementById(id);
        if (el) el.textContent = val;
    }

    /**
     * Called by epic.js when Tauri returns metrics.
     * @param {{ cpu?:string, memory?:string, connType?:string, location?:string,
     *           breakTime?:string, totalBreaks?:string|number, avgBreak?:string }} data
     */
    function update(data = {}) {
        if (data.cpu)         _set('stat-cpu',          data.cpu);
        if (data.memory)      _set('stat-memory',        data.memory);
        if (data.connType)    _set('stat-conn-type',     data.connType);
        if (data.location)    _set('stat-location',      data.location);
        if (data.breakTime)   _set('stat-break-time',    data.breakTime);
        if (data.totalBreaks !== undefined) _set('stat-total-breaks', data.totalBreaks);
        if (data.avgBreak)    _set('stat-avg-break',     data.avgBreak);

        /* Stop simulation once real data arrives */
        if (_simTimer) { clearInterval(_simTimer); _simTimer = null; }
    }

    function _simulate() {
        _set('stat-cpu',    `${(Math.random() * 60 + 20).toFixed(1)}%`);
        _set('stat-memory', `${(Math.random() * 40 + 40).toFixed(1)}%`);
    }

    function init() {
        _simulate();
        _simTimer = setInterval(_simulate, 3000);
    }

    return { init, update };

})();


/* ================================================================
   Collapsible  —  sys-section toggle
================================================================ */
const Collapsible = (() => {

    function init() {
        document.querySelectorAll('.sys-section__toggle').forEach((btn) => {
            btn.addEventListener('click', () => {
                const section = btn.closest('.sys-section');
                if (!section) return;
                const open = section.classList.toggle('is-open');
                btn.setAttribute('aria-expanded', String(open));
            });
        });
    }

    return { init };

})();


/* ================================================================
   PopoverBridge
   Translates CustomEvents from popover.js → EpicCommands
   Requires epic.js to expose:  window.EpicCommands = { ... }
================================================================ */
const PopoverBridge = (() => {

    function _on(event, handler) {
        document.addEventListener(event, handler);
    }

    function _guard(message, fn) {
        if (!confirm(message)) return;
        fn();
    }

    function init() {
        _on('epic:send-analytics', () => window.EpicCommands?.sendAnalytics?.());
        _on('epic:refresh-data',   () => window.EpicCommands?.refreshData?.());
        _on('epic:send-logs',      () => window.EpicCommands?.sendLogs?.());

        _on('epic:signout-org', () =>
            _guard('Sign out of your organization? All local org data will be cleared.',
                   () => window.EpicCommands?.signoutOrg?.()));

        _on('epic:signout', () =>
            _guard('Sign out? You will need to log in again.',
                   () => window.EpicCommands?.signout?.()));

        _on('epic:timer-done', (e) => {
            alert(`Timer "${e.detail.name}" finished!`);
        });
    }

    return { init };

})();


/* ================================================================
   Boot  —  initialise all modules on DOMContentLoaded
================================================================ */
document.addEventListener('DOMContentLoaded', () => {
    Navigation.init();
    BreakTimers.init();
    TimerModal.init();
    CallControls.init();
    SystemStats.init();
    Collapsible.init();
    PopoverBridge.init();

    /* popover.js modules are also deferred — init them here */
    ProfilePopover.init();
    AboutPanel.init();
});