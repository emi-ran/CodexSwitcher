# Codex Switcher

[English](README.md)

Codex Switcher, 9Router üzerindeki Codex hesaplarını eşitler, kullanım limitlerini gösterir ve etkin `~/.codex/auth.json` hesabını değiştirir. Yeni masaüstü uygulaması mevcut HTML/CSS/JavaScript arayüzüyle Tauri v2 ve Rust arka uç kullanır. Windows, Linux ve macOS hedeflenir.

## Geliştirme

İşletim sisteminize uygun [Tauri v2 gereksinimlerini](https://v2.tauri.app/start/prerequisites/), Node.js 20+ ve Rust kurun. Linux'ta WebKitGTK 4.1 ve appindicator geliştirme paketleri gerekir.

```bash
npm ci
npm run tauri -- dev
```

Geçiş süresince eski Electron sürümü `npm start` ile açılabilir. Tauri paketi Rust IPC kullanır; Node.js ve Express'i paketlemez.

## Paketleme ve sürüm çıkarma

```bash
npm run tauri -- build
```

[Release iş akışı](.github/workflows/release.yml) Windows x64, Linux x64, macOS Apple Silicon ve macOS Intel paketlerini kendi işletim sistemi runner'larında derler. Manuel iş akışı çalıştırması indirilebilir CI çıktıları üretir. `package.json` sürümüyle eşleşen `v<version>` etiketi ayrıca GitHub Release yayımlar.

| Platform | Paketler |
| --- | --- |
| Windows x64 | Portable uygulama `.exe`, NSIS kurulum `.exe`, MSI kurulum |
| Linux x64 | `.deb`, `.AppImage` |
| macOS ARM64 / x64 | Her mimari için `.dmg` |

Windows portable dosyası sistemdeki WebView2 çalışma zamanını kullanır. macOS paketleri geçici imzayla hazırlanır; Apple Developer sertifikası olmadan indirilen uygulamaya Gizlilik ve Güvenlik ayarlarından izin vermek gerekebilir. Linux/macOS davranışı, yalnızca Windows üzerinde yapılan yerel kontrollerle doğrulanamaz.

## Veriler ve çalışma biçimi

- Ayarlar ve hesap önbelleği: `~/.codex/switcher_config.dat`, `~/.codex/switcher_accounts.dat` (AES-256-GCM).
- Etkin oturum: `~/.codex/auth.json`. Hesap değişmeden önce `~/.codex/backups/` altına zaman damgalı yedek alınır.
- Eski Windows `.dat` dosyaları aynı kullanıcı profili ve bilgisayarda Tauri uygulaması tarafından okunur. Şifreleme anahtarı kullanıcıya ve bilgisayara bağlıdır; `.dat` dosyalarını başka bilgisayara kopyalamak geçiş yöntemi değildir.
- OAuth tokenları Rust arka uçta kalır. Arayüze token içermeyen hesap ve kota bilgileri gönderilir. Eski sürümün tarayıcı depolamasındaki token içeren önbellek açılışta silinir.
- Ayarlarda oturum açınca otomatik başlatma ve hesap değişiminden sonra masaüstü uygulamasını açma seçeneği vardır. İkinci seçenek kapalıysa yalnızca hesap dosyası güncellenir.
- İsteğe bağlı uygulama açma, Windows'ta Store uygulama kimliğini, Linux'ta `chatgpt` komutunu, macOS'ta `open -a ChatGPT` komutunu kullanır. [Linux masaüstü sürümü şu anda önizlemededir](https://learn.chatgpt.com/docs/linux/linux-app).

Pencere kapatıldığında uygulama sistem tepsisinde kalır. Tam çıkış için tepsi menüsündeki **Quit** öğesini kullanın.

## Kaynak dizini

| Yol | Görev |
| --- | --- |
| `src-tauri/` | Tauri ayarları, Rust IPC, şifreli depolama, hesap ve süreç yönetimi |
| `public/` | Ortak arayüz ve IPC/HTTP köprüsü |
| `electron-main.js`, `server.js`, `lib/` | Geçiş karşılaştırması için tutulan eski Electron ve Express uygulaması |

Paket gereksinimleri ve imzalama için [Tauri dağıtım belgelerine](https://v2.tauri.app/distribute/) bakın.
