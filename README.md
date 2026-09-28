# Codex Switcher

<p align="center">
  <img src="./assets/screenshot.png" alt="Codex Switcher UI" width="850" />
</p>

<p align="center">
  <b>A lightweight, ultra-clean Windows account switcher & quota monitor for OpenAI Codex and ChatGPT Desktop.</b>
</p>

<p align="center">
  <a href="README.md"><b>English</b></a> •
  <a href="README.tr.md"><b>Türkçe</b></a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Windows-0078D6?logo=windows&logoColor=white" alt="Platform" />
  <img src="https://img.shields.io/badge/Runtime-Node.js%20%3E%3D18-339933?logo=node.js&logoColor=white" alt="Node" />
  <img src="https://img.shields.io/badge/Integration-9Router-7928CA" alt="9Router" />
  <img src="https://img.shields.io/badge/UI-Minimalist%20Dark-111111" alt="Design" />
  <img src="https://img.shields.io/badge/License-MIT-blue" alt="License" />
</p>

---

## Highlights

- **Dedicated Desktop App & System Tray**: Opens in its own clean desktop window with no console window and no browser tabs. Closing minimizes directly to the Windows System Tray with a balloon notification.
- **Single Instance Lock (Mutex)**: Prevents duplicate instances; launching again seamlessly restores and focuses the active window.
- **Smart Dynamic Port Allocation**: If port 3210 is occupied, it automatically discovers and binds to the next available free port without port collision errors.
- **Start with Windows Toggle**: One-click autostart configuration available right inside the UI Settings modal and the System Tray context menu.
- **Seamless 1-Click Switching**: Instantly switch between accounts in `~/.codex/auth.json` with zero hassle.
- **Automated Process Lifecycle**: Gracefully closes running `ChatGPT.exe` / `codex.exe` instances and automatically restarts the official Windows ChatGPT Desktop App (`OpenAI.Codex_2p2nqsd0c76g0!App`).
- **Real-Time Remaining Quotas**: Direct integration with OpenAI WHAM backend usage API displaying remaining percentage for **5-Hour Session** and **Weekly Limit**, alongside remaining reset credits and countdown timers.
- **9Router Database Sync**: Authenticates against your 9Router gateway, extracts actual Codex provider accounts, and filters out non-Codex connections.
- **Persistent Local Configuration**: All settings and cached accounts are stored safely in `~/.codex/` without requiring `.env` files.
- **Bilingual Interface (TR / EN)**: Full instantaneous language switching with localized formatting.

---

## Architecture & How It Works

```
┌─────────────────────────────────┐
│     9Router Database Sync       │ (Fetches & filters codex provider tokens)
└────────────────┬────────────────┘
                 │
                 ▼
┌─────────────────────────────────┐
│      OpenAI WHAM Usage API      │ (Fetches 5h and weekly remaining limits)
└────────────────┬────────────────┘
                 │
                 ▼
┌─────────────────────────────────┐
│   Safe Process & File Swapper   │
│  1. Terminate ChatGPT.exe       │
│  2. Backup & write auth.json    │
│  3. Launch ChatGPT Desktop App  │
└─────────────────────────────────┘
```

1. **Credentials Management**:
   The application interacts with `~/.codex/auth.json`. When switching accounts, it creates an atomic backup in `~/.codex/backups/`, parses the JWT payload to extract user claims (`chatgpt_plan_type`, `chatgpt_account_id`), and writes new session tokens.

2. **Windows Desktop Integration**:
   The desktop application is registered under the Windows Store AppX ID `OpenAI.Codex_2p2nqsd0c76g0!App` with process image `ChatGPT.exe`. Switcher handles clean process termination and reopens the desktop client cleanly via `explorer.exe shell:AppsFolder\...`.

3. **Rate Limits & Quota**:
   Queries `https://chatgpt.com/backend-api/wham/usage` using browser-like client headers and authorization bearers, converting raw usage counts into real-time remaining percentage bars.

---

## Getting Started

### Prerequisites

- **Windows 10 / 11**
- **Node.js** (v18.0 or newer recommended)
- **OpenAI ChatGPT Desktop App** (Windows Store)
- A **9Router** instance URL and password

### Installation

1. **Clone or download the repository:**
   ```bash
   git clone https://github.com/your-username/CodexSwitcher.git
   cd CodexSwitcher
   ```

2. **Install dependencies:**
   ```bash
   npm install
   ```

3. **Configuration:**
   > [!TIP]
   > No `.env` file is needed! Simply open the application and configure your 9Router URL and password from the **Settings** modal in the top right. All configuration is safely stored locally in `~/.codex/switcher_config.json` on your PC, ensuring data persists permanently across updates.

4. **Launch the app:**
   - **Native Desktop App (.exe)** *(Fastest)*:
     Double-click `CodexSwitcher.exe` on Windows! It opens immediately in its own native desktop application window with no console/terminal popup and no browser tabs.
   - **From Terminal / Development mode**:
     ```bash
     npm start   # or npm run dev (opens desktop window)
     ```
   - **Package Desktop .exe**:
     ```bash
     npm run build
     ```

---

## Project Structure

```
CodexSwitcher/
├── assets/
│   └── screenshot.png         # UI dashboard preview
├── lib/
│   ├── codexManager.js        # Auth.json parser, backup, process killer & launcher
│   ├── routerClient.js        # 9Router login & database account extractor
│   └── usageClient.js         # OpenAI WHAM usage API client & token refresher
├── public/
│   ├── app.js                 # Frontend controller and state management
│   ├── i18n.js                # TR / EN dictionaries & localized formatting
│   ├── index.html             # Clean semantic HTML markup
│   └── style.css              # Minimal dark design system
├── .gitignore                 # Standard git exclusions
├── package.json               # Node.js project manifest & scripts
├── README.md                  # English Documentation (Default)
├── README.tr.md               # Turkish Documentation
└── server.js                  # Express backend & API endpoints
```

---

## REST API Reference

| Endpoint | Method | Description |
|---|---|---|
| `/api/status` | `GET` | Returns active account, remaining quotas, router info & process state |
| `/api/sync` | `POST` | Authenticates with 9Router, pulls database, enriches with limits |
| `/api/switch` | `POST` | Safely closes ChatGPT, writes `auth.json`, reopens app |
| `/api/config` | `POST` | Saves `routerUrl` and `password` to `~/.codex/switcher_config.json` |
| `/api/codex/stop` | `POST` | Gracefully closes running interactive ChatGPT instances |
| `/api/codex/start` | `POST` | Launches the Windows Store ChatGPT Desktop App |

---

## Keyboard & UI Tips

- **Language Toggle**: Click **TR** or **EN** in the top navigation bar to toggle UI language instantly.
- **Search**: Filter accounts dynamically by email or account ID.
- **Settings Modal**: Press `Esc` or click outside the dialog to dismiss modals.
- **Watch Mode**: `npm run dev` uses Node's native `--watch` flag for zero-delay restarts.

---

## License

This project is licensed under the [MIT License](LICENSE).
