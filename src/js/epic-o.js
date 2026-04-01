// For Tauri v2, we use the global __TAURI__ object instead of imports
// The API is injected by Tauri automatically

// Get invoke from the global Tauri object
const { invoke } = window.__TAURI__.core;
let activeStopwatchInterval = null;
let activeSeconds = 0;

// Navigation
const navItems = document.querySelectorAll('.nav-item');
const contents = document.querySelectorAll('.content');
const headerTitle = document.getElementById('headerTitle');
const headerIcon = document.getElementById('headerIcon');

const titles = {
    productivity: 'Productivity Sessions',
    breaks: 'Break Timer',
    calls: 'Calls',
    system: 'System Info'
};

const icons = {
    productivity: 'fas fa-clock',
    breaks: 'fas fa-hourglass-half',
    calls: 'fas fa-phone',
    system: 'fas fa-chart-line'
};

navItems.forEach(item => {
    item.addEventListener('click', () => {
        const page = item.dataset.page;
        
        navItems.forEach(nav => nav.classList.remove('active'));
        item.classList.add('active');
        
        contents.forEach(content => content.classList.remove('active'));
        document.getElementById(page).classList.add('active');
        
        headerTitle.textContent = titles[page];
        headerIcon.className = icons[page] + ' header-icon';
    });
});

// Stopwatch functionality
let stopwatchInterval;
let stopwatchTime = 0;
let stopwatchRunning = false;

const stopwatchDisplay = document.getElementById('stopwatchDisplay');
const startStopwatch = document.getElementById('startStopwatch');
const endStopwatch = document.getElementById('endStopwatch');

function formatStopwatchTime(ms) {
    const hours = Math.floor(ms / 3600000);
    const minutes = Math.floor((ms % 3600000) / 60000);
    const seconds = Math.floor((ms % 60000) / 1000);
    
    return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}

startStopwatch.addEventListener('click', async () => {
    try {
        console.log("Attempting checkin...");
        
        // Check current status first
        const isRunning = await invoke("get_status");
        if (isRunning) {
            alert("Already checked in! Please check out first.");
            return;
        }
        
        // Show loading state
        startStopwatch.disabled = true;
        startStopwatch.innerHTML = '<i class="fas fa-spinner fa-spin"></i>';
        
        // Call Rust check-in (this starts system-level input hooks automatically)
        const checkinTime = await invoke("checkin");
        console.log("✅ Checkin successful at:", checkinTime);
        console.log("🎯 System-level input monitoring active (works when minimized!)");

        // Update UI
        stopwatchRunning = true;
        startStopwatch.innerHTML = '<i class="fas fa-play"></i>';
        endStopwatch.disabled = false;

        // Start stopwatch display
        stopwatchInterval = setInterval(() => {
            stopwatchTime += 1000;
            stopwatchDisplay.textContent = formatStopwatchTime(stopwatchTime);
        }, 1000);

    } catch (err) {
        console.error("❌ Checkin failed:", err);
        
        // Reset button state
        startStopwatch.disabled = false;
        startStopwatch.innerHTML = '<i class="fas fa-play"></i>';
        
        // Show user-friendly error message
        const errorMsg = err.toString();
        
        if (errorMsg.includes("Another application is using system input hooks") || 
            errorMsg.includes("Failed to install")) {
            // Hook installation failure - show detailed help
            alert(`❌ Unable to Start Activity Tracking

EPIC could not access system input monitoring. This usually happens when another application is already using input hooks.

Common causes:
• Screen recording software (OBS, Camtasia, etc.)
• Macro/automation tools (AutoHotkey, etc.)
• Other monitoring applications
• Security software blocking access

Solutions:
1. Close other monitoring/recording applications
2. Try running EPIC as Administrator
3. Restart your computer and try again
4. Check antivirus settings

Technical details:
${err}`);
        } else if (errorMsg.includes("Already checked in")) {
            alert("Already checked in! Please check out first.");
        } else {
            // Generic error
            alert(`Unable to check in: ${err}\n\nPlease check the logs for more details.`);
        }
    }
});

endStopwatch.addEventListener('click', async () => {
    try {
        console.log("Attempting checkout...");
        
        // Check if actually checked in
        const isRunning = await invoke("get_status");
        if (!isRunning) {
            alert("Not checked in! Nothing to check out.");
            return;
        }
        
        // Call Rust check-out (this stops system-level input hooks)
        const checkoutTime = await invoke("checkout");
        console.log("✅ Checkout successful at:", checkoutTime);

        // Stop stopwatch
        stopwatchRunning = false;
        clearInterval(stopwatchInterval);
        
        // Re-enable start button
        startStopwatch.disabled = false;
        endStopwatch.disabled = true;
        
    } catch (err) {
        console.error("❌ Checkout failed:", err);
        alert("Unable to check-out: " + err);
    }
});

// Timer functionality
let timers = [];
let timerCounter = 1;
let activeTimerIntervals = {};

const addTimerBtn = document.getElementById('addTimerBtn');
const timerModal = document.getElementById('timerModal');
const saveTimerBtn = document.getElementById('saveTimerBtn');
const cancelTimerBtn = document.getElementById('cancelTimerBtn');
const timerList = document.getElementById('timerList');
const emptyState = document.getElementById('emptyState');
const editTimersBtn = document.getElementById('editTimersBtn');

function incrementTime(inputId, direction) {
    const input = document.getElementById(inputId);
    let value = parseInt(input.value) || 0;
    const max = inputId === 'modalHours' ? 23 : 59;
    
    value += direction;
    if (value < 0) value = max;
    if (value > max) value = 0;
    
    input.value = String(value).padStart(2, '0');
}

window.incrementTime = incrementTime;

/*addTimerBtn.addEventListener('click', () => {
    document.getElementById('modalHours').value = '00';
    document.getElementById('modalMinutes').value = '00';
    document.getElementById('modalSeconds').value = '00';
    document.getElementById('timerName').value = `Timer (${timerCounter})`;
    timerModal.classList.add('active');
});

cancelTimerBtn.addEventListener('click', () => {
    timerModal.classList.remove('active');
});

timerModal.addEventListener('click', (e) => {
    if (e.target === timerModal) {
        timerModal.classList.remove('active');
    }
});

saveTimerBtn.addEventListener('click', () => {
    const hours = parseInt(document.getElementById('modalHours').value) || 0;
    const minutes = parseInt(document.getElementById('modalMinutes').value) || 0;
    const seconds = parseInt(document.getElementById('modalSeconds').value) || 0;
    const name = document.getElementById('timerName').value || `Timer (${timerCounter})`;
    
    const totalSeconds = hours * 3600 + minutes * 60 + seconds;
    
    if (totalSeconds > 0) {
        const timer = {
            id: Date.now(),
            name: name,
            totalSeconds: totalSeconds,
            remainingSeconds: totalSeconds,
            running: false
        };
        
        timers.push(timer);
        timerCounter++;
        renderTimers();
        timerModal.classList.remove('active');
    }
});*/

function formatTimerTime(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

function renderTimers() {
    if (timers.length === 0) {
        emptyState.style.display = 'flex';
        timerList.style.display = 'none';
        editTimersBtn.style.display = 'none';
    } else {
        emptyState.style.display = 'none';
        timerList.style.display = 'flex';
        editTimersBtn.style.display = 'flex';
        
        timerList.innerHTML = timers.map(timer => `
            <div class="timer-card">
                <div class="timer-card-info">
                    <div class="timer-card-name">${timer.name}</div>
                    <div class="timer-card-time">${formatTimerTime(timer.remainingSeconds)}</div>
                </div>
                <div class="timer-card-controls">
                    <button class="icon-btn play" onclick="toggleTimer(${timer.id})">
                        <i class="fas fa-${timer.running ? 'pause' : 'play'}"></i>
                    </button>
                    <button class="icon-btn" onclick="resetTimer(${timer.id})">
                        <i class="fas fa-redo"></i>
                    </button>
                    <button class="icon-btn delete" onclick="deleteTimer(${timer.id})">
                        <i class="fas fa-trash"></i>
                    </button>
                </div>
            </div>
        `).join('');
    }
}

window.toggleTimer = function(id) {
    const timer = timers.find(t => t.id === id);
    if (!timer) return;
    
    timer.running = !timer.running;
    
    if (timer.running) {
        activeTimerIntervals[id] = setInterval(() => {
            timer.remainingSeconds--;
            
            if (timer.remainingSeconds <= 0) {
                clearInterval(activeTimerIntervals[id]);
                delete activeTimerIntervals[id];
                timer.running = false;
                timer.remainingSeconds = 0;
                alert(`Timer "${timer.name}" finished!`);
            }
            
            renderTimers();
        }, 1000);
    } else {
        clearInterval(activeTimerIntervals[id]);
        delete activeTimerIntervals[id];
    }
    
    renderTimers();
};

window.resetTimer = function(id) {
    const timer = timers.find(t => t.id === id);
    if (!timer) return;
    
    if (activeTimerIntervals[id]) {
        clearInterval(activeTimerIntervals[id]);
        delete activeTimerIntervals[id];
    }
    
    timer.running = false;
    timer.remainingSeconds = timer.totalSeconds;
    renderTimers();
};

window.deleteTimer = function(id) {
    if (activeTimerIntervals[id]) {
        clearInterval(activeTimerIntervals[id]);
        delete activeTimerIntervals[id];
    }
    
    timers = timers.filter(t => t.id !== id);
    renderTimers();
};

// Call controls
/*const videoBtn = document.getElementById('videoBtn');
const audioBtn = document.getElementById('audioBtn');
const endCallBtn = document.getElementById('endCallBtn');

videoBtn.addEventListener('click', () => {
    videoBtn.classList.toggle('muted');
    const icon = videoBtn.querySelector('i');
    if (videoBtn.classList.contains('muted')) {
        icon.className = 'fas fa-video-slash';
    } else {
        icon.className = 'fas fa-video';
    }
});

audioBtn.addEventListener('click', () => {
    audioBtn.classList.toggle('muted');
    const icon = audioBtn.querySelector('i');
    if (audioBtn.classList.contains('muted')) {
        icon.className = 'fas fa-microphone-slash';
    } else {
        icon.className = 'fas fa-microphone';
    }
});

endCallBtn.addEventListener('click', () => {
    if (confirm('End the call?')) {
        videoBtn.classList.remove('muted');
        audioBtn.classList.remove('muted');
        videoBtn.querySelector('i').className = 'fas fa-video';
        audioBtn.querySelector('i').className = 'fas fa-microphone';
    }
});

// Focus session
let focusInterval;
let focusTime = 0;
let focusRunning = false;
let totalFocusMinutes = 0;
let sessionCount = 0;

const focusDisplay = document.getElementById('focusDisplay');
const startFocus = document.getElementById('startFocus');
const pauseFocus = document.getElementById('pauseFocus');
const stopFocus = document.getElementById('stopFocus');

function formatFocusTime(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

startFocus.addEventListener('click', () => {
    focusRunning = true;
    startFocus.disabled = true;
    pauseFocus.disabled = false;
    stopFocus.disabled = false;
    
    focusInterval = setInterval(() => {
        focusTime++;
        focusDisplay.textContent = formatFocusTime(focusTime);
    }, 1000);
});

pauseFocus.addEventListener('click', () => {
    clearInterval(focusInterval);
    focusRunning = false;
    startFocus.disabled = false;
    startFocus.textContent = 'Resume';
    pauseFocus.disabled = true;
});

stopFocus.addEventListener('click', () => {
    clearInterval(focusInterval);
    totalFocusMinutes += Math.floor(focusTime / 60);
    sessionCount++;
    focusTime = 0;
    focusRunning = false;
    focusDisplay.textContent = '00:00:00';
    startFocus.disabled = false;
    startFocus.textContent = 'Start Focus';
    pauseFocus.disabled = true;
    stopFocus.disabled = true;
    
    document.getElementById('sessionsValue').textContent = sessionCount;
    const hours = Math.floor(totalFocusMinutes / 60);
    const mins = totalFocusMinutes % 60;
    document.getElementById('totalTimeValue').textContent = `${hours}h ${mins}m`;
});

// Simulate system stats
function updateSystemStats() {
    const cpu = (Math.random() * 60 + 20).toFixed(1);
    const mem = (Math.random() * 40 + 40).toFixed(1);
    document.getElementById('cpuValue').textContent = `${cpu}%`;
    document.getElementById('memValue').textContent = `${mem}%`;
}

setInterval(updateSystemStats, 2000);
updateSystemStats();*/

function showBanner(message, type = "info", duration = 8000) {
    // Remove any existing banner
    const existing = document.getElementById('appBanner');
    if (existing) {
        existing.remove();
    }

    // Create banner element
    const banner = document.createElement('div');
    banner.id = 'appBanner';
    banner.textContent = message;
    banner.style.cssText = `
        position: fixed;
        top: 10px;
        left: 50%;
        transform: translateX(-50%);
        padding: 12px 24px;
        border-radius: 8px;
        color: white;
        font-weight: bold;
        z-index: 10000;
        box-shadow: 0 4px 12px rgba(0,0,0,0.3);
        transition: opacity 0.5s;
        min-width: 300px;
        text-align: center;
    `;

    // Set background color based on type
    switch (type) {
        case "success":
            banner.style.backgroundColor = "#28a745";
            break;
        case "warning":
            banner.style.backgroundColor = "#ffc107";
            banner.style.color = "#212529";
            break;
        case "error":
            banner.style.backgroundColor = "#dc3545";
            break;
        case "info":
        default:
            banner.style.backgroundColor = "#17a2b8";
            break;
    }

    document.body.appendChild(banner);

    // Auto-hide after duration (unless duration = 0 for permanent)
    if (duration > 0) {
        setTimeout(() => {
            banner.style.opacity = '0';
            setTimeout(() => banner.remove(), 500);
        }, duration);
    }
}

function formatActiveTime(seconds) {
    const h = Math.floor(seconds / 3600).toString().padStart(2, '0');
    const m = Math.floor((seconds % 3600) / 60).toString().padStart(2, '0');
    const s = (seconds % 60).toString().padStart(2, '0');
    return `${h}:${m}:${s}`;
}

async function updateActiveStopwatch() {
    try {
        // Call your new Rust command to get active seconds (excluding breaks)
        activeSeconds = await invoke("get_total_active_seconds");
    } catch (err) {
        console.error("Failed to fetch active time:", err);
        // Fallback: keep last known value
    }
    
    document.getElementById('stopwatchDisplay').textContent = formatActiveTime(activeSeconds);
}

function startActiveStopwatch() {
    // Initial update
    updateActiveStopwatch();
    
    // Update every second for smooth counting
    activeStopwatchInterval = setInterval(updateActiveStopwatch, 1000);
}

function stopActiveStopwatch() {
    if (activeStopwatchInterval) {
        clearInterval(activeStopwatchInterval);
        activeStopwatchInterval = null;
    }
    document.getElementById('stopwatchDisplay').textContent = "00:00:00";
}

// Check status on page load
/*document.addEventListener('DOMContentLoaded', async () => {
    try {
        const status = await invoke("get_startup_status");

        if (status.has_active_session) {
            //alert("in");
            // Auto-resume: Update UI to show checked-in state
            document.getElementById('startStopwatch').disabled = true;
            document.getElementById('endStopwatch').disabled = false;
            console.log(`Active since ${status.checkin_time}`);
            //document.getElementById('statusText').textContent = `Active since ${status.checkin_time}`;

            // Show resume banner
            if (status.offline_minutes > 5) {
                showBanner(`⚠️ ${status.message}`, "warning");
            } else {
                showBanner(`✅ ${status.message}`, "success");
            }

            // === AUTO-RESUME HOOKS (keyboard/mouse tracking) ===
            try {
                const result = await invoke("resume_tracking");
                console.log("Hooks resumed:", result);
                showBanner("✅ Keyboard & mouse tracking active", "success", 5000);
            } catch (err) {
                console.error("Failed to resume hooks:", err);
                showBanner("⚠️ Tracking unavailable — antivirus or conflicting app detected", "error", 0); // permanent
                showBanner("Close other monitoring apps and restart EPIC", "warning", 0);
            }
        } else {
            // Fresh start — no active session
            document.getElementById('startStopwatch').disabled = false;
            document.getElementById('endStopwatch').disabled = true;
            showBanner("Ready to check in", "info");
        }
    } catch (err) {
        console.log("Startup status check failed:", err);
        //showBanner("⚠️ Could not connect to backend. Please restart the app.", "error");
    }
});*/
document.addEventListener('DOMContentLoaded', async () => {
    try {
        const status = await invoke("get_startup_status");

        if (status.has_active_session) {
            // === UI Resume ===
            document.getElementById('startStopwatch').disabled = true;
            document.getElementById('endStopwatch').disabled = false;
            console.log(`Active since ${status.checkin_time}`);
            //document.getElementById('statusText').textContent = `Active since ${status.checkin_time}`;

            // === Show Banner ===
            if (status.offline_minutes > 5) {
                showBanner(`⚠️ ${status.message} (Offline time marked as break)`, "warning");
            } else {
                showBanner(`✅ Session resumed`, "success");
            }

            // === Resume Hooks (Keyboard/Mouse Tracking) ===
            try {
                await invoke("resume_tracking");
                console.log("Hooks resumed successfully");
            } catch (err) {
                console.error("Hook resume failed:", err);
                showBanner("⚠️ Tracking unavailable — close antivirus/conflicting apps", "error", 0);
            }

            // === START ACCURATE STOPWATCH (Excludes Offline/Breaks) ===
            startActiveStopwatch();

        } else {
            // Fresh start — no active session
            document.getElementById('startStopwatch').disabled = false;
            document.getElementById('endStopwatch').disabled = true;

            stopActiveStopwatch();  // Reset to 00:00:00
            showBanner("Ready to check in", "info");
        }
    } catch (err) {
        console.error("Startup failed:", err);
        showBanner("⚠️ App error — please restart", "error");
        stopActiveStopwatch();
    }

    // Your existing system stats update, etc.
});