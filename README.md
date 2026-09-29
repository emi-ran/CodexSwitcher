# Codex Switcher

[Türkçe](README.tr.md)

Codex Switcher syncs Codex accounts from 9Router, shows usage limits, and switches the active `~/.codex/auth.json` account. The new desktop app uses Tauri v2 with a Rust backend and the existing HTML/CSS/JavaScript interface. It targets Windows, Linux, and macOS.

## Development

Install [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform, Node.js 20+, and Rust. On Linux, Tauri needs WebKitGTK 4.1 and appindicator development packages.

```bash
npm ci
npm run tauri -- dev
```

The older Electron app remains available with `npm start` during migration. The Tauri build uses Rust IPC and does not package Node.js or Express.

## Build and release

```bash
npm run tauri -- build
```

The [release workflow](.github/workflows/release.yml) builds Windows x64, Linux x64, macOS Apple Silicon, and macOS Intel on their native runners. A manual workflow run builds downloadable CI artifacts. A matching `v<package.json version>` tag also publishes a GitHub Release. The expected files are:

| Platform | Packages |
| --- | --- |
| Windows x64 | portable executable, NSIS setup `.exe`, MSI installer |
| Linux x64 | `.deb`, `.AppImage` |
| macOS ARM64 / x64 | `.dmg` per architecture |

The portable Windows executable uses the installed WebView2 runtime. macOS packages use ad hoc signing; distribution without an Apple Developer certificate can still require approval in macOS Privacy & Security settings. CI packaging on Linux and macOS requires a run on those systems; local Windows checks cannot verify their behavior.

## Data and behavior

- Configuration and account cache: `~/.codex/switcher_config.dat`, `~/.codex/switcher_accounts.dat` (AES-256-GCM).
- Active login: `~/.codex/auth.json`. Before switching, the app writes a timestamped copy under `~/.codex/backups/`.
- Existing Windows `.dat` files remain readable by the Tauri app on the same user profile and machine. Encryption keys are tied to the user and computer, so copying `.dat` files to a different computer is not a migration method.
- Account credentials stay in the Rust backend. The interface receives account details and quota data without OAuth tokens. Old token bearing browser storage is removed when the new interface loads.
- Settings offer automatic start at login and an optional desktop app launch after switching accounts. When launch is disabled, the account file is updated without opening ChatGPT.
- The ChatGPT desktop app launcher uses the Windows Store app ID on Windows, `chatgpt` on Linux, and `open -a ChatGPT` on macOS. ChatGPT must be installed for the optional launch to work. [Linux desktop support is currently a preview](https://learn.chatgpt.com/docs/linux/linux-app).

The interface can be closed to the tray. Use the tray menu's **Quit** action to exit fully.

## Source layout

| Path | Purpose |
| --- | --- |
| `src-tauri/` | Tauri configuration, Rust IPC, encrypted storage, auth switch, process handling |
| `public/` | Shared web interface and IPC/HTTP bridge |
| `electron-main.js`, `server.js`, `lib/` | Previous Electron and Express implementation kept for migration comparison |

See [Tauri distribution documentation](https://v2.tauri.app/distribute/) for package requirements and code signing.
