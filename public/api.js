// The desktop app communicates with the Rust backend through Tauri IPC.
async function apiFetch(path, options = {}) {
  const invoke = window.__TAURI__.core.invoke;
  const body = options.body ? JSON.parse(options.body) : {};
  let data;
  try {
    switch (path) {
      case '/api/status':
        data = await invoke('get_status');
        break;
      case '/api/sync':
        data = await invoke('sync_accounts', { password: body.password || null });
        break;
      case '/api/switch':
        data = await invoke('switch_account', { accountId: body.accountId });
        break;
      case '/api/config':
        data = await invoke('save_settings', {
          routerUrl: body.routerUrl,
          password: body.password,
          launchDesktopAfterSwitch: body.launchDesktopAfterSwitch
        });
        data.success = true;
        break;
      case '/api/autostart':
        if (options.method === 'POST') {
          data = { success: true, enabled: await invoke('set_autostart', { enabled: body.enabled }) };
        } else {
          data = { success: true, enabled: await invoke('get_autostart') };
        }
        break;
      case '/api/codex/stop':
        data = await invoke('stop_desktop');
        break;
      case '/api/codex/start':
        data = await invoke('start_desktop');
        break;
      default:
        throw new Error(`Unsupported API call: ${path}`);
    }
  } catch (error) {
    data = { success: false, error: String(error) };
  }
  return { ok: !!data.success, json: async () => data };
}
