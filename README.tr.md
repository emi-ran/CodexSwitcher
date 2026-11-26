# Codex Switcher

[English](README.md)

Codex Switcher, 9Router üzerindeki Codex hesaplarını eşitler, kullanım limitlerini gösterir ve etkin `~/.codex/auth.json` hesabını değiştirir. Yeni masaüstü uygulaması mevcut HTML/CSS/JavaScript arayüzüyle Tauri v2 ve Rust arka uç kullanır. Windows, Linux ve macOS hedeflenir.

## Komut satırı

Betikler ve yapay zekâ ajanları için komut satırı `codexswitcher-cli`dir ve uygulamanın yanına kurulur. `codexswitcher.exe` uygulamadır: çift tıklayarak açın. Taşınabilir indirmede komut satırı ayrı bir dosya olarak bulunur: `CodexSwitcher-<sürüm>-cli-x64.exe`. Tam başvuru için `codexswitcher-cli help` çalıştırın. İki dosya da PATH'e eklenmez; tam yoluyla çağırın.

```text
codexswitcher-cli list [--plan <plan>] [--refresh] [--json]
codexswitcher-cli current [--json]
codexswitcher-cli switch <selector> [--no-daemon-restart | --restart] [--json]
```

- `list` önce kayıtlı 9Router bağlantısından eşitler (uygulamadaki eşitleme düğmesi gibi), sonra hesapları plan, son kullanma tarihi, kalan 5 saatlik ve haftalık kullanım, sıfırlanma zamanları ve alınma zamanıyla gösterir. `--plan` süzgeci eşitlemeden sonra uygulanır. `--refresh` hâlâ kabul edilir ve etkisi yoktur.
- `current` aynı şekilde eşitler, ardından `auth.json` ile eşleşen önbellekteki hesabı gösterir.
- `switch` eşitlemez; önbellekteki hesapları kullanır.
- Eşitleme veya bir hesabın kullanım alımı başarısız olursa, son başarılı değerler gösterilir. Bu satırlar JSON'da `"stale": true` taşır, metin çıktısında "kayıtlı <zaman> (<yaş>)" gösterir; `fetchedAt` değerlerin alındığı zamandır (bilinmiyorsa null). Uyarı hesabı adıyla anar veya eşitlemenin tamamlanamadığını söyler. Metin çıktısında uyarılar en üstte yer alır; JSON'da `"refreshed"` ve `"warnings"` bulunur. Daha önce değeri olmayan hesaplarda sayı yoktur, yalnızca hata gösterilir.
- `switch`, bağlantı kimliği, e-posta (büyük/küçük harf duyarsız) veya ChatGPT hesap kimliğini kabul eder. Seçici tam olarak bir hesapla eşleşmelidir.
  - Varsayılan: `auth.json` dosyasını yazar ve çalışıyorsa Codex app-server daemon'unu yeniden başlatır; böylece daemon'a bağlı Codex oturumları yeni hesapla yeniden bağlanır. ChatGPT masaüstü uygulaması durdurulmaz veya başlatılmaz. Gömülü oturumlar (örneğin `--no-daemon`) ve uzak oturumlar değiştirilmez ve eski hesapta kalır; çıktı bunların sayısını verir.
  - Bir Codex süreci içinden çalıştırıldığında daemon yeniden başlatması, komut bittikten sonra ayrık bir yardımcı süreçle planlanır; çünkü eşzamanlı durdurma, çalışan turun bitmesini bekler.
  - `--no-daemon-restart` yalnızca `auth.json` dosyasını yazar.
  - `--restart`, uygulamanın tam yeniden başlatmasını çalıştırır: masaüstü uygulamasını ve gömülü CLI oturumlarını kapatıp sürdürür. Bir Codex veya ChatGPT süreci içinden çalıştırılırsa reddedilir.

Notlar:

- Ayarlara ve 9Router parolasına komut satırından erişilemez; hiçbir token, API anahtarı veya yönlendirici adresi yazdırılmaz.
- Kayıtlı ayar dosyası yalnızca gözle okumaya karşı korunur. Aynı kullanıcı altında çalışan başka bir program bu dosyayı yine okuyabilir.
- Çıkış kodları: 0 başarı (eşitleme başarısız olsa da kayıtlı değerler varsa), 1 işlem başarısız (örneğin kayıtlı hesap yokken eşitleme başarısızsa), 2 hatalı kullanım. `--json` ile standart çıktıda tek bir JSON nesnesi çıkar (`{"ok":true,...}` veya `{"ok":false,"error":"..."}`).
- `codexswitcher-cli` bir konsol programıdır: kabuk onu bekler, çıktı sırayla görünür ve `$LASTEXITCODE` çıkış kodunu tutar. Enter'a basmak veya `| Out-String` kullanmak gerekmez.
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
- Etkin oturum: `~/.codex/auth.json`. Hesap değişmeden önce `~/.codex/backups/` altına zaman damgalı yedek alınır. `auth.json` güncellenmeden önce çalışan ChatGPT masaüstü süreçleri kapatılır ve tamamen kapandığı doğrulanır; süreçler zaten kapalıysa veya kapanma sırasında sonlandıysa hesap geçişi hatalı durdurma uyarısı vermeden güvenle tamamlanır.
- Eski Windows `.dat` dosyaları aynı kullanıcı profili ve bilgisayarda Tauri uygulaması tarafından okunur. Şifreleme anahtarı kullanıcıya ve bilgisayara bağlıdır; `.dat` dosyalarını başka bilgisayara kopyalamak geçiş yöntemi değildir.
- OAuth tokenları Rust arka uçta kalır. Arayüze token içermeyen hesap ve kota bilgileri gönderilir. Eski sürümün tarayıcı depolamasındaki token içeren önbellek açılışta silinir.
- Ayarlarda oturum açınca otomatik başlatma ve hesap değişiminden sonra masaüstü uygulamasını açma seçeneği vardır. İkinci seçenek kapalıysa yalnızca hesap dosyası güncellenir.
- Ayarları kaydederken şifre alanı boş bırakılırsa kayıtlı şifre korunur.
- İki kotası da kullanılabilir hesaplar, kalan 5 saatlik ve haftalık yüzdelerinin küçüğü yüksek olacak şekilde sıralanır. Eşitlikte sırasıyla 5 saatlik pay, haftalık pay, sıfırlama hakları ve aktif hesap tercih edilir. Kota bilgisi eksik hesaplar kullanılabilir hesaplardan sonra; kotası biten ve kota sorgusu hatalı hesaplar en sonda gösterilir. Sıfırlama hakkı, bitmiş kotayı kullanılabilir saymaz. İlk kullanılabilir sonuç **En iyi seçim** olarak işaretlenir.
- Plan rozetleri, kullanım isteği başarılı olduğunda canlı yanıttaki (`plan_type`) planı kullanır ve giriş tokenındaki kayıtlı planın yerine geçer. Plan değiştiğinde veya ücretsiz olduğunda kayıtlı son kullanma tarihi temizlenir. Plan bilinmiyorsa Plus yerine **Bilinmiyor** gösterilir.
- Çalışan Codex CLI oturumları algılanır ve üst bilgide **CLI: n** olarak gösterilir; bu gösterge yalnızca n sıfırdan büyükse görünür.
- **Hesap değişiminden sonra açık Codex CLI oturumlarını yeniden başlat** seçeneği açıksa (varsayılan; yalnızca Windows) Codex app-server daemon'ını kullanan oturumlar açık kalır ve daemon yeniden başlatıldığında yeni hesaba kendiliğinden bağlanır. Kendi sunucusunu kullanan oturumlar (`--no-daemon`, `--oss`, `-p`, `--search` ya da izin verilen liste dışında bir `-c` anahtarıyla başlatılanlar) Ctrl+C ile kapatılır, asla zorla sonlandırılmaz ve aynı terminalde özgün seçenekleriyle `codex resume <session-id>` komutuyla yeniden açılır. Oturum kimliği belirlenemezse `codex resume` seçici ekranı açılır. `--remote` ile başlatılan oturumlara hiç dokunulmaz.
- Yönetilen Codex app-server daemon'ı, yeniden başlatma açıksa veya hiçbir daemon oturumu algılanmazsa `auth.json` değişmeden önce durdurulur. Durdurma, süren yanıtların bitmesi için en fazla 320 sn bekler. Daemon, durdurma başarısız olsa bile geçişten sonra yeniden başlatılır; yeniden başlatma yalnızca iki işlem de başarılıysa tamamlanmış sayılır. Yeniden başlatma açıkken bu başarısız olursa daemon oturumları başarısız sayılır.
- CLI yeniden başlatma sınırlamaları: Yönetici olarak çalışan terminallere ulaşılamayabilir (denenmedi); VS Code eklentisinin kendi app-server süreci yeniden başlatılmaz; güvenle yeniden yazılamayan seçenekler atlanır. Başarısız ve atlanan oturumlar geçiş penceresinde listelenir; aktarılamayan seçenekler bildirim olarak gösterilir.
- Aynı terminalde kapatma ve devam ettirme, Windows'ta codex-cli 0.172.0 ile tek hesapta bir kez doğrulandı. Farklı bir hesapta devam ettirme henüz doğrulanmadı.
- `ROUTER_URL` ve `ROUTER_PASSWORD`, yalnızca ilgili ayar kaydedilmemişse yedek değer olarak kullanılır. Kayıtlı ayarlar önceliklidir.
- NSIS kurulumunda masaüstü kısayolu seçilebilir ve Windows oturumunda otomatik başlatma sorulur. MSI kurulumunda masaüstü kısayolu ve otomatik başlatma özellikleri seçilebilir. İkisi de Windows Uygulamalar listesinde görünür. Etkileşimli kaldırmada `.codex` verileri varsayılan olarak korunur; istenirse yalnızca Switcher ayar ve hesap önbelleği dosyaları silinir. `auth.json` ve yedekleri silinmez. Sessiz MSI kaldırması `.codex` verilerini korur.
- Windows portable `.exe` dosyasının kurulum veya kaldırma sihirbazı yoktur. Linux ya da macOS uygulaması kaldırıldığında `~/.codex` içindeki Switcher dosyaları kalır; gerekirse bu dosyaları `auth.json` ve yedeklere dokunmadan elle silin.
- İsteğe bağlı uygulama açma, Windows'ta Store uygulama kimliğini, Linux'ta `chatgpt` komutunu, macOS'ta `open -a ChatGPT` komutunu kullanır. [Linux masaüstü sürümü şu anda önizlemededir](https://learn.chatgpt.com/docs/linux/linux-app).

Pencere kapatıldığında uygulama sistem tepsisinde kalır. Tam çıkış için tepsi menüsündeki **Quit** öğesini kullanın.

## Kaynak dizini

| Yol | Görev |
| --- | --- |
| `src-tauri/` | Tauri ayarları, Rust IPC, şifreli depolama, hesap ve süreç yönetimi |
| `src-tauri/src/cli.rs` | Codex CLI oturum algılama, nazikçe kapatma ve hesap değişiminden sonra devam ettirme |
| `src-tauri/src/bin/codexswitcher-cli.rs` | Konsol komut satırı (`codexswitcher-cli`); uygulamayla aynı komut kodunu çağırır |
| `public/` | Arayüz ve Tauri IPC köprüsü |
| `src-tauri/windows/` | Windows kurulum özelleştirmeleri |

Paket gereksinimleri ve imzalama için [Tauri dağıtım belgelerine](https://v2.tauri.app/distribute/) bakın.
