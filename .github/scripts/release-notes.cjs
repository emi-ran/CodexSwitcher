const fs = require('node:fs');
const path = require('node:path');

const args = process.argv.slice(2);
const options = {};
for (let i = 0; i < args.length; i += 2) {
  options[args[i]] = args[i + 1];
}
const tag = options['--tag'];
const repo = options['--repo'];
if (!/^v\d+\.\d+\.\d+(?:-[\w.-]+)?$/.test(tag || '') || !/^[\w.-]+\/[\w.-]+$/.test(repo || '')) {
  throw new Error('A version tag and owner/repository are required.');
}
if (!options['--output'] || (!options['--assets'] && !options['--assets-json'])) {
  throw new Error('Provide --output and either --assets or --assets-json.');
}

const assets = options['--assets-json']
  ? JSON.parse(fs.readFileSync(options['--assets-json'], 'utf8')).assets
  : fs.readdirSync(options['--assets'], { withFileTypes: true })
    .filter(entry => entry.isFile())
    .map(entry => ({ name: entry.name, size: fs.statSync(path.join(options['--assets'], entry.name)).size }));
if (!assets?.length) throw new Error('No release assets found.');

const packages = [
  [/portable.*\.exe$/, 'Windows x64', 'Portable EXE'],
  [/setup.*\.exe$/, 'Windows x64', 'Installer EXE'],
  [/\.msi$/, 'Windows x64', 'MSI'],
  [/\.deb$/, 'Linux x64', 'DEB'],
  [/\.AppImage$/, 'Linux x64', 'AppImage'],
  [/macos-arm64\.dmg$/, 'macOS Apple Silicon', 'DMG'],
  [/macos-x64\.dmg$/, 'macOS Intel', 'DMG'],
];
const rows = packages.flatMap(([pattern, platform, label]) => assets
  .filter(asset => pattern.test(asset.name))
  .map(asset => `| ${platform} | [${label}](https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(asset.name)}) | ${(asset.size / 1_000_000).toFixed(1)} MB |`));
if (rows.length !== assets.length) throw new Error('An asset has no matching download table entry.');

const highlightsPath = path.join(__dirname, '..', 'release-notes', `${tag}.md`);
const highlights = fs.existsSync(highlightsPath) ? fs.readFileSync(highlightsPath, 'utf8').trim() : '';
const body = [
  'Codex account switching and usage monitoring for Windows, Linux, and macOS, built with Tauri v2.',
  '',
  ...(highlights ? [highlights, ''] : []),
  '## Downloads',
  '',
  '| Platform | Package | Size |',
  '| --- | --- | ---: |',
  ...rows,
  '',
  '## Installation notes',
  '',
  '- **Windows:** choose the installer EXE for a regular installation, MSI for deployment, or portable EXE to run without installing. The portable EXE requires WebView2.',
  '- **Linux:** choose DEB for Debian/Ubuntu or AppImage for a portable package. Mark the AppImage executable before launching it.',
  '- **macOS:** choose the DMG for your processor. Packages are not notarized; macOS may require approval in Privacy & Security.',
  '- Settings and account cache are stored in `~/.codex`. Windows interactive uninstallers offer optional removal of Switcher data; Codex `auth.json` and backups are kept.',
  '',
  `[Documentation](https://github.com/${repo}/blob/${encodeURIComponent(tag)}/README.md) · [Türkçe](https://github.com/${repo}/blob/${encodeURIComponent(tag)}/README.tr.md)`,
  '',
].join('\n');
fs.writeFileSync(options['--output'], body, 'utf8');
