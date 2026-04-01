let currentWindow = null;

// Initialize the app
async function initApp() {
    try {
        // Get the current window using the CORRECT API for your Tauri version
        currentWindow = window.__TAURI__.window.getCurrentWindow();
        
        // Load app information
        await loadAppInfo();
        
        console.log('Tauri app initialized successfully!');
        console.log('Current window:', currentWindow);
        
        // Update status
        //document.querySelector('.status').innerHTML = '✅ Tauri Desktop Mode Active - Window controls ready!';
            
    } catch (error) {
        console.log('Failed to initialize Tauri app:', error);
        //document.getElementById('appInfo').innerHTML = 'Error: ' + error.message;
        /*document.querySelector('.status').innerHTML = 
            '❌ Error initializing Tauri app: ' + error.message;*/
    }
}

// Load app information
async function loadAppInfo() {
    try {
        const app = window.__TAURI__.app;
        const appName = await app.getName();
        const appVersion = await app.getVersion();
        const tauriVersion = await app.getTauriVersion();
        
        console.log(`
            <strong>Name:</strong> ${appName}<br>
            <strong>Version:</strong> ${appVersion}<br>
            <strong>Tauri:</strong> ${tauriVersion}
        `);
    } catch (error) {
        console.log('Could not load app info: ' + error.message);
    }
}

// Window control functions - THESE WILL WORK!
async function minimizeWindow() {
    try {
        if (!currentWindow) {
            currentWindow = window.__TAURI__.window.getCurrentWindow();
        }
        await currentWindow.minimize();
        console.log('✅ Window minimized successfully');
    } catch (error) {
        console.log('Minimize error:', error);
    }
}

async function maximizeWindow() {
    try {
        if (!currentWindow) {
            currentWindow = window.__TAURI__.window.getCurrentWindow();
        }
        const isMaximized = await currentWindow.isMaximized();
        if (isMaximized) {
            await currentWindow.unmaximize();
            console.log('✅ Window unmaximized');
        } else {
            await currentWindow.maximize();
            console.log('✅ Window maximized');
        }
    } catch (error) {
        console.log('Maximize error:', error);
    }
}

async function toggleFullscreen() {
    try {
        if (!currentWindow) {
            currentWindow = window.__TAURI__.window.getCurrentWindow();
        }
        const isFullscreen = await currentWindow.isFullscreen();
        await currentWindow.setFullscreen(!isFullscreen);
        console.log('✅ Fullscreen toggled: ' + !isFullscreen);
    } catch (error) {
        console.log('Fullscreen error:', error);
    }
}

async function showMessage() {
    try {
        // For Tauri v2, dialog might be in a different location
        // Let's try multiple approaches
        let dialog = window.__TAURI__.dialog;
        if (!dialog) {
            dialog = window.__TAURI__.core?.dialog;
        }
        
        if (dialog && typeof dialog.message === 'function') {
            await dialog.message('Hello from Tauri Desktop App!', {
                title: 'My Tauri App',
                type: 'info'
            });
        } else {
            // Fallback to alert
            alert('Hello from Tauri Desktop App!\n\nUsing: ' + window.location.href);
        }
        console.log('✅ Message shown successfully');
    } catch (error) {
        console.log('Dialog error:', error);
        // Fallback to alert
        alert('Hello from Tauri Desktop App!\n\nDialog API not available');
    }
}

async function closeWindow() {
    try {
        if (!currentWindow) {
            currentWindow = window.__TAURI__.window.getCurrentWindow();
        }
        
        if (confirm('Are you sure you want to close the app?')) {
            await currentWindow.close();
        }
    } catch (error) {
        console.log('Close error:', error);
    }
}

async function getWindowInfo() {
    try {
        if (!currentWindow) {
            currentWindow = window.__TAURI__.window.getCurrentWindow();
        }
        
        const title = await currentWindow.title();
        const isMaximized = await currentWindow.isMaximized();
        const isFullscreen = await currentWindow.isFullscreen();
        const isVisible = await currentWindow.isVisible();
        const isDecorated = await currentWindow.isDecorated();
        const isResizable = await currentWindow.isResizable();
        
        alert(`📋 Window Information:
                  Title: ${title}
                  Maximized: ${isMaximized}
                  Fullscreen: ${isFullscreen}
                  Visible: ${isVisible}
                  Decorated: ${isDecorated}
                  Resizable: ${isResizable}`);
    } catch (error) {
        console.log('Window info error:', error);
    }
}

// Additional useful functions
async function centerWindow() {
    try {
        if (!currentWindow) {
            currentWindow = window.__TAURI__.window.getCurrentWindow();
        }
        await currentWindow.center();
    } catch (error) {
        console.log('Center error:', error);
    }
}

async function setAlwaysOnTop() {
    try {
        if (!currentWindow) {
            currentWindow = window.__TAURI__.window.getCurrentWindow();
        }
        const alwaysOnTop = await currentWindow.isAlwaysOnTop();
        await currentWindow.setAlwaysOnTop(!alwaysOnTop);
    } catch (error) {
        console.log('Always on top error:', error);
    }
}



// Initialize when page loads
document.addEventListener('DOMContentLoaded', initApp);