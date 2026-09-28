const path = require('path');
const fs = require('fs');

// Determine execution environment (standalone .exe vs dev script vs Electron)
const isElectron = !!process.versions.electron;
const isExecutable = process.execPath.toLowerCase().endsWith('codexswitcher.exe');
const appDir = isExecutable && !isElectron ? path.dirname(process.execPath) : __dirname;

// Try loading .env if present (backward compatibility)
try {
  const envPath = path.join(appDir, '.env');
  if (fs.existsSync(envPath)) {
    require('dotenv').config({ path: envPath });
  }
} catch (e) {}

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

// Load persistent configuration from ~/.codex/switcher_config.json
const initialConfig = loadConfig();
const PORT = initialConfig.port || process.env.PORT || 3210;

app.use(express.json({ limit: '10mb' }));

// Static asset folders (supports standalone executable and normal node)
const publicDir = fs.existsSync(path.join(appDir, 'public')) ? path.join(appDir, 'public') : path.join(__dirname, 'public');
const assetsDir = fs.existsSync(path.join(appDir, 'assets')) ? path.join(appDir, 'assets') : path.join(__dirname, 'assets');

app.use(express.static(publicDir));
app.use('/assets', express.static(assetsDir));

// Cache in memory for currently fetched accounts from 9Router (persisted in ~/.codex/switcher_accounts_cache.json)
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
        hasPassword: config.hasPassword
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
    const routerUrl = req.body.routerUrl || config.routerUrl || '';
    const password = req.body.password || config.password;

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
    const switchResult = await performSwitch(targetAccount);

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

// 4. Save Configuration (Persisted in ~/.codex/switcher_config.json)
app.post('/api/config', (req, res) => {
  try {
    const { routerUrl, password } = req.body;
    if (!routerUrl) {
      return res.status(400).json({ success: false, error: 'Router URL is required' });
    }

    const saved = saveConfig({
      routerUrl: cleanUrl(routerUrl),
      password: password !== undefined ? password : ''
    });

    res.json({
      success: true,
      message: 'Configuration saved locally',
      routerConfig: {
        url: saved.routerUrl,
        hasPassword: !!saved.password && saved.password.trim().length > 0
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

// Start listening
app.listen(PORT, () => {
  const url = `http://localhost:${PORT}`;
  const config = loadConfig();
  console.log(`=======================================================`);
  console.log(` Codex Switcher Server running on ${url}`);
  console.log(` Router URL: ${config.routerUrl}`);
  console.log(` Password configured: ${config.hasPassword ? 'Yes' : 'No'}`);
  console.log(` Persistent Config: ${getConfigPath()}`);
  console.log(`=======================================================`);

  // Automatically open default browser when launched as standalone CLI executable or with --open flag (not in Electron)
  if (!isElectron && (isExecutable || process.argv.includes('--open'))) {
    console.log(`[App] Opening ${url} in default browser...`);
    const { exec } = require('child_process');
    exec(`start ${url}`);
  }
});

module.exports = app;
