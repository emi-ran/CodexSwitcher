# Codex Switcher

[English](README.md)

Codex Switcher, 9Router üzerindeki Codex hesaplarını eşitler, kullanım limitlerini gösterir ve etkin `~/.codex/auth.json` hesabını değiştirir. Yeni masaüstü uygulaması mevcut HTML/CSS/JavaScript arayüzüyle Tauri v2 ve Rust arka uç kullanır. Windows, Linux ve macOS hedeflenir.

## Geliştirme

İşletim sisteminize uygun [Tauri v2 gereksinimlerini](https://v2.tauri.app/start/prerequisites/), Node.js 20+ ve Rust kurun. Linux'ta WebKitGTK 4.1 ve appindicator geliştirme paketleri gerekir.

```bash
npm ci
npm run dev
```

`npm start` da Tauri'yi açar. Node.js yalnızca geliştirme aracıdır; dağıtım paketleri Rust IPC ve sistem webview bileşenini kullanır.

## Paketleme ve sürüm çıkarma

```bash
npm run build
```

[Release iş akışı](.github/workflows/release.yml) Windows x64, Linux x64, macOS Apple Silicon ve macOS Intel paketlerini kendi işletim sistemi runner'larında derler. Manuel iş akışı çalıştırması indirilebilir CI çıktıları üretir. `package.json` sürümüyle eşleşen `v<version>` etiketi ayrıca GitHub Release yayımlar.

| Platform | Paketler |
| --- | --- |
| Windows x64 | Portable uygulama `.exe`, NSIS kurulum `.exe`, MSI kurulum |
| Linux x64 | `.deb`, `.AppImage` |
| macOS ARM64 / x64 | Her mimari için `.dmg` |

Windows portable dosyası sistemdeki WebView2 çalışma zamanını kullanır. macOS paketleri geçici imzayla hazırlanır; Apple Developer sertifikası olmadan indirilen uygulamaya Gizlilik ve Güvenlik ayarlarından izin vermek gerekebilir. Linux/macOS davranışı, yalnızca Windows üzerinde yapılan yerel kontrollerle doğrulanamaz.

## Veriler ve çalışma biçimi

Release başlığı, indirme bağlantıları ve paket boyutları CI tarafından oluşturulur. Sürüme özel yenilikler için tag oluşturmadan önce `.github/release-notes/v<version>.md` eklenebilir; GitHub'ın otomatik changelog'u açıklamanın sonuna eklenir.

- Ayarlar ve hesap önbelleği: `~/.codex/switcher_config.dat`, `~/.codex/switcher_accounts.dat` (AES-256-GCM).
- Etkin oturum: `~/.codex/auth.json`. Hesap değişmeden önce `~/.codex/backups/` altına zaman damgalı yedek alınır.
- Eski Windows `.dat` dosyaları aynı kullanıcı profili ve bilgisayarda Tauri uygulaması tarafından okunur. Şifreleme anahtarı kullanıcıya ve bilgisayara bağlıdır; `.dat` dosyalarını başka bilgisayara kopyalamak geçiş yöntemi değildir.
- OAuth tokenları Rust arka uçta kalır. Arayüze token içermeyen hesap ve kota bilgileri gönderilir. Eski sürümün tarayıcı depolamasındaki token içeren önbellek açılışta silinir.
- Ayarlarda oturum açınca otomatik başlatma ve hesap değişiminden sonra masaüstü uygulamasını açma seçeneği vardır. İkinci seçenek kapalıysa yalnızca hesap dosyası güncellenir.
- Ayarları kaydederken şifre alanı boş bırakılırsa kayıtlı şifre korunur.
- İki kotası da kullanılabilir hesaplar, kalan 5 saatlik ve haftalık yüzdelerinin küçüğü yüksek olacak şekilde sıralanır. Eşitlikte sırasıyla 5 saatlik pay, haftalık pay, sıfırlama hakları ve aktif hesap tercih edilir. Kota bilgisi eksik hesaplar kullanılabilir hesaplardan sonra; kotası biten ve kota sorgusu hatalı hesaplar en sonda gösterilir. Sıfırlama hakkı, bitmiş kotayı kullanılabilir saymaz. İlk kullanılabilir sonuç **En iyi seçim** olarak işaretlenir.
- `ROUTER_URL` ve `ROUTER_PASSWORD`, yalnızca ilgili ayar kaydedilmemişse yedek değer olarak kullanılır. Kayıtlı ayarlar önceliklidir.
- NSIS kurulumunda masaüstü kısayolu seçilebilir ve Windows oturumunda otomatik başlatma sorulur. MSI kurulumunda masaüstü kısayolu ve otomatik başlatma özellikleri seçilebilir. İkisi de Windows Uygulamalar listesinde görünür. Etkileşimli kaldırmada `.codex` verileri varsayılan olarak korunur; istenirse yalnızca Switcher ayar ve hesap önbelleği dosyaları silinir. `auth.json` ve yedekleri silinmez. Sessiz MSI kaldırması `.codex` verilerini korur.
- Windows portable `.exe` dosyasının kurulum veya kaldırma sihirbazı yoktur. Linux ya da macOS uygulaması kaldırıldığında `~/.codex` içindeki Switcher dosyaları kalır; gerekirse bu dosyaları `auth.json` ve yedeklere dokunmadan elle silin.
- İsteğe bağlı uygulama açma, Windows'ta Store uygulama kimliğini, Linux'ta `chatgpt` komutunu, macOS'ta `open -a ChatGPT` komutunu kullanır. [Linux masaüstü sürümü şu anda önizlemededir](https://learn.chatgpt.com/docs/linux/linux-app).

Pencere kapatıldığında uygulama sistem tepsisinde kalır. Tam çıkış için tepsi menüsündeki **Quit** öğesini kullanın.

## Kaynak dizini

| Yol | Görev |
| --- | --- |
| `src-tauri/` | Tauri ayarları, Rust IPC, şifreli depolama, hesap ve süreç yönetimi |
| `public/` | Arayüz ve Tauri IPC köprüsü |
| `src-tauri/windows/` | Windows kurulum özelleştirmeleri |

Paket gereksinimleri ve imzalama için [Tauri dağıtım belgelerine](https://v2.tauri.app/distribute/) bakın.
