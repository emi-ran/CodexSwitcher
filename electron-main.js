const { app, BrowserWindow, Menu, Tray, Notification, shell, nativeImage } = require('electron');
const path = require('path');
const fs = require('fs');

// 1. Mutex / Single Instance Lock (prevent running more than one instance)
const gotTheLock = app.requestSingleInstanceLock();
if (!gotTheLock) {
  console.log('[App] Another instance of Codex Switcher is already running. Focusing existing instance.');
  app.quit();
  process.exit(0);
}

let mainWindow = null;
let tray = null;
let isQuitting = false;
let hasShownCloseNotice = false;

// 2. Auto-Start with Windows Bridge
global.getAutoStart = () => {
  return app.getLoginItemSettings().openAtLogin;
};

global.setAutoStart = (enable) => {
  app.setLoginItemSettings({
    openAtLogin: !!enable,
    path: process.execPath,
    args: ['--hidden']
  });
  updateTrayMenu();
};

function getTrayIcon() {
  const iconPath = path.join(__dirname, 'assets', 'icon.png');
  if (fs.existsSync(iconPath)) {
    const img = nativeImage.createFromPath(iconPath);
    return img.resize({ width: 24, height: 24, quality: 'best' });
  }
  return nativeImage.createEmpty();
}

function updateTrayMenu() {
  if (!tray) return;

  const isAutoStart = global.getAutoStart();
  const contextMenu = Menu.buildFromTemplate([
    {
      label: 'Codex Switcher\'ı Aç',
      click: () => {
        showMainWindow();
      }
    },
    { type: 'separator' },
    {
      label: 'Windows ile Başlat',
      type: 'checkbox',
      checked: isAutoStart,
      click: (menuItem) => {
        global.setAutoStart(menuItem.checked);
      }
    },
    {
      label: 'ChatGPT\'yi Yeniden Başlat',
      click: async () => {
        try {
          const { stopCodex, startCodex } = require('./lib/codexManager');
          await stopCodex();
          setTimeout(() => startCodex(), 600);
        } catch (e) {
          console.error('[Tray] Failed to restart ChatGPT:', e.message);
        }
      }
    },
    { type: 'separator' },
    {
      label: 'Tamamen Kapat (Çıkış)',
      click: () => {
        isQuitting = true;
        app.quit();
      }
    }
  ]);

  tray.setContextMenu(contextMenu);
}

function createTray() {
  const icon = getTrayIcon();
  tray = new Tray(icon);
  tray.setToolTip('Codex Switcher');

  updateTrayMenu();

  tray.on('click', () => {
    showMainWindow();
  });

  tray.on('double-click', () => {
    showMainWindow();
  });
}

function showMainWindow() {
  if (!mainWindow) return;
  if (mainWindow.isMinimized()) mainWindow.restore();
  if (!mainWindow.isVisible()) mainWindow.show();
  mainWindow.focus();
}

function notifyMinimizedToTray() {
  if (hasShownCloseNotice) return;
  hasShownCloseNotice = true;

  const title = 'Codex Switcher';
  const message = 'Uygulama arka planda sistem tepsisinde (tray) çalışmaya devam ediyor.\nTamamen kapatmak için tepsi simgesine sağ tıklayıp "Tamamen Kapat" seçebilirsiniz.';

  try {
    if (Notification.isSupported()) {
      const notif = new Notification({
        title,
        body: message,
        icon: path.join(__dirname, 'assets', 'icon.png')
      });
      notif.on('click', () => showMainWindow());
      notif.show();
    } else if (tray && typeof tray.displayBalloon === 'function') {
      tray.displayBalloon({
        title,
        content: message,
        iconType: 'info'
      });
    }
  } catch (e) {}
}

async function createMainWindow(port) {
  const appIconPath = path.join(__dirname, 'assets', 'icon.png');

  mainWindow = new BrowserWindow({
    width: 1220,
    height: 840,
    minWidth: 920,
    minHeight: 620,
    title: 'Codex Switcher',
    icon: fs.existsSync(appIconPath) ? appIconPath : undefined,
    backgroundColor: '#0c0e14',
    show: false,
    autoHideMenuBar: true,
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true
    }
  });

  Menu.setApplicationMenu(null);

  mainWindow.loadURL(`http://localhost:${port}`);

  // Check if launched with --hidden flag (e.g. from Windows auto-start)
  const isHiddenLaunch = process.argv.includes('--hidden');

  mainWindow.once('ready-to-show', () => {
    if (!isHiddenLaunch) {
      mainWindow.show();
    }
  });

  // External links open in default web browser
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('http:') || url.startsWith('https:')) {
      shell.openExternal(url);
    }
    return { action: 'deny' };
  });

  // Intercept window close (minimize to tray with notification)
  mainWindow.on('close', (event) => {
    if (!isQuitting) {
      event.preventDefault();
      mainWindow.hide();
      notifyMinimizedToTray();
    }
  });

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

// When a second instance tries to launch, focus this window
app.on('second-instance', () => {
  showMainWindow();
});

app.on('before-quit', () => {
  isQuitting = true;
});

app.whenReady().then(async () => {
  try {
    // 1. Create System Tray
    createTray();

    // 2. Start Backend Server on available port
    const { startServer } = require('./server.js');
    const { port } = await startServer();

    // 3. Create Desktop Window
    await createMainWindow(port);

    app.on('activate', () => {
      if (mainWindow) {
        showMainWindow();
      } else {
        createMainWindow(port);
      }
    });
  } catch (err) {
    console.error('Fatal initialization error in Electron:', err);
    app.quit();
  }
});

app.on('window-all-closed', () => {
  // Keep running in tray even when window is closed unless isQuitting is true
  if (isQuitting && process.platform !== 'darwin') {
    app.quit();
    process.exit(0);
  }
});
