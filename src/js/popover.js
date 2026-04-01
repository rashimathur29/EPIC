/**
 * js/popover.js
 *
 * Two encapsulated modules:
 *
 *   ProfilePopover  — drops from the title-bar avatar (top-right).
 *                     Dispatches CustomEvents for every action so
 *                     epic.js / app.js can handle Tauri calls
 *                     without this file knowing about them.
 *
 *   AboutPanel      — right-edge slide panel with About + Privacy tabs.
 *                     Opened by ProfilePopover or directly via the
 *                     public API: AboutPanel.open('privacy').
 *
 * Depends on: css/popover.css, css/tokens.css (already linked in index.html)
 * Loaded by:  index.html (defer)
 */

'use strict';

/* ================================================================
   ProfilePopover
================================================================ */
const ProfilePopover = (() => {

    let _open = false;
    let _btn  = null;   // #btn-profile  (title-bar avatar)
    let _el   = null;   // #profile-popover

    /* Position the popover flush under the avatar button */
    function _place() {
        if (!_btn || !_el) return;
        const r   = _btn.getBoundingClientRect();
        const vpW = window.innerWidth;
        const popW = _el.offsetWidth || 300;
        const right = Math.min(vpW - r.right, vpW - popW - 8);
        _el.style.top   = `${r.bottom + 4}px`;
        _el.style.right = `${right}px`;
        _el.style.left  = 'auto';
    }

    function open() {
        if (_open) return;
        _open = true;
        _place();
        _el.classList.add('is-open');
        _btn.setAttribute('aria-expanded', 'true');
    }

    function close() {
        if (!_open) return;
        _open = false;
        _el.classList.remove('is-open');
        _btn.setAttribute('aria-expanded', 'false');
    }

    function toggle() { _open ? close() : open(); }

    /* Called by app.js after user data arrives from Rust */
    function setUser({ initials = '', name = '', role = '', email = '' } = {}) {
        [
            ['popover-avatar-initials',  initials],
            ['popover-user-name',        name],
            ['popover-user-role',        role],
            ['popover-user-email',       email],
            ['titlebar-avatar-initials', initials],  // mirror in title bar
        ].forEach(([id, val]) => {
            const el = document.getElementById(id);
            if (el) el.textContent = val;
        });
    }

    /* Wire one action button → close popover → fire CustomEvent */
    function _bindAction(id, eventName) {
        document.getElementById(id)?.addEventListener('click', () => {
            close();
            document.dispatchEvent(new CustomEvent(eventName));
        });
    }

    function init() {
        _btn = document.getElementById('btn-profile');
        _el  = document.getElementById('profile-popover');

        if (!_btn || !_el) {
            console.warn('[ProfilePopover] Elements not found — check IDs.');
            return;
        }

        /* Open/close on avatar click */
        _btn.addEventListener('click', (e) => { e.stopPropagation(); toggle(); });

        /* Close on outside click */
        document.addEventListener('click', (e) => {
            if (_open && !_el.contains(e.target) && e.target !== _btn) close();
        });

        /* Close on Escape */
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && _open) close();
        });

        /* Re-position on resize */
        window.addEventListener('resize', () => { if (_open) _place(); });

        /* "About EPIC" → open About panel */
        document.getElementById('popover-item-about')
            ?.addEventListener('click', () => { close(); AboutPanel.open('about'); });

        /* "Privacy Policy" footer link → open Privacy panel */
        document.getElementById('popover-privacy-link')
            ?.addEventListener('click', (e) => {
                e.preventDefault();
                close();
                AboutPanel.open('privacy');
            });

        /* Action items → CustomEvents (handled in app.js / epic.js) */
        _bindAction('popover-item-analytics',   'epic:send-analytics');
        _bindAction('popover-item-refresh',      'epic:refresh-data');
        _bindAction('popover-item-logs',         'epic:send-logs');
        _bindAction('popover-item-signout-org',  'epic:signout-org');
        _bindAction('popover-item-signout',      'epic:signout');
    }

    return { init, open, close, setUser };

})();


/* ================================================================
   AboutPanel
================================================================ */
const AboutPanel = (() => {

    let _overlay = null;
    let _title   = null;

    const TITLES = { about: 'About EPIC', privacy: 'Privacy Policy' };

    function open(tab = 'about') {
        _overlay?.classList.add('is-open');
        _switchTab(tab);
    }

    function close() {
        _overlay?.classList.remove('is-open');
    }

    function _switchTab(name) {
        document.querySelectorAll('.panel__tab').forEach((t) => {
            const active = t.dataset.tab === name;
            t.classList.toggle('is-active', active);
            t.setAttribute('aria-selected', String(active));
        });

        document.querySelectorAll('.panel__tab-content').forEach((p) => {
            p.classList.toggle('is-active', p.dataset.tabContent === name);
        });

        if (_title) _title.textContent = TITLES[name] ?? name;
    }

    /* Called by app.js to inject version from Rust */
    function setVersion(v) {
        document.getElementById('about-app-version')?.textContent && (
            document.getElementById('about-app-version').textContent = v
        );
    }

    function init() {
        _overlay = document.getElementById('about-panel-overlay');
        _title   = document.getElementById('about-panel-title');

        if (!_overlay) { console.warn('[AboutPanel] Overlay not found.'); return; }

        document.getElementById('about-panel-close')?.addEventListener('click', close);

        /* Click outside panel body → close */
        _overlay.addEventListener('click', (e) => {
            if (e.target === _overlay) close();
        });

        /* Tab switching */
        document.querySelectorAll('.panel__tab').forEach((t) => {
            t.addEventListener('click', () => _switchTab(t.dataset.tab));
        });

        /* Escape */
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && _overlay.classList.contains('is-open')) close();
        });
    }

    return { init, open, close, setVersion };

})();
