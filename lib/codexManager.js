const fs = require('fs');
const path = require('path');
const { exec, spawn } = require('child_process');
const util = require('util');
const execPromise = util.promisify(exec);

// Path to ~/.codex directory and auth.json
function getCodexDir() {
  const home = process.env.USERPROFILE || process.env.HOME;
  return path.join(home, '.codex');
}

function getAuthJsonPath() {
  return path.join(getCodexDir(), 'auth.json');
}

// Decode JWT without external dependency
function parseJwt(token) {
  if (!token || typeof token !== 'string') return null;
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  try {
    const payload = Buffer.from(parts[1], 'base64').toString('utf8');
    return JSON.parse(payload);
  } catch (e) {
    return null;
  }
}

// Extract human-readable details from auth.json
function parseAuthInfo(authData) {
  if (!authData) return null;
  
  let info = {
    authMode: authData.auth_mode || (authData.OPENAI_API_KEY ? 'api_key' : 'chatgpt'),
    email: null,
    plan: null,
    accountId: null,
    expiresAt: null,
    hasTokens: false,
    hasApiKey: !!authData.OPENAI_API_KEY,
    lastRefresh: authData.last_refresh || null,
    tokens: authData.tokens || null,
    raw: authData
  };

  if (authData.tokens) {
    info.hasTokens = true;
    info.tokens = authData.tokens;
    info.accountId = authData.tokens.account_id || null;
    
    if (authData.tokens.id_token) {
      const claims = parseJwt(authData.tokens.id_token);
      if (claims) {
        info.email = claims.email || null;
        const authClaims = claims['https://api.openai.com/auth'] || {};
        info.plan = authClaims.chatgpt_plan_type || info.plan;
        info.accountId = authClaims.chatgpt_account_id || info.accountId;
        if (authClaims.chatgpt_subscription_active_until) {
          info.expiresAt = authClaims.chatgpt_subscription_active_until;
        }
      }
    }
  }

  return info;
}

// Read current active account from ~/.codex/auth.json
function getCurrentActiveAccount() {
  const authPath = getAuthJsonPath();
  if (!fs.existsSync(authPath)) {
    return null;
  }

  try {
    const raw = fs.readFileSync(authPath, 'utf8');
    const authData = JSON.parse(raw);
    return parseAuthInfo(authData);
  } catch (err) {
    console.error('Error reading ~/.codex/auth.json:', err.message);
    return { error: err.message };
  }
}

// Detect if user-interactive ChatGPT / Codex desktop processes are running
const IGNORED_PROCESSES = ['codex-windows-sandbox-service', 'codexswitcher', 'service'];

function isTargetCodexProcess(procName) {
  if (!procName) return false;
  const name = procName.toLowerCase();
  for (const ign of IGNORED_PROCESSES) {
    if (name.includes(ign)) return false;
  }
  return name === 'chatgpt.exe' || name === 'chatgpt' ||
         name === 'codex.exe' || name === 'codex' ||
         name.startsWith('codex-code-mode') || name.startsWith('codex-computer');
}

async function isCodexRunning() {
  try {
    const { stdout } = await execPromise('tasklist /FO CSV /NH');
    const lines = stdout.split('\r\n');
    const running = [];

    for (const line of lines) {
      if (!line.trim()) continue;
      const match = line.match(/^"([^"]+)","([^"]+)"/);
      if (match) {
        const procName = match[1];
        const pid = match[2];
        if (isTargetCodexProcess(procName)) {
          running.push({ name: procName, pid });
        }
      }
    }

    return {
      running: running.length > 0,
      processes: running
    };
  } catch (err) {
    console.error('Error checking running processes:', err.message);
    return { running: false, processes: [], error: err.message };
  }
}

// Gracefully terminate interactive ChatGPT / Codex processes
async function stopCodex() {
  console.log('Terminating interactive ChatGPT and Codex processes...');
  try {
    await execPromise('taskkill /F /IM ChatGPT.exe /T');
  } catch (e) {}
  try {
    await execPromise('taskkill /F /IM codex.exe /T');
  } catch (e) {}
  try {
    await execPromise('taskkill /F /IM codex-code-mode-host.exe /T');
  } catch (e) {}
  try {
    await execPromise('taskkill /F /IM codex-computer-use-swift.exe /T');
  } catch (e) {}

  // Wait 600ms
  await new Promise(r => setTimeout(r, 600));

  return {
    success: true
  };
}

// Backup current auth.json
function backupCurrentAuth() {
  const authPath = getAuthJsonPath();
  if (!fs.existsSync(authPath)) return null;

  try {
    const backupDir = path.join(getCodexDir(), 'backups');
    if (!fs.existsSync(backupDir)) {
      fs.mkdirSync(backupDir, { recursive: true });
    }

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupFile = path.join(backupDir, `auth.json.bak.${timestamp}`);
    fs.copyFileSync(authPath, backupFile);
    console.log(`Backed up auth.json to ${backupFile}`);
    return backupFile;
  } catch (err) {
    console.error('Failed to create backup of auth.json:', err.message);
    return null;
  }
}

// Write new account credentials to ~/.codex/auth.json
function writeAuthJson(accountData) {
  const codexDir = getCodexDir();
  if (!fs.existsSync(codexDir)) {
    fs.mkdirSync(codexDir, { recursive: true });
  }

  const authPath = getAuthJsonPath();
  
  // Backup before writing
  const backupPath = backupCurrentAuth();

  let targetContent;

  // If already structured as AuthDotJson
  if (accountData.tokens || accountData.OPENAI_API_KEY) {
    targetContent = {
      auth_mode: accountData.auth_mode || (accountData.OPENAI_API_KEY ? 'api_key' : 'chatgpt'),
      OPENAI_API_KEY: accountData.OPENAI_API_KEY || null,
      tokens: accountData.tokens || null,
      last_refresh: new Date().toISOString()
    };
  } else if (accountData.authData) {
    // Nested inside authData
    targetContent = {
      auth_mode: accountData.authData.auth_mode || 'chatgpt',
      OPENAI_API_KEY: accountData.authData.OPENAI_API_KEY || null,
      tokens: accountData.authData.tokens || null,
      last_refresh: new Date().toISOString()
    };
  } else {
    // Custom flat format
    targetContent = {
      auth_mode: accountData.authMode || 'chatgpt',
      OPENAI_API_KEY: accountData.apiKey || null,
      tokens: {
        id_token: accountData.id_token || accountData.idToken || '',
        access_token: accountData.access_token || accountData.accessToken || '',
        refresh_token: accountData.refresh_token || accountData.refreshToken || '',
        account_id: accountData.account_id || accountData.accountId || null
      },
      last_refresh: new Date().toISOString()
    };
  }

  // Write atomically: write to temp file then rename
  const tmpPath = `${authPath}.tmp-${Date.now()}`;
  fs.writeFileSync(tmpPath, JSON.stringify(targetContent, null, 2), 'utf8');
  fs.renameSync(tmpPath, authPath);

  console.log('Successfully wrote new auth.json');
  return { success: true, backupPath, updated: parseAuthInfo(targetContent) };
}

// Start ChatGPT Desktop application
function startCodex() {
  console.log('Starting ChatGPT Desktop App (OpenAI.Codex_2p2nqsd0c76g0!App)...');
  try {
    // explorer.exe shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App launches the Windows Store app cleanly
    const child = spawn('explorer.exe', ['shell:AppsFolder\\OpenAI.Codex_2p2nqsd0c76g0!App'], {
      detached: true,
      stdio: 'ignore'
    });
    child.unref();

    return { success: true, message: 'ChatGPT Desktop app launched.' };
  } catch (err) {
    console.error('Error starting ChatGPT app:', err.message);
    return { success: false, error: err.message };
  }
}

// Complete Account Switch Workflow:
// 1. Check if running -> if so, kill ChatGPT and Codex
// 2. Write new auth.json
// 3. Restart ChatGPT app
async function performSwitch(account) {
  const steps = [];

  // Step 1: Check if running
  const runningStatus = await isCodexRunning();
  let wasRunning = runningStatus.running;

  if (wasRunning) {
    steps.push(`ChatGPT/Codex is currently running (${runningStatus.processes.length} process(es)). Closing...`);
    await stopCodex();
    steps.push('ChatGPT closed successfully.');
  } else {
    steps.push('ChatGPT was not running.');
  }

  // Step 2: Write auth.json
  steps.push('Backing up current credentials and writing new auth.json...');
  const writeRes = writeAuthJson(account);
  steps.push('auth.json successfully updated.');

  // Step 3: Re-launch ChatGPT desktop app
  steps.push('Restarting ChatGPT Desktop app...');
  const startRes = startCodex();
  if (startRes.success) {
    steps.push('ChatGPT Desktop app restarted successfully.');
  } else {
    steps.push(`Notice: Could not auto-launch ChatGPT (${startRes.error}). You can launch it manually.`);
  }

  // Step 4: Verify current active
  const activeNow = getCurrentActiveAccount();

  return {
    success: true,
    wasRunning,
    steps,
    backupPath: writeRes.backupPath,
    activeAccount: activeNow
  };
}

module.exports = {
  getCodexDir,
  getAuthJsonPath,
  parseJwt,
  parseAuthInfo,
  getCurrentActiveAccount,
  isCodexRunning,
  stopCodex,
  writeAuthJson,
  startCodex,
  performSwitch
};
