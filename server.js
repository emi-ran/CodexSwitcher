const path = require('path');
const fs = require('fs');

// Determine execution environment (standalone .exe vs dev script vs Electron)
const isElectron = !!process.versions.electron;
const isExecutable = process.execPath.toLowerCase().endsWith('codexswitcher.exe');
const appDir = isExecutable && !isElectron ? path.dirname(process.execPath) : __dirname;

const express = require('express');

const {
  loadConfig,
  saveConfig,
  loadAccountsCache,
  saveAccountsCache,
  getConfigPath
} = require('./lib/configManager');

const {
  getCurrentActiveAccount,
  isCodexRunning,
  stopCodex,
  startCodex,
  performSwitch
} = require('./lib/codexManager');

const {
  cleanUrl,
  login9Router,
  fetch9RouterDatabase,
  extractCodexProviders,
  markActiveAccount
} = require('./lib/routerClient');

const {
  getAccountUsage,
  enrichAccountsWithUsage
} = require('./lib/usageClient');

const app = express();

// Load persistent configuration from ~/.codex/switcher_config.dat (AES-256-GCM encrypted)
const initialConfig = loadConfig();
const PORT = initialConfig.port || process.env.PORT || 3210;

app.use(express.json({ limit: '10mb' }));

// Static asset folders (supports standalone executable and normal node)
const publicDir = fs.existsSync(path.join(appDir, 'public')) ? path.join(appDir, 'public') : path.join(__dirname, 'public');
const assetsDir = fs.existsSync(path.join(appDir, 'assets')) ? path.join(appDir, 'assets') : path.join(__dirname, 'assets');

app.use(express.static(publicDir));
app.use('/assets', express.static(assetsDir));

// Cache in memory for currently fetched accounts from 9Router (persisted encrypted in ~/.codex/switcher_accounts.dat)
let cachedAccounts = loadAccountsCache();
let cachedRawDb = null;
if (cachedAccounts.length > 0) {
  console.log(`[Cache] Loaded ${cachedAccounts.length} account(s) from persistent cache.`);
}

// 1. Get Status: Router config, Codex processes, and current active account
app.get('/api/status', async (req, res) => {
  try {
    const config = loadConfig();
    const activeAccount = getCurrentActiveAccount();
    if (activeAccount && activeAccount.hasTokens) {
      activeAccount.usage = await getAccountUsage(activeAccount);
    }
    const codexStatus = await isCodexRunning();

    // Mark active in cached accounts so client can display them immediately
    const markedAccounts = markActiveAccount(cachedAccounts, activeAccount);

    res.json({
      success: true,
      routerConfig: {
        url: config.routerUrl,
        hasPassword: config.hasPassword,
        launchDesktopAfterSwitch: config.launchDesktopAfterSwitch
      },
      activeAccount,
      codexStatus,
      accounts: markedAccounts,
      cachedAccountsCount: markedAccounts.length
    });
  } catch (err) {
    res.status(500).json({ success: false, error: err.message });
  }
});

// 2. Sync from 9Router: Login, fetch database, extract Codex accounts
app.post('/api/sync', async (req, res) => {
  try {
    const config = loadConfig();
    const routerUrl = req.body.routerUrl || config.routerUrl;
    const password = req.body.password || config.password;

    if (!routerUrl) {
      return res.status(400).json({
        success: false,
        requiresConfiguration: true,
        error: '9Router URL is required. Please set it in Settings.'
      });
    }

    if (!password) {
      return res.status(400).json({
        success: false,
        requiresPassword: true,
        error: '9Router password is required. Please set it in Settings.'
      });
    }

    // Step 1: Login to 9Router
    const loginRes = await login9Router(routerUrl, password);

    // Step 2: Fetch database
    const db = await fetch9RouterDatabase(routerUrl, password, loginRes.cookieHeader);
    cachedRawDb = db;

    // Step 3: Extract Codex accounts
    const extracted = extractCodexProviders(db);

    // Step 4: Compare with current active account in ~/.codex/auth.json
    const activeNow = getCurrentActiveAccount();
    const markedAccounts = markActiveAccount(extracted, activeNow);

    // Step 5: Enrich with 5h session and weekly usage
    const enrichedAccounts = await enrichAccountsWithUsage(markedAccounts);
    if (activeNow && activeNow.hasTokens) {
      activeNow.usage = await getAccountUsage(activeNow);
    }

    cachedAccounts = enrichedAccounts;
    saveAccountsCache(enrichedAccounts);

    res.json({
      success: true,
      accounts: enrichedAccounts,
      activeAccount: activeNow,
      routerUrl: cleanUrl(routerUrl)
    });
  } catch (err) {
    console.error('Error during /api/sync:', err.message);
    res.status(500).json({
      success: false,
      error: err.message
    });
  }
});

// 3. Switch Account
app.post('/api/switch', async (req, res) => {
  try {
    const { accountId, accountData } = req.body;

    let targetAccount = accountData;
    if (!targetAccount && accountId) {
      targetAccount = cachedAccounts.find(a => a.id === accountId);
    }

    if (!targetAccount) {
      return res.status(400).json({
        success: false,
        error: 'Account data or valid account ID is required to switch.'
      });
    }

    console.log(`[Switch] Switching to account: ${targetAccount.name} (${targetAccount.email})...`);

    // Perform the entire atomic switch workflow:
    // 1. Detect and stop Codex if running
    // 2. Backup & write ~/.codex/auth.json
    // 3. Restart Codex
    const switchResult = await performSwitch(targetAccount, loadConfig().launchDesktopAfterSwitch);

    // Refresh cached accounts active status
    const activeNow = switchResult.activeAccount;
    cachedAccounts = markActiveAccount(cachedAccounts, activeNow);
    saveAccountsCache(cachedAccounts);

    res.json({
      success: true,
      message: `Successfully switched to ${targetAccount.name || targetAccount.email}!`,
      switchResult,
      activeAccount: activeNow,
      accounts: cachedAccounts
    });
  } catch (err) {
    console.error('Switch error:', err.message);
    res.status(500).json({
      success: false,
      error: err.message
    });
  }
});

// 4. Save Configuration (Persisted encrypted in ~/.codex/switcher_config.dat)
app.post('/api/config', (req, res) => {
  try {
    const { routerUrl, password, launchDesktopAfterSwitch } = req.body;
    if (!routerUrl) {
      return res.status(400).json({ success: false, error: 'Router URL is required' });
    }

    const saved = saveConfig({
      routerUrl: cleanUrl(routerUrl),
      password: password !== undefined ? password : '',
      launchDesktopAfterSwitch
    });

    res.json({
      success: true,
      message: 'Configuration saved locally',
      routerConfig: {
        url: saved.routerUrl,
        hasPassword: !!saved.password && saved.password.trim().length > 0,
        launchDesktopAfterSwitch: saved.launchDesktopAfterSwitch
      }
    });
  } catch (err) {
    res.status(500).json({ success: false, error: err.message });
  }
});

// 5. Codex Process Controls
app.post('/api/codex/stop', async (req, res) => {
  try {
    const result = await stopCodex();
    res.json(result);
  } catch (err) {
    res.status(500).json({ success: false, error: err.message });
  }
});

app.post('/api/codex/start', async (req, res) => {
  try {
    const result = startCodex();
    res.json(result);
  } catch (err) {
    res.status(500).json({ success: false, error: err.message });
  }
});

// 6. Debug / Raw Database Inspector
app.get('/api/debug/db-sample', (req, res) => {
  if (!cachedRawDb) {
    return res.status(404).json({ error: 'No database cached yet. Run sync first.' });
  }
  const keys = Object.keys(cachedRawDb);
  res.json({
    keys,
    providersCount: cachedRawDb.providers?.length || 0,
    extractedAccounts: cachedAccounts
  });
});

// 7. Auto-start with Windows (Electron Bridge)
app.get('/api/autostart', (req, res) => {
  const isEnabled = typeof global.getAutoStart === 'function' ? global.getAutoStart() : false;
  res.json({ success: true, enabled: isEnabled });
});

app.post('/api/autostart', (req, res) => {
  const { enabled } = req.body;
  if (typeof global.setAutoStart === 'function') {
    global.setAutoStart(!!enabled);
    return res.json({ success: true, enabled: !!enabled });
  }
  res.json({ success: false, error: 'Auto-start only available in desktop app' });
});

const net = require('net');

function isPortAvailable(port) {
  return new Promise((resolve) => {
    const tester = net.createServer();
    tester.once('error', (err) => {
      resolve(false);
    });
    tester.once('listening', () => {
      tester.close(() => resolve(true));
    });
    tester.listen(port);
  });
}

async function findAvailablePort(startPort = 3210, maxAttempts = 50) {
  for (let p = startPort; p < startPort + maxAttempts; p++) {
    if (await isPortAvailable(p)) {
      return p;
    }
  }
  return startPort;
}

function startServer(preferredPort) {
  return new Promise(async (resolve, reject) => {
    const config = loadConfig();
    const desiredPort = preferredPort || config.port || parseInt(process.env.PORT, 10) || 3210;
    const actualPort = await findAvailablePort(desiredPort);

    const server = app.listen(actualPort, () => {
      const url = `http://localhost:${actualPort}`;
      console.log(`=======================================================`);
      console.log(` Codex Switcher Server running on ${url}`);
      console.log(` Router URL: ${config.routerUrl}`);
      console.log(` Password configured: ${config.hasPassword ? 'Yes' : 'No'}`);
      console.log(` Persistent Config: ${getConfigPath()}`);
      if (actualPort !== desiredPort) {
        console.log(` [Port] Port ${desiredPort} was busy, bound to free port ${actualPort}`);
      }
      console.log(`=======================================================`);

      if (!isElectron && (isExecutable || process.argv.includes('--open'))) {
        console.log(`[App] Opening ${url} in default browser...`);
        const { exec } = require('child_process');
        exec(`start ${url}`);
      }

      resolve({ port: actualPort, server });
    });

    server.on('error', reject);
  });
}

// Auto-start if executed directly (e.g. `node server.js`)
if (require.main === module) {
  startServer();
}

module.exports = {
  app,
  startServer
};
