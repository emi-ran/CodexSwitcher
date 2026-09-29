const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
let launchDesktopPreference = true;

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
  return path.join(getConfigDir(), 'switcher_config.dat');
}

function getAccountsCachePath() {
  return path.join(getConfigDir(), 'switcher_accounts.dat');
}

// Derive a hardware/user-bound key so only this app on this user machine can decrypt
function getVaultKey() {
  const machineEntropy = [
    process.env.USERPROFILE || '',
    process.env.COMPUTERNAME || '',
    process.env.USERNAME || '',
    'CodexSwitcher_Vault_Key_9b2e7c4f1a'
  ].join('##');

  return crypto.scryptSync(machineEntropy, 'codex_vault_salt_secure_2026', 32);
}

/**
 * Encrypts data to binary buffer using AES-256-GCM
 * Format: [Magic: 4 bytes ("CDXV")] + [IV: 12 bytes] + [AuthTag: 16 bytes] + [Ciphertext]
 */
function encryptData(obj) {
  const key = getVaultKey();
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);

  const jsonStr = JSON.stringify(obj);
  let encrypted = cipher.update(jsonStr, 'utf8');
  encrypted = Buffer.concat([encrypted, cipher.final()]);

  const authTag = cipher.getAuthTag();
  const magic = Buffer.from('CDXV', 'ascii');

  return Buffer.concat([magic, iv, authTag, encrypted]);
}

/**
 * Decrypts binary buffer using AES-256-GCM
 */
function decryptData(buffer) {
  if (!buffer || buffer.length < 36) return null;

  const magic = buffer.subarray(0, 4).toString('ascii');
  if (magic !== 'CDXV') {
    // If not encrypted with magic, check if legacy plain text JSON
    try {
      return JSON.parse(buffer.toString('utf8'));
    } catch (e) {
      return null;
    }
  }

  const iv = buffer.subarray(4, 16);
  const authTag = buffer.subarray(16, 32);
  const encrypted = buffer.subarray(32);

  const key = getVaultKey();
  const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
  decipher.setAuthTag(authTag);

  let decrypted = decipher.update(encrypted, undefined, 'utf8');
  decrypted += decipher.final('utf8');

  return JSON.parse(decrypted);
}

/**
 * Safely writes buffer to disk atomically
 */
function safeWriteBinary(targetPath, buffer) {
  const tmp = `${targetPath}.tmp-${Date.now()}`;
  fs.writeFileSync(tmp, buffer);
  try {
    fs.renameSync(tmp, targetPath);
  } catch (err) {
    fs.copyFileSync(tmp, targetPath);
    try { fs.unlinkSync(tmp); } catch (e) {}
  }
}

/**
 * Loads configuration from ~/.codex/switcher_config.dat
 * Automatically migrates from legacy .json if present and removes plain text .json
 */
function loadConfig() {
  const datPath = getConfigPath();
  const legacyJsonPath = path.join(getConfigDir(), 'switcher_config.json');

  let fileConfig = {};

  if (fs.existsSync(datPath)) {
    try {
      const raw = fs.readFileSync(datPath);
      fileConfig = decryptData(raw) || {};
    } catch (err) {
      console.warn('[Config] Error decrypting switcher_config.dat:', err.message);
    }
  } else if (fs.existsSync(legacyJsonPath)) {
    // Migrate legacy plain JSON to encrypted .dat
    try {
      const rawJson = fs.readFileSync(legacyJsonPath, 'utf8');
      fileConfig = JSON.parse(rawJson) || {};
      if (fs.existsSync(legacyJsonPath)) {
        try { fs.unlinkSync(legacyJsonPath); } catch (e) {}
      }
      console.log('[Config] Migrated legacy JSON to encrypted switcher_config.dat and deleted plain JSON.');
    } catch (e) {
      console.warn('[Config] Legacy migration error:', e.message);
    }
  }

  const routerUrl = fileConfig.routerUrl || process.env.ROUTER_URL || '';
  const password = fileConfig.password || process.env.ROUTER_PASSWORD || '';
  const port = fileConfig.port || parseInt(process.env.PORT, 10) || 3210;
  const launchDesktopAfterSwitch = fileConfig.launchDesktopAfterSwitch !== false;
  launchDesktopPreference = launchDesktopAfterSwitch;

  process.env.ROUTER_URL = routerUrl;
  process.env.ROUTER_PASSWORD = password;
  process.env.PORT = String(port);

  // If no encrypted dat exists yet but we have credentials, save them encrypted
  if (!fs.existsSync(datPath) && password) {
    saveConfig({ routerUrl, password, port });
  }

  return {
    routerUrl,
    password,
    port,
    launchDesktopAfterSwitch,
    hasPassword: !!password && password.trim().length > 0
  };
}

/**
 * Saves configuration encrypted to ~/.codex/switcher_config.dat
 */
function saveConfig({ routerUrl, password, port, launchDesktopAfterSwitch }) {
  const current = {
    routerUrl: process.env.ROUTER_URL || '',
    password: process.env.ROUTER_PASSWORD || '',
    port: parseInt(process.env.PORT, 10) || 3210,
    launchDesktopAfterSwitch: launchDesktopPreference
  };

  const updated = {
    routerUrl: routerUrl !== undefined ? routerUrl : current.routerUrl,
    password: password !== undefined ? password : current.password,
    port: port !== undefined ? port : current.port,
    launchDesktopAfterSwitch: launchDesktopAfterSwitch !== undefined ? !!launchDesktopAfterSwitch : current.launchDesktopAfterSwitch,
    updatedAt: new Date().toISOString()
  };

  const encryptedBuffer = encryptData(updated);
  safeWriteBinary(getConfigPath(), encryptedBuffer);

  process.env.ROUTER_URL = updated.routerUrl;
  process.env.ROUTER_PASSWORD = updated.password;
  launchDesktopPreference = updated.launchDesktopAfterSwitch;
  if (updated.port) process.env.PORT = String(updated.port);

  // If old plain json exists, delete it
  const legacyJson = path.join(getConfigDir(), 'switcher_config.json');
  if (fs.existsSync(legacyJson)) {
    try { fs.unlinkSync(legacyJson); } catch (e) {}
  }

  return updated;
}

/**
 * Loads accounts cache from ~/.codex/switcher_accounts.dat
 * Automatically migrates from legacy .json if present
 */
function loadAccountsCache() {
  const datPath = getAccountsCachePath();
  const legacyJsonPath = path.join(getConfigDir(), 'switcher_accounts_cache.json');
  const rootCacheJson = path.join(process.cwd(), '.accounts_cache.json');

  if (fs.existsSync(datPath)) {
    try {
      const raw = fs.readFileSync(datPath);
      return decryptData(raw) || [];
    } catch (err) {
      console.warn('[Cache] Error decrypting accounts cache:', err.message);
    }
  }

  // Check legacy unencrypted files
  let legacyData = null;
  if (fs.existsSync(legacyJsonPath)) {
    try {
      legacyData = JSON.parse(fs.readFileSync(legacyJsonPath, 'utf8'));
      fs.unlinkSync(legacyJsonPath);
    } catch (e) {}
  } else if (fs.existsSync(rootCacheJson)) {
    try {
      legacyData = JSON.parse(fs.readFileSync(rootCacheJson, 'utf8'));
      fs.unlinkSync(rootCacheJson);
    } catch (e) {}
  }

  if (legacyData && Array.isArray(legacyData)) {
    saveAccountsCache(legacyData);
    return legacyData;
  }

  return [];
}

/**
 * Saves accounts cache encrypted to ~/.codex/switcher_accounts.dat
 */
function saveAccountsCache(accounts) {
  try {
    const encryptedBuffer = encryptData(accounts || []);
    safeWriteBinary(getAccountsCachePath(), encryptedBuffer);

    // Clean up any remaining plain json files
    const legacy1 = path.join(getConfigDir(), 'switcher_accounts_cache.json');
    if (fs.existsSync(legacy1)) {
      try { fs.unlinkSync(legacy1); } catch (e) {}
    }
    const legacy2 = path.join(process.cwd(), '.accounts_cache.json');
    if (fs.existsSync(legacy2)) {
      try { fs.unlinkSync(legacy2); } catch (e) {}
    }
  } catch (err) {
    console.warn('[Cache] Could not save encrypted accounts cache:', err.message);
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
