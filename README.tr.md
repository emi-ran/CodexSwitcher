# Codex Switcher (Türkçe Döküman)

<p align="center">
  <img src="./assets/screenshot.png" alt="Codex Switcher Arayüzü" width="850" />
</p>

<p align="center">
  <b>OpenAI Codex ve ChatGPT Masaüstü Uygulaması için sade, modern Windows hesap değiştirici ve kota takip paneli.</b>
</p>

<p align="center">
  <a href="README.md"><b>English</b></a> •
  <a href="README.tr.md"><b>Türkçe</b></a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Windows-0078D6?logo=windows&logoColor=white" alt="Platform" />
  <img src="https://img.shields.io/badge/S%C3%BCr%C3%BCm-v0.1.0-10b981" alt="Sürüm" />
  <img src="https://img.shields.io/badge/Runtime-Node.js%20%3E%3D18-339933?logo=node.js&logoColor=white" alt="Node" />
  <img src="https://img.shields.io/badge/Entegrasyon-9Router-7928CA" alt="9Router" />
  <img src="https://img.shields.io/badge/Aray%C3%BCz-Minimalist%20Dark-111111" alt="Tasarım" />
  <img src="https://img.shields.io/badge/Lisans-MIT-blue" alt="Lisans" />
</p>

---

## Öne Çıkan Özellikler

- **Özel Masaüstü Penceresi & Sistem Tepsisi (Tray)**: Terminal veya tarayıcı sekmeleri olmadan kendi penceresinde açılır. Kapatıldığında arka planda sistem tepsisine (Tray) küçülür ve çalışmaya devam eder.
- **Tekil Örnek Kilidi (Mutex)**: Uygulama birden fazla kez açılamaz; tekrar açıldığında mevcut pencere ön plana gelir.
- **Akıllı Boş Port Tespiti**: Varsayılan port (3210) doluysa çakışma yaşamaz, otomatik olarak bir sonraki boş porta bağlanır.
- **Windows ile Birlikte Başlat**: Ayarlar penceresinden veya sistem tepsisi menüsünden tek tıkla Windows başlangıcına eklenebilir.
- **Tek Tıkla Sorunsuz Hesap Değiştirme**: `~/.codex/auth.json` kimlik bilgilerini saniyeler içinde değiştirir.
- **Otomatik Süreç Yönetimi**: Hesap değişirken açık `ChatGPT.exe` ve `codex.exe` süreçlerini nazikçe sonlandırır, `auth.json` güncellendikten sonra resmi Windows Store ChatGPT Masaüstü Uygulamasını (`OpenAI.Codex_2p2nqsd0c76g0!App`) otomatik olarak başlatır.
- **Canlı Kalan Kota Takibi**: OpenAI WHAM kullanım API'si ile doğrudan entegre olarak **5 Saatlik Oturum** ve **Haftalık Limit** için **kalan** yüzdeyi, sıfırlanma geri sayımını ve reset haklarını gösterir.
- **9Router Veritabanı Senkronizasyonu**: 9Router sunucunuza giriş yaparak veritabanını çeker, diğer sağlayıcıları ayıklayarak yalnızca gerçek Codex hesaplarını listeler.
- **Kalıcı Yerel Depolama**: Ayarlarınız ve önbellek `.env` dosyasına ihtiyaç duymadan doğrudan `~/.codex/` altında saklanır; güncellemelerde asla kaybolmaz.
- **Çift Dil Desteği (TR / EN)**: Üst menüden tek tıkla Türkçe veya İngilizce arayüze geçiş.

---

## Nasıl Çalışır?

```
┌─────────────────────────────────┐
│     9Router Veritabanı          │ (Codex sağlayıcı hesaplarını çeker ve filtreler)
└────────────────┬────────────────┘
                 │
                 ▼
┌─────────────────────────────────┐
│      OpenAI WHAM Kullanım API   │ (5s ve haftalık kalan kotaları sorgular)
└────────────────┬────────────────┘
                 │
                 ▼
┌─────────────────────────────────┐
│   Güvenli Süreç ve Dosya Değişimi│
│  1. Açık ChatGPT.exe'yi kapat   │
│  2. auth.json'ı yedekle ve yaz  │
│  3. ChatGPT Masaüstü App'i aç   │
└─────────────────────────────────┘
```

1. **Kimlik Bilgisi Yönetimi**:
   Uygulama doğrudan Windows kullanıcı dizinindeki `~/.codex/auth.json` dosyasını yönetir. JWT verisini çözümleyerek plan türü (`chatgpt_plan_type`) ve hesap ID'si bilgilerini dinamik olarak ayrıştırır.

2. **Windows Masaüstü Entegrasyonu**:
   ChatGPT Masaüstü uygulaması Microsoft Store üzerinden kurulduğunda `OpenAI.Codex_2p2nqsd0c76g0!App` kimliğine ve `ChatGPT.exe` süreç adına sahip olur. Uygulama, CLI yerine doğrudan bu resmi uygulamayı güvenle yeniden başlatır.

3. **Kota ve Limit Analizi**:
   Tarayıcı başlıklarıyla `https://chatgpt.com/backend-api/wham/usage` servisine bağlanır; kullanılan miktarı değil **kalan** kota oranını ve sıfırlanma süresini görsel çubuklarla sunar.

---

## Kurulum ve Başlangıç

### Gereksinimler

- **Windows 10 / 11**
- **Node.js** (v18.0 veya üzeri)
- **ChatGPT Windows Masaüstü Uygulaması** (Microsoft Store)
- Bir **9Router** sunucu adresi ve şifresi

### Adım Adım Kurulum

1. **Projeyi indirin veya klonlayın:**
   ```bash
   git clone https://github.com/kullanici-adiniz/CodexSwitcher.git
   cd CodexSwitcher
   ```

2. **Bağımlılıkları yükleyin:**
   ```bash
   npm install
   ```

3. **Yapılandırma:**
   > [!TIP]
   > Herhangi bir `.env` dosyasıyla uğraşmanıza gerek yoktur! Uygulamayı açtıktan sonra sağ üstteki **Ayarlar (Settings)** penceresinden 9Router adresinizi ve şifrenizi bir defa girmeniz yeterlidir. Tüm ayarlarınız ve önbelleğiniz cihazınıza özel **AES-256-GCM** algoritmasıyla şifrelenerek `~/.codex/switcher_config.dat` ve `~/.codex/switcher_accounts.dat` dosyalarında güvenle saklanır. Düz metin (JSON) olarak okunamaz, yalnızca bu uygulama çözebilir.

4. **Uygulamayı başlatın:**
   - **Masaüstü Uygulaması (.exe)** *(En Pratik)*:
     Windows üzerinde doğrudan `CodexSwitcher.exe` dosyasına çift tıklayın! Herhangi bir siyah terminal penceresi veya tarayıcı sekmesi açılmadan, doğrudan kendi şık masaüstü penceresiyle açılır.
   - **Geliştirme / Komut Satırından Çalıştırma**:
     ```bash
     npm start   # veya npm run dev (Masaüstü uygulamasını açar)
     ```
   - **Yeniden .exe Paketleme**:
     ```bash
     npm run build
     ```

5. **Yeni Sürüm Çıkarma (GitHub Actions CI/CD):**
   Tag oluşturup pushladığınızda GitHub Actions Windows x64 için üç ayrı dosya üretir: tek dosyalık portable `.exe`, kurulum `.exe` ve kurulum `.msi`. Sürüm notlarıyla birlikte GitHub Release'e ekler. Portable sürüm kurulum gerektirmez; uygulama ayarları yine kullanıcı profilindeki `~/.codex/` klasöründe tutulur.
   Yerelde aynı paketleri üretmek için Windows üzerinde `npm run build:release` kullanın. Mevcut `npm run build` komutu geliştirme amaçlı klasör paketi ve ona bağlı başlatıcıyı üretmeye devam eder.
   ```bash
   # Otomatik tag oluşturur
   npm run release

   # Tag'i GitHub'a gönderir (CI derlemesini tetikler)
   git push origin v0.1.0
   ```

---

## Proje Dizini

```
CodexSwitcher/
├── assets/
│   └── screenshot.png         # Arayüz ekran görüntüsü
├── lib/
│   ├── codexManager.js        # Auth.json yönetimi, yedekleme ve ChatGPT başlatıcı
│   ├── routerClient.js        # 9Router giriş ve veritabanı ayrıştırıcı
│   └── usageClient.js         # OpenAI WHAM kota sorgulama ve token yenileyici
├── public/
│   ├── app.js                 # Ön yüz mantığı ve durum yönetimi
│   ├── i18n.js                # TR / EN dil sözlükleri
│   ├── index.html             # Semantik HTML arayüzü
│   └── style.css              # Minimal koyu tema stilleri
├── .gitignore                 # Git dışlama kuralları
├── package.json               # Paket bağımlılıkları ve betikler
├── README.md                  # İngilizce Dökümantasyon (Varsayılan)
├── README.tr.md               # Türkçe Dökümantasyon
└── server.js                  # Express API sunucusu
```

---

## API Uç Noktaları

| Uç Nokta | Yöntem | Açıklama |
|---|---|---|
| `/api/status` | `GET` | Aktif hesap, kalan limitler, router durumu ve süreç bilgisini döner |
| `/api/sync` | `POST` | 9Router'a bağlanır, hesapları çeker ve güncel limitlerle harmanlar |
| `/api/switch` | `POST` | ChatGPT'yi kapatır, `auth.json` günceller ve uygulamayı yeniden açar |
| `/api/config` | `POST` | Router adresi ve şifresini şifrelenmiş olarak `~/.codex/switcher_config.dat` içine kaydeder |
| `/api/codex/stop` | `POST` | Açık olan ChatGPT masaüstü süreçlerini kapatır |
| `/api/codex/start` | `POST` | ChatGPT Windows masaüstü uygulamasını başlatır |

---

## İpuçları

- **Dil Değiştirme**: Üst menüdeki **TR** veya **EN** butonuna tıklayarak arayüz dilini anında değiştirebilirsiniz.
- **Arama**: E-posta veya hesap ID'sine göre anlık filtreleme yapabilirsiniz.
- **Kısayol**: Pencereleri kapatmak için `Esc` tuşuna basabilir veya dışarıya tıklayabilirsiniz.
- **Geliştirme**: `npm run dev` komutu Node.js'in yerleşik `--watch` mekanizmasını kullandığı için dosya değişikliklerinde anında güncellenir.

---

## Lisans

Bu proje [MIT Lisansı](LICENSE) kapsamında lisanslanmıştır.
