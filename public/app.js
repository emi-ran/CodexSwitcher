// Codex Switcher Client Controller

// ============================================================================
// State
// ============================================================================
let appState = {
  activeAccount: null,
  codexStatus: { running: false, processes: [] },
  cliStatus: { running: 0, restartSupported: false },
  routerConfig: { url: '', hasPassword: false },
  accounts: [],
  isSyncing: false,
  isSwitching: false
};

// Dialog and rendering state
let rowsEntryPending = true;
let lastSpotlightKey = null;
let entryTimer = null;
const quotaMemory = new Map();
const dialogOpeners = new Map();

// DOM Elements
const statusDot = document.getElementById('statusDot');
const routerStatusText = document.getElementById('routerStatusText');
const syncBtn = document.getElementById('syncBtn');
const syncBtnText = document.getElementById('syncBtnText');
const openSettingsBtn = document.getElementById('openSettingsBtn');
const langBtnTr = document.getElementById('langBtnTr');
const langBtnEn = document.getElementById('langBtnEn');

const codexStatusPill = document.getElementById('codexStatusPill');
const processDot = document.getElementById('processDot');
const processText = document.getElementById('processText');
const toggleCodexProcessBtn = document.getElementById('toggleCodexProcessBtn');
const processActionIcon = document.getElementById('processActionIcon');
const cliStatusPill = document.getElementById('cliStatusPill');
const cliStatusText = document.getElementById('cliStatusText');
const cliDot = document.getElementById('cliDot');

const spotlightEmail = document.getElementById('spotlightEmail');
const spotlightPlanBadge = document.getElementById('spotlightPlanBadge');
const spotlightAccountId = document.getElementById('spotlightAccountId');
const spotlightLastRefresh = document.getElementById('spotlightLastRefresh');
const restartCodexBtn = document.getElementById('restartCodexBtn');

const accountsCount = document.getElementById('accountsCount');
const searchInput = document.getElementById('searchInput');
const accountsGrid = document.getElementById('accountsGrid');
const emptyState = document.getElementById('emptyState');
const emptyTitle = document.getElementById('emptyTitle');
const emptyDesc = document.getElementById('emptyDesc');
const emptyActionBtn = document.getElementById('emptyActionBtn');
const logMessage = document.getElementById('logMessage');

const settingsModal = document.getElementById('settingsModal');
const settingsForm = document.getElementById('settingsForm');
const settingsRouterUrl = document.getElementById('settingsRouterUrl');
const settingsPassword = document.getElementById('settingsPassword');
const settingsAutoStart = document.getElementById('settingsAutoStart');
const settingsLaunchDesktop = document.getElementById('settingsLaunchDesktop');
const settingsRestartCli = document.getElementById('settingsRestartCli');
const togglePasswordBtn = document.getElementById('togglePasswordBtn');
const closeSettingsModalBtn = document.getElementById('closeSettingsModalBtn');

const switchModal = document.getElementById('switchModal');
const switchTargetLabel = document.getElementById('switchTargetLabel');
const switchStepsList = document.getElementById('switchStepsList');
const switchModalActions = document.getElementById('switchModalActions');
const switchModalCloseBtn = document.getElementById('switchModalCloseBtn');
const toastContainer = document.getElementById('toastContainer');
const spotlightMain = document.getElementById('spotlightMain');
const accountsColHead = document.getElementById('accountsColHead');
const noResultsState = document.getElementById('noResultsState');

function changeLanguage(lang) {
  window.hideAppTooltip?.();
  applyLanguage(lang, () => {
    if (togglePasswordBtn) {
      const isPassword = settingsPassword.type === 'password';
      togglePasswordBtn.textContent = isPassword ? t('showPassword') : t('hidePassword');
    }
    updateSpotlightUI();
    updateCodexProcessUI();
    updateRouterStatusUI(!!appState.routerConfig.url);
    renderAccounts(filterAccounts(searchInput.value));
  });
}

// ============================================================================
// Initialization
// ============================================================================
document.addEventListener('DOMContentLoaded', async () => {
  setupEventListeners();
  changeLanguage(currentLang);

  // Older versions cached account tokens in web storage. The encrypted backend cache replaces it.
  localStorage.removeItem('codex_accounts_cache');

  // 2. Fetch server status (updates active account, server cached accounts & processes)
  await loadStatus();

  // 3. If password exists, trigger sync in background
  if (appState.routerConfig && appState.routerConfig.hasPassword) {
    await triggerSync();
  } else {
    log(t('readyLog'));
  }

  // Smart polling: only runs when window is visible, completely paused in tray
  startPolling();
});

let pollInterval = null;
let processPollInFlight = false;

function startPolling() {
  if (pollInterval || document.hidden) return;
  pollInterval = setInterval(async () => {
    if (document.hidden) {
      stopPolling();
      return;
    }
    await pollProcessStatus();
  }, 4000);
}

function stopPolling() {
  if (pollInterval) {
    clearInterval(pollInterval);
    pollInterval = null;
  }
}

// Pause all network and CPU polling when minimized to tray
document.addEventListener('visibilitychange', () => {
  if (document.hidden) {
    stopPolling();
  } else {
    pollProcessStatus();
    startPolling();
  }
});

window.addEventListener('focus', () => {
  pollProcessStatus();
  startPolling();
});

function setupEventListeners() {
  if (langBtnTr) {
    langBtnTr.addEventListener('click', () => changeLanguage('tr'));
  }
  if (langBtnEn) {
    langBtnEn.addEventListener('click', () => changeLanguage('en'));
  }

  syncBtn.addEventListener('click', () => triggerSync());
  emptyActionBtn.addEventListener('click', () => triggerSync());

  openSettingsBtn.addEventListener('click', () => openSettingsModal());
  closeSettingsModalBtn.addEventListener('click', () => closeModal(settingsModal));

  settingsForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    await saveSettings();
  });

  togglePasswordBtn.addEventListener('click', () => {
    const isPassword = settingsPassword.type === 'password';
    settingsPassword.type = isPassword ? 'text' : 'password';
    togglePasswordBtn.textContent = isPassword ? t('hidePassword') : t('showPassword');
  });

  restartCodexBtn.addEventListener('click', async () => {
    await handleRestartCodex();
  });

  toggleCodexProcessBtn.addEventListener('click', async () => {
    if (appState.codexStatus.running) {
      await handleStopCodex();
    } else {
      await handleStartCodex();
    }
  });

  searchInput.addEventListener('input', (e) => {
    renderAccounts(filterAccounts(e.target.value));
  });

  switchModalCloseBtn.addEventListener('click', () => closeModal(switchModal));

  // Backdrop click closes a dialog; the switch dialog stays open while a switch runs
  [settingsModal, switchModal].forEach((modal) => {
    modal.addEventListener('click', (e) => {
      if (e.target !== modal) return;
      if (modal === switchModal && appState.isSwitching) return;
      closeModal(modal);
    });
  });

  // Tab is trapped inside the open dialog; Escape closes it (not the switch dialog while switching)
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Tab') {
      trapDialogFocus(e);
      return;
    }
    if (e.key === 'Escape') {
      if (settingsModal.classList.contains('open')) {
        closeModal(settingsModal);
      } else if (!appState.isSwitching) {
        closeModal(switchModal);
      }
    }
  });
}

// ============================================================================
// API Calls & Actions
// ============================================================================

async function loadStatus() {
  try {
    const res = await apiFetch('/api/status');
    const data = await res.json();
    if (data.success) {
      appState.routerConfig = data.routerConfig;
      appState.activeAccount = data.activeAccount;
      appState.codexStatus = data.codexStatus;
      appState.cliStatus = data.cliStatus || appState.cliStatus;

      // Immediately render cached accounts from server if available
      if (Array.isArray(data.accounts) && data.accounts.length > 0) {
        appState.accounts = data.accounts;
        renderAccounts(filterAccounts(searchInput.value));
      }

      updateSpotlightUI();
      updateCodexProcessUI();
      updateCliStatusUI();
      updateRouterStatusUI(!!appState.routerConfig.url);
    }
  } catch (err) {
    console.error('Failed to load initial status:', err);
    updateRouterStatusUI(false);
    log(`Status load failed: ${err.message}`);
  }
}

async function pollProcessStatus() {
  if (document.hidden || processPollInFlight) return;
  processPollInFlight = true;
  try {
    const res = await apiFetch('/api/status');
    const data = await res.json();
    if (data.success) {
      appState.codexStatus = data.codexStatus;
      appState.cliStatus = data.cliStatus || appState.cliStatus;
      if (data.activeAccount) {
        appState.activeAccount = data.activeAccount;
        updateSpotlightUI();
      }
      // The active account can change outside the app: refresh the list only when the active set differs.
      if (Array.isArray(data.accounts) && data.accounts.length > 0 &&
          activeSignature(data.accounts) !== activeSignature(appState.accounts)) {
        appState.accounts = data.accounts;
        renderAccounts(filterAccounts(searchInput.value));
      }
      updateCodexProcessUI();
      updateCliStatusUI();
    }
  } catch (e) {
    // Silent fail on polling
  } finally {
    processPollInFlight = false;
  }
}

async function triggerSync(explicitPassword = null) {
  if (appState.isSyncing) return;
  appState.isSyncing = true;

  syncBtn.classList.add('loading');
  syncBtnText.textContent = t('syncingBtn');
  log(t('syncingLog'));

  try {
    const payload = {};
    if (explicitPassword) {
      payload.password = explicitPassword;
    }

    const res = await apiFetch('/api/sync', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(payload)
    });

    const data = await res.json();

    if (!data.success) {
      if (data.requiresConfiguration) {
        openSettingsModal();
        showToast(t('routerRequiredDesc'), 'info');
        log(t('routerRequiredDesc'));
        return;
      }
      if (data.requiresPassword) {
        openSettingsModal();
        showToast(t('passwordRequiredDesc'), 'info');
        log(t('passwordRequiredDesc'));
        return;
      }
      throw new Error(data.error || 'Sync failed.');
    }

    appState.accounts = data.accounts || [];
    appState.activeAccount = data.activeAccount;

    rowsEntryPending = true;
    renderAccounts(filterAccounts(searchInput.value));
    updateSpotlightUI();
    updateRouterStatusUI(true);

    log(t('syncSuccessLog', appState.accounts.length));
    showToast(t('syncSuccessLog', appState.accounts.length), 'success');
  } catch (err) {
    console.error('Sync failed:', err);
    showToast(`Sync Error: ${err.message}`, 'error');
    log(`Sync Error: ${err.message}`);
    updateRouterStatusUI(false);
  } finally {
    appState.isSyncing = false;
    syncBtn.classList.remove('loading');
    syncBtnText.textContent = t('syncBtn');
  }
}

async function handleSwitch(account) {
  if (appState.isSwitching) return;
  appState.isSwitching = true;
  const cliRunningBefore = appState.cliStatus?.running || 0;
  // The daemon restart waits for running Codex responses, so the wait is shown up front.
  const cliWaiting = Boolean(appState.cliStatus?.daemonRunning) && cliRunningBefore > 0 &&
    appState.routerConfig.restartCliAfterSwitch !== false && Boolean(appState.cliStatus?.restartSupported);

  openSwitchModal(account);
  log(t('switchingLog', account.email || account.name));

  try {
    updateSwitchStep(1, 'active', t('step1Closing'));
    if (cliWaiting) appendSwitchStep(9, 'active', t('cliStepWaiting'));

    const res = await apiFetch('/api/switch', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ accountId: account.id })
    });

    const data = await res.json();

    if (!data.success) {
      throw new Error(data.error || 'Switch failed.');
    }

    const wasRunning = data.switchResult ? data.switchResult.wasRunning : false;
    updateSwitchStep(1, 'done', t('step1Done', wasRunning));
    updateSwitchStep(2, 'done', t('step2Done'));
    updateSwitchStep(3, 'done', appState.routerConfig.launchDesktopAfterSwitch === false ? t('step3Skipped') : (data.switchResult?.launched === false ? t('step3Manual') : t('step3Done')));

    const cli = data.switchResult?.cli || {};
    const cliRestarted = cli.restarted || 0;
    const cliFailed = cli.failed || 0;
    const cliSkipped = cli.skipped || 0;
    const cliAutoOff = appState.routerConfig.restartCliAfterSwitch === false || !appState.cliStatus?.restartSupported;
    // With the setting off the backend reports running sessions as skipped, so one manual-restart message covers both.
    if (cliRunningBefore > 0 && cliAutoOff) {
      const message = t('cliStepManual', cliRunningBefore);
      appendSwitchStep(4, 'pending', message);
    } else if (cliSkipped > 0) {
      appendSwitchStep(7, 'pending', t('cliStepSkipped', cliSkipped));
    }
    const cliReconnected = cli.reconnecting || 0;
    if (cliReconnected > 0) {
      appendSwitchStep(8, 'done', t('cliStepReconnected', cliReconnected));
    }
    if (cliRestarted > 0) {
      appendSwitchStep(5, 'done', t('cliStepRestarted', cliRestarted));
    }
    if (cliFailed > 0) {
      appendSwitchStep(6, 'error', t('cliStepFailed', cliFailed));
    }
    const cliDropped = cli.droppedFlags || 0;
    if (cliDropped > 0) {
      const message = t('cliDroppedFlags', cliDropped);
      log(message);
      showToast(message, 'info');
    }

    appState.activeAccount = data.activeAccount;
    appState.accounts = data.accounts || [];

    updateSpotlightUI();
    renderAccounts(filterAccounts(searchInput.value));

    log(`Active account switched to: ${account.email || account.name}`);
    showToast(t('switchSuccessToast', account.email || account.name), 'success');

    switchModalActions.style.display = 'flex';

    // Keep the modal open when a CLI session needs attention.
    if (cliFailed === 0 && cliSkipped === 0) {
      setTimeout(() => closeModal(switchModal), 1800);
    }

  } catch (err) {
    updateSwitchStep(1, 'error', 'Error: ' + err.message);
    showToast(t('switchFailedToast', err.message), 'error');
    log('Switch error: ' + err.message);
    switchModalActions.style.display = 'flex';
  } finally {
    document.getElementById('step9')?.remove();
    appState.isSwitching = false;
  }
}

async function handleRestartCodex() {
  restartCodexBtn.disabled = true;
  log(t('restartingLog'));
  showToast(t('restartingToast'), 'info');

  try {
    const stopData = await (await apiFetch('/api/codex/stop', { method: 'POST' })).json();
    if (!stopData.success) throw new Error(stopData.error || 'Could not stop ChatGPT');
    await new Promise(r => setTimeout(r, 800));
    const startData = await (await apiFetch('/api/codex/start', { method: 'POST' })).json();
    if (!startData.success) throw new Error(startData.error || 'Could not start ChatGPT');
    showToast(t('restartedToast'), 'success');
    log(t('restartedToast'));
    await pollProcessStatus();
  } catch (err) {
    showToast('Error restarting: ' + err.message, 'error');
  } finally {
    restartCodexBtn.disabled = false;
  }
}

async function handleStopCodex() {
  try {
    const data = await (await apiFetch('/api/codex/stop', { method: 'POST' })).json();
    if (!data.success) throw new Error(data.error || 'Could not stop ChatGPT');
    showToast(t('stoppingToast'), 'info');
    await pollProcessStatus();
  } catch (err) {
    showToast('Error stopping ChatGPT: ' + err.message, 'error');
  }
}

async function handleStartCodex() {
  try {
    const data = await (await apiFetch('/api/codex/start', { method: 'POST' })).json();
    if (!data.success) throw new Error(data.error || 'Could not start ChatGPT');
    showToast(t('restartedToast'), 'success');
    await pollProcessStatus();
  } catch (err) {
    showToast('Error starting ChatGPT: ' + err.message, 'error');
  }
}

function openSettingsModal() {
  settingsRouterUrl.value = appState.routerConfig.url || '';
  if (settingsLaunchDesktop) settingsLaunchDesktop.checked = appState.routerConfig.launchDesktopAfterSwitch !== false;
  if (settingsRestartCli) {
    settingsRestartCli.checked = appState.routerConfig.restartCliAfterSwitch !== false;
    settingsRestartCli.disabled = !appState.cliStatus?.restartSupported;
  }
  settingsPassword.value = '';

  // Fetch current autostart setting
  apiFetch('/api/autostart')
    .then(r => r.json())
    .then(d => {
      if (settingsAutoStart) settingsAutoStart.checked = !!d.enabled;
    })
    .catch(() => {});

  openModal(settingsModal, settingsRouterUrl);
}

async function saveSettings() {
  const url = settingsRouterUrl.value.trim();
  const password = settingsPassword.value.trim();

  try {
    const res = await apiFetch('/api/config', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ routerUrl: url, password, launchDesktopAfterSwitch: settingsLaunchDesktop?.checked ?? true, restartCliAfterSwitch: settingsRestartCli?.checked ?? true })
    });
    const data = await res.json();

    // Save autostart setting only after the configuration was saved.
    if (data.success && settingsAutoStart) {
      try {
        await apiFetch('/api/autostart', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ enabled: settingsAutoStart.checked })
        });
      } catch (e) {}
    }

    if (data.success) {
      showToast(t('settingsSavedToast'), 'success');
      closeModal(settingsModal);
      appState.routerConfig = data.routerConfig;
      await triggerSync(password);
    } else {
      showToast(data.error, 'error');
    }
  } catch (err) {
    showToast('Failed to save: ' + err.message, 'error');
  }
}

// ============================================================================
// Renderers
// ============================================================================

function updateSpotlightUI() {
  const acc = appState.activeAccount;

  // Gentle content fade only when the active account changes, never on polling.
  const spotKey = acc ? `${acc.accountId || ''}|${acc.email || ''}` : '';
  if (spotKey !== lastSpotlightKey) {
    lastSpotlightKey = spotKey;
    spotlightMain.classList.remove('swap');
    void spotlightMain.offsetWidth;
    spotlightMain.classList.add('swap');
  }
  const activeLimitsGrid = document.getElementById('activeLimitsGrid');
  const active5hPercent = document.getElementById('active5hPercent');
  const active5hFill = document.getElementById('active5hFill');
  const active5hReset = document.getElementById('active5hReset');
  const activeWeeklyPercent = document.getElementById('activeWeeklyPercent');
  const activeWeeklyFill = document.getElementById('activeWeeklyFill');
  const activeWeeklyReset = document.getElementById('activeWeeklyReset');
  const activeCreditsCount = document.getElementById('activeCreditsCount');

  if (!acc || (!acc.email && !acc.accountId && !acc.hasApiKey)) {
    spotlightEmail.textContent = t('noActiveSession');
    spotlightPlanBadge.textContent = 'NONE';
    spotlightAccountId.textContent = '-';
    spotlightAccountId.dataset.tooltip = '';
    spotlightAccountId.tabIndex = -1;
    if (activeLimitsGrid) activeLimitsGrid.style.display = 'none';
    return;
  }

  renderEmailWithBreak(spotlightEmail, acc.email || 'ChatGPT Account');
  spotlightPlanBadge.textContent = (acc.plan && acc.plan !== 'unknown' ? acc.plan : t('planUnknown')).toUpperCase();
  spotlightAccountId.textContent = acc.accountId ? acc.accountId : 'N/A';
  spotlightAccountId.dataset.tooltip = acc.accountId || '';
  spotlightAccountId.tabIndex = acc.accountId ? 0 : -1;

  if (acc.lastRefresh) {
    spotlightLastRefresh.textContent = `${t('updatedAt', new Date(acc.lastRefresh).toLocaleTimeString())}`;
  } else {
    spotlightLastRefresh.textContent = '~/.codex/auth.json';
  }

  // Render 5h and Weekly Limits for Active Account (Remaining / Kalan)
  if (acc.usage && !acc.usage.error && acc.usage.primary && activeLimitsGrid) {
    activeLimitsGrid.style.display = 'grid';

    const rem5 = getRemainingQuota(acc.usage.primary);
    active5hPercent.textContent = rem5 === null ? '—' : formatPercent(rem5);
    active5hFill.style.width = `${rem5 ?? 0}%`;
    active5hFill.className = `limit-bar-fill ${rem5 !== null && rem5 <= 10 ? 'danger' : (rem5 !== null && rem5 <= 25 ? 'warning' : '')}`;
    active5hReset.textContent = t('resetsIn', formatResetTime(acc.usage.primary.resetAfterSeconds));

    if (acc.usage.secondary) {
      const remW = getRemainingQuota(acc.usage.secondary);
      activeWeeklyPercent.textContent = remW === null ? '—' : formatPercent(remW);
      activeWeeklyFill.style.width = `${remW ?? 0}%`;
      activeWeeklyFill.className = `limit-bar-fill ${remW !== null && remW <= 10 ? 'danger' : (remW !== null && remW <= 25 ? 'warning' : '')}`;
      activeWeeklyReset.textContent = t('resetsIn', formatResetTime(acc.usage.secondary.resetAfterSeconds));
    } else {
      activeWeeklyPercent.textContent = '—';
      activeWeeklyFill.style.width = '0%';
      activeWeeklyFill.className = 'limit-bar-fill';
      activeWeeklyReset.textContent = t('quotaUnknown');
    }

    const activeCreditsCard = document.getElementById('activeCreditsCard');
    if (activeCreditsCard) {
      const credits = (acc.usage && typeof acc.usage.resetCredits === 'number') ? acc.usage.resetCredits : 0;
      if (activeCreditsCount) {
        activeCreditsCount.textContent = credits;
      }
      activeCreditsCard.style.display = credits > 0 ? 'flex' : 'none';
    }
  } else if (activeLimitsGrid) {
    activeLimitsGrid.style.display = 'none';
  }
}

function updateCodexProcessUI() {
  const isRunning = appState.codexStatus && appState.codexStatus.running;
  if (isRunning) {
    processDot.className = 'indicator-dot running';
    processText.textContent = t('runningBadge');
    toggleCodexProcessBtn.dataset.tooltip = t('stopChatGpt');
    toggleCodexProcessBtn.setAttribute('aria-label', t('stopChatGpt'));
    processActionIcon.innerHTML = '<rect x="6" y="6" width="12" height="12" rx="2"/>';
  } else {
    processDot.className = 'indicator-dot stopped';
    processText.textContent = t('stoppedBadge');
    toggleCodexProcessBtn.dataset.tooltip = t('startChatGpt');
    toggleCodexProcessBtn.setAttribute('aria-label', t('startChatGpt'));
    processActionIcon.innerHTML = '<polygon points="7 4 20 12 7 20 7 4"/>';
  }
}

function updateCliStatusUI() {
  if (!cliStatusPill) return;
  const running = Number(appState.cliStatus?.running) || 0;
  cliStatusPill.style.display = running > 0 ? '' : 'none';
  cliStatusText.textContent = `CLI: ${running}`;
  cliDot.style.display = running > 0 ? '' : 'none';
}

function updateRouterStatusUI(connected = true) {
  routerStatusText.textContent = appState.routerConfig.url ? '9Router' : t('routerNotConfigured');
  statusDot.className = `status-dot-mini ${connected ? '' : 'offline'}`;
}

function renderAccounts(accounts) {
  window.hideAppTooltip?.();
  const list = accounts || [];
  const sortedAccounts = rankAccounts(list);
  const bestAccount = sortedAccounts.find(account => getAccountRank(account).group === 0);
  accountsGrid.innerHTML = '';
  accountsCount.textContent = sortedAccounts.length;

  if (list.length === 0) {
    accountsColHead.style.display = 'none';
    accountsGrid.classList.remove('is-entering');
    // No synced accounts at all shows the sync empty state; a search with no match shows the no-results message.
    const noAccounts = appState.accounts.length === 0;
    emptyState.style.display = noAccounts ? 'flex' : 'none';
    noResultsState.style.display = noAccounts ? 'none' : 'flex';
    if (noAccounts) {
      emptyTitle.textContent = appState.routerConfig.hasPassword ? t('emptyTitle') : t('passwordRequiredTitle');
      emptyDesc.textContent = appState.routerConfig.hasPassword ? t('emptyDesc') : t('passwordRequiredDesc');
    }
    return;
  }

  emptyState.style.display = 'none';
  noResultsState.style.display = 'none';
  accountsColHead.style.display = '';

  // Rows animate only on the first non-empty render and after a manual sync.
  const entering = rowsEntryPending;
  rowsEntryPending = false;
  accountsGrid.classList.toggle('is-entering', entering);
  if (entering) {
    clearTimeout(entryTimer);
    entryTimer = setTimeout(() => accountsGrid.classList.remove('is-entering'), 900);
  }

  // Bars start at the previously drawn width so a changed value animates.
  const bar = (key, target) => {
    const start = entering ? 0 : (quotaMemory.has(key) ? quotaMemory.get(key) : target);
    quotaMemory.set(key, target);
    return `data-bar-target="${target}" style="width: ${start}%"`;
  };

  sortedAccounts.forEach((acc, index) => {
    const row = document.createElement('div');
    row.className = `account-row ${acc.isActive ? 'is-active-row' : ''}`;
    row.style.setProperty('--i', String(Math.min(index, 7)));
    const rowKey = String(acc.id || acc.accountId || acc.email || index);

    const plan = String(acc.plan || '').toLowerCase();
    const knownPlans = ['free', 'plus', 'pro', 'team', 'business', 'enterprise', 'edu'];
    const planClass = knownPlans.includes(plan) ? plan : 'unknown';
    const planLabel = plan && plan !== 'unknown' ? plan : t('planUnknown');
    const idShort = acc.accountId ? String(acc.accountId).slice(0, 8) + '...' + String(acc.accountId).slice(-4) : '-';

    let quotaHtml = '';
    let creditsHtml = '';
    if (acc.usage) {
      if (acc.usage.error) {
        const expired = /\b401\b/.test(acc.usage.error);
        const errorLabel = t(expired ? 'sessionExpired' : 'usageUnavailable');
        const errorDetail = expired ? t('sessionExpiredHelp') : String(acc.usage.error);
        quotaHtml = `<div class="quota-span"><span class="row-error-badge" tabindex="0" data-tooltip="${escapeHtml(errorLabel)}" data-tooltip-detail="${escapeHtml(errorDetail)}">${escapeHtml(errorLabel)}</span></div>`;
      } else if (acc.usage.primary) {
        const rem5 = getRemainingQuota(acc.usage.primary);
        const remW = getRemainingQuota(acc.usage.secondary);
        const percent5 = rem5 === null ? '—' : formatPercent(rem5);
        const percentW = remW === null ? '—' : formatPercent(remW);

        const t5 = formatResetTime(acc.usage.primary.resetAfterSeconds);
        const tW = acc.usage.secondary ? formatResetTime(acc.usage.secondary.resetAfterSeconds) : '';
        const c5Class = rem5 !== null && rem5 <= 10 ? 'danger' : (rem5 !== null && rem5 <= 25 ? 'warning' : '');
        const cwClass = remW !== null && remW <= 10 ? 'danger' : (remW !== null && remW <= 25 ? 'warning' : '');

        const reset5 = t('resetsIn', t5);
        const resetW = acc.usage.secondary ? t('resetsIn', tW) : t('quotaUnknown');
        const tip5 = rem5 === null ? t('quotaUnknown') : `${t('remaining')}: ${percent5}\n${reset5}`;
        const tipW = remW === null ? t('quotaUnknown') : `${t('remaining')}: ${percentW}\n${resetW}`;
        const resetCell5 = quotaResetHtml(t5, reset5);
        const resetCellW = acc.usage.secondary ? quotaResetHtml(tW, resetW) : quotaResetHtml(null, resetW);

        if (typeof acc.usage.resetCredits === 'number' && acc.usage.resetCredits > 0) {
          creditsHtml = `<span class="credits-pill" tabindex="0" data-tooltip="${escapeHtml(`${t('resetCredits')}: ${acc.usage.resetCredits}`)}" data-tooltip-detail="${escapeHtml(t('resetCreditsHelp'))}"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/></svg><span>${acc.usage.resetCredits}</span></span>`;
        }

        quotaHtml = `
          <div class="quota-cell" tabindex="0" data-tooltip="${escapeHtml(t('fiveHourQuota'))}" data-tooltip-detail="${escapeHtml(tip5)}">
            <span class="quota-tag">${escapeHtml(t('colFiveHour'))}</span>
            <div class="quota-top"><span class="quota-pct">${escapeHtml(percent5)}</span></div>
            <div class="mini-bar-track"><div class="mini-bar-fill ${c5Class}" ${bar(`${rowKey}:5h`, rem5 ?? 0)}></div></div>
            ${resetCell5}
          </div>
          <div class="quota-cell" tabindex="0" data-tooltip="${escapeHtml(t('weeklyQuota'))}" data-tooltip-detail="${escapeHtml(tipW)}">
            <span class="quota-tag">${escapeHtml(t('colWeekly'))}</span>
            <div class="quota-top"><span class="quota-pct">${escapeHtml(percentW)}</span></div>
            <div class="mini-bar-track"><div class="mini-bar-fill ${cwClass}" ${bar(`${rowKey}:w`, remW ?? 0)}></div></div>
            ${resetCellW}
          </div>
        `;
      }
    }
    if (!quotaHtml) {
      quotaHtml = `<div class="quota-span"><span class="row-quota-unknown" tabindex="0" data-tooltip="${escapeHtml(t('quotaUnknown'))}">${escapeHtml(t('quotaUnknown'))}</span></div>`;
    }

    row.innerHTML = `
      <div class="cell-account">
        <span class="row-status-dot"></span>
        <div class="account-row-info">
          <div class="account-row-title-line">
            <span class="row-email" tabindex="0" data-tooltip="${escapeHtml(acc.email)}">${escapeHtml(acc.email)}</span>
            <span class="row-plan-badge ${planClass}">${escapeHtml(planLabel)}</span>
            ${acc === bestAccount ? `<span class="best-choice-badge" tabindex="0" data-tooltip="${escapeHtml(t('bestChoice'))}" data-tooltip-detail="${escapeHtml(t('bestChoiceHelp'))}">${escapeHtml(t('bestChoice'))}</span>` : ''}
          </div>
          <div class="account-row-subline">
            <span>ID:</span>
            <span class="row-id" tabindex="0" data-tooltip="${escapeHtml(acc.accountId || '')}">${escapeHtml(idShort)}</span>
            ${creditsHtml}
          </div>
        </div>
      </div>

      ${quotaHtml}

      <div class="cell-action">
        ${acc.isActive ? `
          <span class="active-state-tag">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>
            ${escapeHtml(t('activeBadge'))}
          </span>
        ` : `
          <button class="btn btn-default btn-sm btn-switch-action">${escapeHtml(t('switchBtn'))}</button>
        `}
      </div>
    `;

    const switchBtn = row.querySelector('.btn-switch-action');
    if (switchBtn) {
      switchBtn.addEventListener('click', () => {
        handleSwitch(acc);
      });
    }

    accountsGrid.appendChild(row);
  });

  animateQuotaBars();
}

function filterAccounts(query) {
  if (!query || !query.trim()) return appState.accounts;
  const q = query.toLowerCase().trim();
  return appState.accounts.filter(a => {
    return (a.email && a.email.toLowerCase().includes(q)) ||
           (a.accountId && a.accountId.toLowerCase().includes(q)) ||
           (a.name && a.name.toLowerCase().includes(q));
  });
}

function openSwitchModal(account) {
  switchTargetLabel.textContent = `${t('switchingLog', account.email || account.name)}`;
  switchModalActions.style.display = 'none';

  switchStepsList.innerHTML = `
    <div class="step-line active" id="step1">
      <div class="step-indicator"><div class="clean-spinner"></div></div>
      <div class="step-text">${t('step1Closing')}</div>
    </div>
    <div class="step-line pending" id="step2">
      <div class="step-indicator"><span class="bullet"></span></div>
      <div class="step-text">${t('step2Writing')}</div>
    </div>
    <div class="step-line pending" id="step3">
      <div class="step-indicator"><span class="bullet"></span></div>
      <div class="step-text">${t('step3Opening')}</div>
    </div>
  `;

  openModal(switchModal, switchModal.querySelector('.modal-box'));
}

function appendSwitchStep(stepNum, status, label) {
  const stepEl = document.createElement('div');
  stepEl.className = 'step-line pending';
  stepEl.id = `step${stepNum}`;
  stepEl.innerHTML = `
    <div class="step-indicator"><span class="bullet"></span></div>
    <div class="step-text"></div>
  `;
  switchStepsList.appendChild(stepEl);
  updateSwitchStep(stepNum, status, label);
}

function updateSwitchStep(stepNum, status, label) {
  const stepEl = document.getElementById(`step${stepNum}`);
  if (!stepEl) return;

  stepEl.className = `step-line ${status}`;
  const textEl = stepEl.querySelector('.step-text');
  if (textEl && label) {
    textEl.textContent = label;
  }

  const indicator = stepEl.querySelector('.step-indicator');
  if (!indicator) return;

  if (status === 'active') {
    indicator.innerHTML = '<div class="clean-spinner"></div>';
  } else if (status === 'done') {
    indicator.innerHTML = `
      <svg class="step-ok step-pop" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <polyline points="20 6 9 17 4 12"/>
      </svg>
    `;
  } else if (status === 'error') {
    indicator.innerHTML = `
      <svg class="step-err step-pop" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <line x1="18" y1="6" x2="6" y2="18"/>
        <line x1="6" y1="6" x2="18" y2="18"/>
      </svg>
    `;
  }
}

// ============================================================================
// UI Helpers
// ============================================================================

// Dialog helpers: focus moves in on open, is trapped while open, and returns to the opener on close.
function getFocusable(container) {
  return [...container.querySelectorAll(
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
  )].filter(el => el.getClientRects().length > 0);
}

function openModal(modal, focusTarget) {
  if (modal.classList.contains('open')) return;
  const box = modal.querySelector('.modal-box');
  dialogOpeners.set(modal, document.activeElement);
  modal.classList.add('open');
  const target = focusTarget || getFocusable(box)[0] || box;
  target.focus({ preventScroll: true });
}

function closeModal(modal) {
  if (!modal.classList.contains('open')) return;
  const active = document.activeElement;
  const focusWasInside = !active || active === document.body || modal.contains(active);
  modal.classList.remove('open');
  const opener = dialogOpeners.get(modal);
  dialogOpeners.delete(modal);
  if (!focusWasInside) return;
  if (opener && opener.isConnected) {
    opener.focus({ preventScroll: true });
  } else if (modal === switchModal) {
    // The opener row was re-rendered (account switched), so focus the search field instead.
    searchInput.focus({ preventScroll: true });
  } else {
    openSettingsBtn.focus({ preventScroll: true });
  }
}

function currentDialog() {
  return [settingsModal, switchModal].find(modal => modal.classList.contains('open')) || null;
}

function trapDialogFocus(event) {
  const modal = currentDialog();
  if (!modal) return;
  const box = modal.querySelector('.modal-box');
  const items = getFocusable(box);
  if (items.length === 0) {
    event.preventDefault();
    box.focus({ preventScroll: true });
    return;
  }
  const first = items[0];
  const last = items[items.length - 1];
  const active = document.activeElement;
  const inside = box.contains(active);
  if (event.shiftKey && (!inside || active === first || active === box)) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && (!inside || active === last)) {
    event.preventDefault();
    first.focus();
  }
}

// Quota bars: a bar that changes value animates from its previously drawn width.
function animateQuotaBars() {
  requestAnimationFrame(() => {
    void accountsGrid.offsetWidth;
    accountsGrid.querySelectorAll('[data-bar-target]').forEach((el) => {
      el.style.width = `${el.dataset.barTarget}%`;
    });
  });
}

// Signature of the active set; used to detect switches made outside the app.
function activeSignature(accounts) {
  return accounts
    .filter(account => account.isActive)
    .map(account => String(account.id || account.accountId || account.email || ''))
    .sort()
    .join('|');
}

function log(msg) {
  if (logMessage) {
    logMessage.textContent = msg;
  }
  console.log(`[CodexSwitcher] ${msg}`);
}

function showToast(message, type = 'info') {
  const toast = document.createElement('div');
  toast.className = `toast-item ${type}`;

  let iconSvg = '';
  if (type === 'success') {
    iconSvg = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';
  } else if (type === 'error') {
    iconSvg = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>';
  } else {
    iconSvg = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>';
  }

  toast.innerHTML = `
    <span class="toast-icon">${iconSvg}</span>
    <span class="toast-text">${escapeHtml(message)}</span>
  `;

  toastContainer.appendChild(toast);

  setTimeout(() => {
    toast.classList.add('fading');
    setTimeout(() => toast.remove(), 250);
  }, 3200);
}

const RESET_CLOCK_SVG = '<svg class="reset-clock" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 15 14"/></svg>';

// Table cell: clock and duration only (no ellipsis). The full sentence goes to a screen-reader-only span.
// A null duration means unknown: a dash with no clock.
function quotaResetHtml(duration, fullText) {
  if (!duration) {
    return `<span class="quota-reset">—<span class="sr-only">${escapeHtml(fullText)}</span></span>`;
  }
  return `<span class="quota-reset">${RESET_CLOCK_SVG}<span>${escapeHtml(duration)}</span><span class="sr-only">${escapeHtml(fullText)}</span></span>`;
}

// Builds the email from DOM nodes with a word break after "@" (no innerHTML).
function renderEmailWithBreak(element, text) {
  element.textContent = '';
  String(text).split('@').forEach((part, index) => {
    if (index > 0) element.append('@', document.createElement('wbr'));
    element.append(part);
  });
}

function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
