// window-resize.js
// Resize the Tauri window between auth (small) and dashboard (large).
//
// Auth pages  → 480 × 560   (Teams-like compact login)
// Dashboard   → 1280 × 800  (full productivity view)
//
// Tauri v2 API — requires withGlobalTauri: true in tauri.conf.json
// Permissions needed in capabilities:
//   core:window:allow-set-size
//   core:window:allow-set-min-size
//   core:window:allow-center

const AUTH_SIZE = { width: 480, height: 560 };
const DASH_SIZE = { width: 1280, height: 800 };

function _getTauriWin() {
    return window.__TAURI__?.window?.getCurrentWindow?.();
}

function _LogicalSize(w, h) {
    // Tauri v2: window.__TAURI__.window.LogicalSize
    const LS = window.__TAURI__?.window?.LogicalSize;
    if (LS) return new LS(w, h);
    // Fallback for older API shape
    return { width: w, height: h, type: 'Logical' };
}

async function _resize(w, h, minW, minH) {
    try {
        const win = _getTauriWin();
        if (!win) return;
        await win.setMinSize(_LogicalSize(minW, minH));
        await win.setSize(_LogicalSize(w, h));
        await win.center();
    } catch(e) {
        console.warn('[window-resize]', e);
    }
}

/**
 * Resize to full dashboard size, then navigate to url.
 * Pass url='' to resize only (no navigation).
 */
async function resizeToDashboardAndGo(url) {
    await _resize(DASH_SIZE.width, DASH_SIZE.height, 900, 600);
    if (url) window.location.replace(url);
}

/**
 * Resize back to small auth size, then navigate to url.
 * Pass url='' to resize only (no navigation).
 */
async function resizeToAuthAndGo(url) {
    await _resize(AUTH_SIZE.width, AUTH_SIZE.height, AUTH_SIZE.width, AUTH_SIZE.height);
    if (url) window.location.replace(url);
}
