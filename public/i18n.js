// Codex Switcher i18n Localization Engine

const I18N = {
  tr: {
    appTitle: 'Codex Switcher',
    currentActiveAccount: 'AKTİF HESAP',
    accountId: 'Hesap ID:',
    config: 'Yapılandırma:',
    restartChatGpt: "ChatGPT'yi Yeniden Başlat",
    hour5Remaining: '5-Saatlik Limit (Kalan)',
    weeklyRemaining: 'Haftalık Limit (Kalan)',
    resetCredits: 'Sıfırlama Hakkı',
    manualReset: 'Manuel Sıfırlama',
    accountsTitle: 'Codex Hesapları',
    searchPlaceholder: 'Hesap ara (e-posta veya ID)...',
    activeBadge: 'Aktif',
    switchBtn: 'Geçiş Yap',
    sessionExpired: 'Oturum Süresi Doldu',
    refreshing: 'Yenileniyor...',
    syncBtn: 'Senkronize Et',
    syncingBtn: 'Senkronize Ediliyor...',
    settingsBtn: 'Ayarlar',
    runningBadge: (n) => `Çalışıyor (${n})`,
    stoppedBadge: 'Durduruldu',
    stopChatGpt: "ChatGPT'yi Kapat",
    startChatGpt: "ChatGPT'yi Başlat",
    resetsIn: (t) => `Sıfırlanma: ${t}`,
    remaining: 'Kalan',
    noActiveSession: 'Aktif Codex hesabı yok',
    updatedAt: (t) => `Güncellendi ${t}`,
    emptyTitle: 'Hesap bulunamadı',
    emptyDesc: '9Router üzerinden Codex hesaplarını çekmek için senkronize et butonuna basın.',
    emptyActionBtn: "9Router'dan Senkronize Et",
    noAccountsFoundSearch: 'Aramanızla eşleşen hesap bulunamadı.',
    passwordRequiredTitle: '9Router Şifresi Gerekli',
    passwordRequiredDesc: 'Hesapları çekmek için Ayarlar menüsünden şifrenizi girin.',
    
    // Switch Modal
    switchingTitle: 'Hesap Değiştiriliyor',
    applyingCreds: 'Kimlik bilgileri uygulanıyor...',
    step1Closing: "Çalışan ChatGPT kapatılıyor...",
    step1Done: (was) => was ? 'ChatGPT başarıyla kapatıldı.' : 'ChatGPT çalışmıyordu.',
    step2Writing: '~/.codex/auth.json yedekleniyor ve güncelleniyor...',
    step2Done: 'Kimlik bilgileri ~/.codex/auth.json dosyasına yazıldı (yedek alındı).',
    step3Opening: 'ChatGPT Uygulaması açılıyor...',
    step3Done: 'ChatGPT Masaüstü uygulaması başarıyla açıldı.',
    switchSuccessToast: (email) => `${email} hesabına başarıyla geçildi`,
    switchFailedToast: (err) => `Geçiş başarısız oldu: ${err}`,
    closeBtn: 'Kapat',
    
    // Settings Modal
    settingsTitle: 'Ayarlar',
    settingsSub: '9Router bağlantı ayarları (~/.codex içinde kalıcı saklanır)',
    routerUrlLabel: '9Router Adresi',
    passwordLabel: 'Şifre',
    passwordPlaceholder: '9Router şifrenizi girin',
    passwordHelp: 'Şifreniz ~/.codex dizininde güvenle ve kalıcı olarak saklanır',
    showPassword: 'Göster',
    hidePassword: 'Gizle',
    cancelBtn: 'İptal',
    saveAndSyncBtn: 'Kaydet ve Senkronize Et',
    settingsSavedToast: 'Ayarlar yerel diske kaydedildi',

    // Logs & Notifications
    readyLog: 'Hazır. Hesapları çekmek için Senkronize Et butonuna basın.',
    syncingLog: '9Router üzerinden hesaplar çekiliyor...',
    syncSuccessLog: (n) => `9Router üzerinden ${n} adet Codex hesabı senkronize edildi.`,
    switchingLog: (acc) => `Aktif hesap değiştiriliyor: ${acc}`,
    restartingLog: "ChatGPT uygulaması yeniden başlatılıyor...",
    restartedToast: 'ChatGPT uygulaması açıldı',
    restartingToast: 'ChatGPT yeniden başlatılıyor...',
    stoppingToast: 'ChatGPT durduruldu',
    timeNow: 'şimdi'
  },
  en: {
    appTitle: 'Codex Switcher',
    currentActiveAccount: 'CURRENT ACTIVE ACCOUNT',
    accountId: 'Account ID:',
    config: 'Config:',
    restartChatGpt: 'Restart ChatGPT',
    hour5Remaining: '5-Hour Limit (Remaining)',
    weeklyRemaining: 'Weekly Limit (Remaining)',
    resetCredits: 'Reset Credits',
    manualReset: 'Manual Reset',
    accountsTitle: 'Codex Accounts',
    searchPlaceholder: 'Search accounts (email or ID)...',
    activeBadge: 'Active',
    switchBtn: 'Switch',
    sessionExpired: 'Session Expired',
    refreshing: 'Refreshing...',
    syncBtn: 'Sync',
    syncingBtn: 'Syncing...',
    settingsBtn: 'Settings',
    runningBadge: (n) => `Running (${n})`,
    stoppedBadge: 'Stopped',
    stopChatGpt: 'Stop ChatGPT',
    startChatGpt: 'Start ChatGPT',
    resetsIn: (t) => `Resets in: ${t}`,
    remaining: 'Remaining',
    noActiveSession: 'No active Codex session',
    updatedAt: (t) => `Updated ${t}`,
    emptyTitle: 'No accounts found',
    emptyDesc: 'Click the sync button to fetch Codex accounts from 9Router.',
    emptyActionBtn: 'Sync from 9Router',
    noAccountsFoundSearch: 'No accounts matching your search.',
    passwordRequiredTitle: '9Router Password Required',
    passwordRequiredDesc: 'Enter your password in settings to fetch accounts.',
    
    // Switch Modal
    switchingTitle: 'Switching Account',
    applyingCreds: 'Applying credentials...',
    step1Closing: 'Closing running ChatGPT processes...',
    step1Done: (was) => was ? 'ChatGPT closed successfully.' : 'ChatGPT was not running.',
    step2Writing: 'Backing up and writing ~/.codex/auth.json...',
    step2Done: 'Credentials written to ~/.codex/auth.json (backed up).',
    step3Opening: 'Opening ChatGPT App...',
    step3Done: 'ChatGPT Desktop app reopened successfully.',
    switchSuccessToast: (email) => `Switched to ${email}`,
    switchFailedToast: (err) => `Failed to switch: ${err}`,
    closeBtn: 'Close',
    
    // Settings Modal
    settingsTitle: 'Settings',
    settingsSub: '9Router connection settings (stored locally in ~/.codex)',
    routerUrlLabel: '9Router URL',
    passwordLabel: 'Password',
    passwordPlaceholder: 'Enter 9Router password',
    passwordHelp: 'Saved securely on your PC in ~/.codex',
    showPassword: 'Show',
    hidePassword: 'Hide',
    cancelBtn: 'Cancel',
    saveAndSyncBtn: 'Save & Sync',
    settingsSavedToast: 'Settings saved to local storage',

    // Logs & Notifications
    readyLog: 'Ready. Click Sync to fetch accounts.',
    syncingLog: 'Fetching accounts from 9Router...',
    syncSuccessLog: (n) => `Synced ${n} Codex accounts from 9Router.`,
    switchingLog: (acc) => `Switching active account to: ${acc}`,
    restartingLog: 'Restarting ChatGPT Desktop app...',
    restartedToast: 'ChatGPT Desktop app opened',
    restartingToast: 'Restarting ChatGPT...',
    stoppingToast: 'ChatGPT stopped',
    timeNow: 'now'
  }
};

let currentLang = localStorage.getItem('codex_switcher_lang') || 'tr';

function t(key, ...args) {
  const dict = I18N[currentLang] || I18N.tr;
  const val = dict[key] !== undefined ? dict[key] : (I18N.en[key] !== undefined ? I18N.en[key] : key);
  if (typeof val === 'function') {
    return val(...args);
  }
  return val;
}

function formatPercent(val) {
  return currentLang === 'tr' ? `%${val}` : `${val}%`;
}

function formatResetTime(seconds) {
  if (!seconds || seconds <= 0) return t('timeNow');
  const isTr = currentLang === 'tr';
  const unitMin = isTr ? 'dk' : 'm';
  const unitHr = isTr ? 'sa' : 'h';
  const unitDay = isTr ? 'g' : 'd';

  if (seconds < 3600) {
    return `${Math.round(seconds / 60)}${unitMin}`;
  }
  if (seconds < 86400) {
    const hours = Math.floor(seconds / 3600);
    const mins = Math.round((seconds % 3600) / 60);
    return mins > 0 ? `${hours}${unitHr} ${mins}${unitMin}` : `${hours}${unitHr}`;
  }
  const days = Math.floor(seconds / 86400);
  const hours = Math.round((seconds % 86400) / 3600);
  return hours > 0 ? `${days}${unitDay} ${hours}${unitHr}` : `${days}${unitDay}`;
}

function applyLanguage(lang, onLanguageChangeCallback) {
  currentLang = lang;
  localStorage.setItem('codex_switcher_lang', lang);
  document.documentElement.lang = lang;

  const langBtnTr = document.getElementById('langBtnTr');
  const langBtnEn = document.getElementById('langBtnEn');
  if (langBtnTr && langBtnEn) {
    langBtnTr.classList.toggle('active', lang === 'tr');
    langBtnEn.classList.toggle('active', lang === 'en');
  }

  // Update text of all elements with data-i18n
  document.querySelectorAll('[data-i18n]').forEach((el) => {
    const key = el.getAttribute('data-i18n');
    el.textContent = t(key);
  });

  // Update placeholder of elements with data-i18n-placeholder
  document.querySelectorAll('[data-i18n-placeholder]').forEach((el) => {
    const key = el.getAttribute('data-i18n-placeholder');
    el.placeholder = t(key);
  });

  // Update title attribute of elements with data-i18n-title
  document.querySelectorAll('[data-i18n-title]').forEach((el) => {
    const key = el.getAttribute('data-i18n-title');
    el.title = t(key);
  });

  // Trigger optional callback for component re-rendering
  if (typeof onLanguageChangeCallback === 'function') {
    onLanguageChangeCallback();
  }
}
