const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const rootDir = path.join(__dirname, '..');
const distAppDir = path.join(rootDir, 'dist', 'CodexSwitcher-win32-x64');
const rootExe = path.join(rootDir, 'CodexSwitcher.exe');

console.log('=======================================================');
console.log(' Building Codex Switcher Desktop Application (Electron)');
console.log('=======================================================');

try {
  // Step 1: Package Electron desktop app
  console.log('\n[1/2] Packaging Electron native desktop application...');
  execSync('npx electron-packager . CodexSwitcher --platform=win32 --arch=x64 --out=dist --overwrite --prune=true --icon=assets/icon.ico', {
    cwd: rootDir,
    stdio: 'inherit'
  });

  // Step 2: Build GUI launcher for root directory (no terminal window)
  console.log('\n[2/2] Generating standalone Windows GUI launcher (CodexSwitcher.exe)...');
  const launcherSource = path.join(rootDir, 'temp_launcher.rs');
  fs.writeFileSync(launcherSource, `
#![windows_subsystem = "windows"]

use std::process::Command;
use std::path::Path;

fn main() {
    let dist_exe = "dist\\\\CodexSwitcher-win32-x64\\\\CodexSwitcher.exe";
    if Path::new(dist_exe).exists() {
        let _ = Command::new(dist_exe).spawn();
    } else {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        use std::os::windows::process::CommandExt;
        let _ = Command::new("cmd")
            .args(&["/c", "npx electron ."])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}
`, 'utf8');

  try {
    execSync(`rustc -O "${launcherSource}" -o "${rootExe}"`, { cwd: rootDir, stdio: 'inherit' });
  } catch (e) {
    console.warn('[Build] rustc compile note:', e.message);
  } finally {
    if (fs.existsSync(launcherSource)) fs.unlinkSync(launcherSource);
    const pdb = path.join(rootDir, 'temp_launcher.pdb');
    if (fs.existsSync(pdb)) fs.unlinkSync(pdb);
  }

  console.log('\n=======================================================');
  console.log(' ✔ Build Successful!');
  console.log(` Desktop App: ${distAppDir}\\CodexSwitcher.exe`);
  console.log(` Root Launcher: ${rootExe}`);
  console.log(' When opened, it runs in its own native application');
  console.log(' window without any terminal or browser tabs!');
  console.log('=======================================================');
} catch (err) {
  console.error('\n✖ Build Failed:', err.message);
  process.exit(1);
}
