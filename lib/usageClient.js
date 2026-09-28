const CLIENT_ID = 'app_EMoamEEZ73f0CkXaXp7hrann';
const AUTH_ISSUER = 'https://auth.openai.com';
const WHAM_USAGE_URL = 'https://chatgpt.com/backend-api/wham/usage';
const USER_AGENT = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36';

// Cache usage for 30s to prevent spamming
const usageCache = new Map();
const CACHE_TTL_MS = 30 * 1000;

// Refresh ChatGPT OAuth token if expired
async function refreshOAuthToken(refreshToken) {
  if (!refreshToken) throw new Error('No refresh token provided.');

  const body = new URLSearchParams({
    grant_type: 'refresh_token',
    client_id: CLIENT_ID,
    refresh_token: refreshToken
  }).toString();

  const response = await fetch(`${AUTH_ISSUER}/oauth/token`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
      'User-Agent': USER_AGENT
    },
    body
  });

  if (!response.ok) {
    const errText = await response.text();
    throw new Error(`Token refresh failed (Status ${response.status}): ${errText}`);
  }

  return await response.json();
}

// Fetch usage for a single account
async function getAccountUsage(account) {
  if (!account || !account.tokens) return null;

  const cacheKey = account.accountId || account.email || account.id;
  const cached = usageCache.get(cacheKey);
  if (cached && (Date.now() - cached.timestamp < CACHE_TTL_MS)) {
    return cached.data;
  }

  let accessToken = account.tokens.access_token;
  const accountId = account.accountId || account.tokens.account_id;
  const refreshToken = account.tokens.refresh_token;

  let res = await sendUsageRequest(accessToken, accountId);

  // If 401 Unauthorized and we have a refreshToken, try refresh once
  if (res.status === 401 && refreshToken) {
    try {
      console.log(`[Usage] Token expired for ${account.email}, refreshing...`);
      const newTokens = await refreshOAuthToken(refreshToken);
      accessToken = newTokens.access_token;
      if (newTokens.access_token) {
        account.tokens.access_token = newTokens.access_token;
      }
      if (newTokens.refresh_token) {
        account.tokens.refresh_token = newTokens.refresh_token;
      }
      if (newTokens.id_token) {
        account.tokens.id_token = newTokens.id_token;
      }

      res = await sendUsageRequest(accessToken, accountId);
    } catch (refreshErr) {
      console.error(`[Usage] Failed to refresh token for ${account.email}:`, refreshErr.message);
    }
  }

  if (!res.ok) {
    const errText = await res.text();
    console.warn(`[Usage] Could not fetch usage for ${account.email} (Status ${res.status}): ${errText.slice(0, 100)}`);
    return {
      error: `Status ${res.status}`
    };
  }

  const json = await res.json();
  const rateLimit = json.rate_limit || {};
  const primary = rateLimit.primary_window || null;
  const secondary = rateLimit.secondary_window || null;

  const usageData = {
    primary: primary ? {
      usedPercent: Math.round(primary.used_percent || 0),
      limitSeconds: primary.limit_window_seconds || 18000,
      resetAfterSeconds: primary.reset_after_seconds || 0,
      resetAt: primary.reset_at || null
    } : null,
    secondary: secondary ? {
      usedPercent: Math.round(secondary.used_percent || 0),
      limitSeconds: secondary.limit_window_seconds || 604800,
      resetAfterSeconds: secondary.reset_after_seconds || 0,
      resetAt: secondary.reset_at || null
    } : null,
    limitReached: !!rateLimit.limit_reached,
    resetCredits: json.rate_limit_reset_credits ? json.rate_limit_reset_credits.available_count : 0
  };

  usageCache.set(cacheKey, {
    timestamp: Date.now(),
    data: usageData
  });

  return usageData;
}

// Send HTTP request with Chrome browser headers
async function sendUsageRequest(accessToken, accountId) {
  const headers = {
    'Authorization': `Bearer ${accessToken}`,
    'User-Agent': USER_AGENT,
    'Origin': 'https://chatgpt.com',
    'Referer': 'https://chatgpt.com',
    'Accept': 'application/json, text/plain, */*'
  };

  if (accountId) {
    headers['chatgpt-account-id'] = accountId;
  }

  return await fetch(WHAM_USAGE_URL, { headers });
}

// Fetch usage for multiple accounts in parallel
async function enrichAccountsWithUsage(accounts) {
  if (!Array.isArray(accounts)) return accounts;

  const results = await Promise.allSettled(
    accounts.map(async (acc) => {
      try {
        const usage = await getAccountUsage(acc);
        return { ...acc, usage };
      } catch (err) {
        return { ...acc, usage: { error: err.message } };
      }
    })
  );

  return results.map((r, i) => r.status === 'fulfilled' ? r.value : accounts[i]);
}

module.exports = {
  getAccountUsage,
  enrichAccountsWithUsage,
  refreshOAuthToken
};
