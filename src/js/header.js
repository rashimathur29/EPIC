/**
 * js/header.js
 *
 * Window control buttons — minimize, maximize/restore, close.
 *
 * Loaded by: index.html (defer, after DOM ready)
 */

'use strict';

const HeaderControls = (() => {

    /* ── Tauri window handle ── */
    function _win() {
        return window.__TAURI__?.window?.getCurrentWindow?.() ?? null;
    }

    /* ── Actions ── */
    async function minimize() {
        try { await _win()?.minimize(); }
        catch (e) { console.warn('[Header] minimize:', e); }
    }

    async function toggleMaximize() {
        try {
            const win = _win();
            if (!win) return;
            (await win.isMaximized()) ? await win.unmaximize() : await win.maximize();
            _syncMaxIcon();
        } catch (e) { console.warn('[Header] maximize:', e); }
    }

    async function close() {
        try { await _win()?.close(); }
        catch (e) { console.warn('[Header] close:', e); }
    }

    /* Keep maximize icon accurate after external resize */
    async function _syncMaxIcon() {
        try {
            const win  = _win();
            const icon = document.getElementById('btn-maximize')?.querySelector('i');
            if (!win || !icon) return;
            icon.className = (await win.isMaximized())
                ? 'far fa-window-restore'
                : 'far fa-window-maximize';
        } catch (_) { /* non-critical */ }
    }

    /* ── Init ── */
    function init() {
        document.getElementById('btn-minimize') ?.addEventListener('click', minimize);
        document.getElementById('btn-maximize') ?.addEventListener('click', toggleMaximize);
        document.getElementById('btn-close')    ?.addEventListener('click', close);
        window.addEventListener('resize', _syncMaxIcon);
        _syncMaxIcon();
    }

    return { init };

})();

HeaderControls.init();

