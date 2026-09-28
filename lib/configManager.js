const fs = require('fs');
const path = require('path');

function getConfigDir() {
  const home = process.env.USERPROFILE || process.env.HOME || '.';
  const dir = path.join(home, '.codex');
  if (!fs.existsSync(dir)) {
    try {
      fs.mkdirSync(dir, { recursive: true });
    } catch (e) {}
  }
  return dir;
}

function getConfigPath() {
  return path.join(getConfigDir(), 'switcher_config.json');
}

function getAccountsCachePath() {
  return path.join(getConfigDir(), 'switcher_accounts_cache.json');
}

/**
 * Loads configuration:
 * 1. Checks ~/.codex/switcher_config.json
 * 2. Fallbacks to process.env (or .env if present)
 * 3. Migrates to ~/.codex/switcher_config.json if not yet saved
 */
function loadConfig() {
  const configPath = getConfigPath();
  let fileConfig = {};

  if (fs.existsSync(configPath)) {
    try {
      const raw = fs.readFileSync(configPath, 'utf8');
      fileConfig = JSON.parse(raw) || {};
    } catch (err) {
      console.warn('[Config] Error reading switcher_config.json:', err.message);
    }
  }

  const routerUrl = fileConfig.routerUrl || process.env.ROUTER_URL || '';
  const password = fileConfig.password || process.env.ROUTER_PASSWORD || '';
  const port = fileConfig.port || parseInt(process.env.PORT, 10) || 3210;

  // Sync to current process.env
  process.env.ROUTER_URL = routerUrl;
  process.env.ROUTER_PASSWORD = password;
  process.env.PORT = String(port);

  // If switcher_config.json didn't exist but we had values, save it now
  if (!fs.existsSync(configPath) && password) {
    try {
      fs.writeFileSync(configPath, JSON.stringify({
        routerUrl,
        password,
        port,
        createdAt: new Date().toISOString()
      }, null, 2), 'utf8');
      console.log(`[Config] Migrated settings to ${configPath}`);
    } catch (e) {}
  }

  return {
    routerUrl,
    password,
    port,
    hasPassword: !!password && password.trim().length > 0
  };
}

/**
 * Saves configuration to ~/.codex/switcher_config.json
 */
function saveConfig({ routerUrl, password, port }) {
  const current = loadConfig();
  const updated = {
    routerUrl: routerUrl !== undefined ? routerUrl : current.routerUrl,
    password: password !== undefined ? password : current.password,
    port: port !== undefined ? port : current.port,
    updatedAt: new Date().toISOString()
  };

  const configPath = getConfigPath();
  fs.writeFileSync(configPath, JSON.stringify(updated, null, 2), 'utf8');

  // Update current process.env
  process.env.ROUTER_URL = updated.routerUrl;
  process.env.ROUTER_PASSWORD = updated.password;
  if (updated.port) process.env.PORT = String(updated.port);

  console.log(`[Config] Configuration saved to ${configPath}`);
  return updated;
}

/**
 * Loads accounts disk cache from ~/.codex/switcher_accounts_cache.json
 */
function loadAccountsCache() {
  const cachePath = getAccountsCachePath();
  try {
    if (fs.existsSync(cachePath)) {
      const raw = fs.readFileSync(cachePath, 'utf8');
      return JSON.parse(raw) || [];
    }
  } catch (err) {
    console.warn('[Cache] Could not read accounts cache:', err.message);
  }
  return [];
}

/**
 * Saves accounts disk cache to ~/.codex/switcher_accounts_cache.json
 */
function saveAccountsCache(accounts) {
  const cachePath = getAccountsCachePath();
  try {
    fs.writeFileSync(cachePath, JSON.stringify(accounts, null, 2), 'utf8');
  } catch (err) {
    console.warn('[Cache] Could not save accounts cache:', err.message);
  }
}

module.exports = {
  getConfigDir,
  getConfigPath,
  getAccountsCachePath,
  loadConfig,
  saveConfig,
  loadAccountsCache,
  saveAccountsCache
};
