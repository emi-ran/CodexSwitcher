const { parseJwt } = require('./codexManager');

const USER_AGENT = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/154.0.0.0 Safari/537.36';

// Format URL without trailing slash
function cleanUrl(url) {
  if (!url) return '';
  return url.trim().replace(/\/+$/, '');
}

// 1. Authenticate with 9Router
async function login9Router(routerUrl, password) {
  const base = cleanUrl(routerUrl);
  if (!base) {
    throw new Error('9Router URL is not configured. Please check your settings or enter the URL.');
  }
  if (!password) {
    throw new Error('9Router password is not provided. Please check your settings or enter your password.');
  }

  const loginEndpoint = `${base}/api/auth/login`;
  console.log(`[9Router] Logging in to ${loginEndpoint}...`);

  let response;
  try {
    response = await fetch(loginEndpoint, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'accept': '*/*',
        'user-agent': USER_AGENT,
        'origin': base,
        'referer': `${base}/login`
      },
      body: JSON.stringify({ password, rememberMe: false })
    });
  } catch (netErr) {
    throw new Error(`Failed to connect to 9Router at ${base}: ${netErr.message}`);
  }

  const rawText = await response.text();
  let json;
  try {
    json = JSON.parse(rawText);
  } catch (e) {
    throw new Error(`Invalid response from 9Router login (Status ${response.status}): ${rawText.slice(0, 200)}`);
  }

  if (!response.ok || json.error) {
    const errorMsg = json.error || `HTTP ${response.status}: ${response.statusText}`;
    const remaining = json.remainingBeforeLock ? ` (${json.remainingBeforeLock} attempt(s) remaining)` : '';
    throw new Error(`9Router Login Failed: ${errorMsg}${remaining}`);
  }

  // Extract cookies from Set-Cookie headers
  let setCookies = [];
  if (response.headers.getSetCookie) {
    setCookies = response.headers.getSetCookie();
  } else if (response.headers.get('set-cookie')) {
    setCookies = [response.headers.get('set-cookie')];
  }

  const cookiesObj = {};
  let cookieHeader = '';

  for (const cookieStr of setCookies) {
    if (!cookieStr) continue;
    const parts = cookieStr.split(';')[0].split('=');
    if (parts.length >= 2) {
      const name = parts[0].trim();
      const value = parts.slice(1).join('=').trim();
      cookiesObj[name] = value;
      cookieHeader += `${name}=${value}; `;
    }
  }

  console.log('[9Router] Login successful. Cookies captured:', Object.keys(cookiesObj));

  return {
    success: true,
    cookies: cookiesObj,
    cookieHeader: cookieHeader.trim(),
    data: json
  };
}

// 2. Fetch Database from 9Router
async function fetch9RouterDatabase(routerUrl, password, cookieHeader) {
  const base = cleanUrl(routerUrl);
  const dbEndpoint = `${base}/api/settings/database`;
  console.log(`[9Router] Fetching database from ${dbEndpoint}...`);

  let response;
  try {
    response = await fetch(dbEndpoint, {
      method: 'GET',
      headers: {
        'accept': '*/*',
        'cookie': cookieHeader,
        'x-9r-password': password,
        'user-agent': USER_AGENT,
        'origin': base,
        'referer': `${base}/dashboard/profile`
      }
    });
  } catch (netErr) {
    throw new Error(`Failed to fetch database: ${netErr.message}`);
  }

  const rawText = await response.text();
  let json;
  try {
    json = JSON.parse(rawText);
  } catch (e) {
    throw new Error(`Failed to parse 9Router database response (Status ${response.status}): ${rawText.slice(0, 200)}`);
  }

  if (!response.ok || json.error) {
    throw new Error(`9Router Database Error (Status ${response.status}): ${json.error || rawText}`);
  }

  return json;
}

// 3. Extract Codex Accounts Strictly from Database Dump
function extractCodexProviders(dbData) {
  const accounts = [];
  if (!dbData || typeof dbData !== 'object') return accounts;

  // In 9Router, providers are stored in `providerConnections` table
  const connections = dbData.providerConnections || [];

  for (const c of connections) {
    // Check if this provider connection is strictly codex
    const isCodex = (c.provider && c.provider.toLowerCase() === 'codex') ||
                    (c.authType === 'oauth' && c.providerSpecificData && c.providerSpecificData.chatgptAccountId);

    if (!isCodex) {
      continue; // Skip Gemini, Groq, Ollama, Nvidia, Antigravity, etc.
    }

    const psd = c.providerSpecificData || {};
    let idToken = c.idToken || c.id_token || (c.tokens && c.tokens.id_token) || '';
    let accessToken = c.accessToken || c.access_token || (c.tokens && c.tokens.access_token) || '';
    let refreshToken = c.refreshToken || c.refresh_token || (c.tokens && c.tokens.refresh_token) || '';
    let accountId = psd.chatgptAccountId || c.accountId || c.account_id || (c.tokens && c.tokens.account_id) || null;
    let plan = psd.chatgptPlanType || c.plan || 'plus';
    let email = c.email || c.name || null;
    let expiresAt = psd.chatgptSubscriptionActiveUntil || null;

    // Decode idToken claims if available for more details
    if (idToken) {
      const claims = parseJwt(idToken);
      if (claims) {
        email = claims.email || email;
        const authClaims = claims['https://api.openai.com/auth'] || {};
        plan = authClaims.chatgpt_plan_type || plan;
        accountId = authClaims.chatgpt_account_id || accountId;
        if (authClaims.chatgpt_subscription_active_until) {
          expiresAt = authClaims.chatgpt_subscription_active_until;
        }
      }
    }

    const displayName = c.name || email || `Codex Account (${accountId ? accountId.slice(-8) : c.id.slice(0, 8)})`;

    accounts.push({
      id: c.id,
      name: displayName,
      email: email || 'No email associated',
      plan: plan.toLowerCase(),
      accountId,
      authMode: 'chatgpt',
      expiresAt,
      tokens: {
        id_token: idToken,
        access_token: accessToken,
        refresh_token: refreshToken,
        account_id: accountId
      },
      apiKey: null,
      isActive: false,
      providerName: '9Router Codex'
    });
  }

  console.log(`[9Router] Extracted ${accounts.length} actual Codex account(s) from database.`);
  return accounts;
}

// 4. Correlate with currently active account in ~/.codex/auth.json
function markActiveAccount(accounts, currentActive) {
  if (!currentActive) return accounts;

  return accounts.map(acc => {
    let isActive = false;

    // 1. Match by accountId
    if (currentActive.accountId && acc.accountId && currentActive.accountId === acc.accountId) {
      isActive = true;
    }
    // 2. Match by email
    else if (currentActive.email && acc.email && currentActive.email.toLowerCase() === acc.email.toLowerCase()) {
      isActive = true;
    }
    // 3. Match by refresh_token or access_token
    else if (acc.tokens && currentActive.raw?.tokens) {
      if (acc.tokens.refresh_token && acc.tokens.refresh_token === currentActive.raw.tokens.refresh_token) {
        isActive = true;
      } else if (acc.tokens.access_token && acc.tokens.access_token === currentActive.raw.tokens.access_token) {
        isActive = true;
      }
    }

    return {
      ...acc,
      isActive
    };
  });
}

module.exports = {
  cleanUrl,
  login9Router,
  fetch9RouterDatabase,
  extractCodexProviders,
  markActiveAccount
};
