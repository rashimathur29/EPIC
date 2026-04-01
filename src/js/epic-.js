// js/epic.js — Diagnostic + Fixed Version

const invoke = async (cmd, args = {}) => {
    try {
        const result = await window.__TAURI__.core.invoke(cmd, args);
        console.log(`[INVOKE SUCCESS] ${cmd} →`, result);
        return result;
    } catch (err) {
        console.error(`[INVOKE FAILED] ${cmd}:`, err);
        throw err;
    }
};

const fmtTime = (sec) => {
    const h = Math.floor(sec / 3600).toString().padStart(2, '0');
    const m = Math.floor((sec % 3600) / 60).toString().padStart(2, '0');
    const s = (sec % 60).toString().padStart(2, '0');
    return `${h}:${m}:${s}`;
};

let activeInterval = null;

async function updateActiveTime() {
    try {
        const secs = await invoke('get_total_active_seconds');
        console.log(`[ACTIVE TIME] Received ${secs} seconds from Rust`);
        const el = document.getElementById('stopwatchDisplay');
        if (el) el.textContent = fmtTime(secs || 0);
    } catch (e) {
        console.error("Failed to get active seconds", e);
    }
}

function startLiveTimer() {
    console.log("Starting live timer...");
    updateActiveTime();
    if (activeInterval) clearInterval(activeInterval);
    activeInterval = setInterval(updateActiveTime, 1000);
}

function stopLiveTimer() {
    console.log("Stopping live timer");
    if (activeInterval) clearInterval(activeInterval);
    const el = document.getElementById('stopwatchDisplay');
    if (el) el.textContent = "00:00:00";
}

// Check-in
async function handleCheckin() {
    const btn = document.getElementById('startStopwatch');
    if (!btn) return console.warn("Start button not found");

    console.log("handleCheckin called");

    try {
        const isRunning = await invoke('get_status');
        console.log("Current status:", isRunning);

        if (isRunning) {
            showBanner('Already checked in!', 'warn');
            return;
        }

        btn.disabled = true;
        btn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Starting…';

        const checkinTime = await invoke('checkin');
        console.log("Checkin successful:", checkinTime);

        showBanner('✅ Session started', 'success');
        startLiveTimer();

        btn.disabled = true;
        document.getElementById('endStopwatch').disabled = false;
    } catch (err) {
        console.error("Checkin error:", err);
        btn.disabled = false;
        btn.innerHTML = '<i class="fas fa-play"></i> Start session';
        showBanner('Check-in failed: ' + (err.message || err), 'error');
    }
}

// Checkout
async function handleCheckout() {
    try {
        await invoke('checkout');
        showBanner('✅ Session ended', 'success');
        stopLiveTimer();
        document.getElementById('startStopwatch').disabled = false;
        document.getElementById('endStopwatch').disabled = true;
    } catch (e) {
        showBanner('Check-out failed', 'error');
    }
}

function showBanner(msg, type = 'info', dur = 5000) {
    let b = document.getElementById('banner');
    if (b) b.remove();

    b = document.createElement('div');
    b.id = 'banner';
    b.textContent = msg;
    b.style.cssText = `position:fixed;top:60px;left:50%;transform:translateX(-50%);padding:12px 28px;border-radius:6px;color:#fff;font-weight:600;z-index:99999;background:${type==='success'?'#107c10':type==='error'?'#c4314b':'#6264a7'};`;
    document.body.appendChild(b);
    if (dur > 0) setTimeout(() => { b.style.opacity = '0'; setTimeout(() => b.remove(), 400); }, dur);
}

// Init
function initEpic() {
    console.log("🚀 EPIC JS initialized - binding buttons");

    const startBtn = document.getElementById('startStopwatch');
    const endBtn = document.getElementById('endStopwatch');

    if (startBtn) startBtn.addEventListener('click', handleCheckin);
    if (endBtn) endBtn.addEventListener('click', handleCheckout);

    document.querySelectorAll('.sn-item[data-page]').forEach(btn => {
        btn.addEventListener('click', () => navigate(btn.dataset.page));
    });

    startup();
}

async function startup() {
    try {
        const st = await invoke('get_startup_status');
        console.log("Startup status:", st);

        if (st.has_active_session) {
            document.getElementById('startStopwatch').disabled = true;
            document.getElementById('endStopwatch').disabled = false;
            startLiveTimer();
            showBanner(st.offline_minutes > 5 ? `⚠️ ${st.offline_minutes} min offline` : '✅ Session resumed', 'success');
        } else {
            stopLiveTimer();
        }
    } catch (e) {
        console.warn("Startup failed", e);
    }
}

document.addEventListener('DOMContentLoaded', () => {
    setTimeout(initEpic, 1500);
});