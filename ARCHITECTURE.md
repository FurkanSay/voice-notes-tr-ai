# Architecture

## Yüksek seviye akış

```
┌──────────────────────────────────────────────────────────┐
│  Tauri Desktop App                                       │
│  ┌────────────────────────────────────────────────────┐  │
│  │  WebView (React + TS)                              │  │
│  │  - Drag-drop / Live record kontrolleri             │  │
│  │  - Transcript pane (segment listesi)               │  │
│  │  - Action items pane                               │  │
│  └─────────────────────┬──────────────────────────────┘  │
│                        │ Tauri commands (IPC)            │
│  ┌─────────────────────┴──────────────────────────────┐  │
│  │  Rust Backend                                      │  │
│  │  ┌──────────┐  ┌─────────┐  ┌────────┐  ┌──────┐   │  │
│  │  │ audio    │→ │ vad     │→ │ stt    │→ │ llm  │   │  │
│  │  │ capture  │  │ chunker │  │ client │  │ client│  │  │
│  │  │ (cpal)   │  │         │  │        │  │       │  │  │
│  │  └──────────┘  └─────────┘  └────┬───┘  └──┬───┘   │  │
│  │                                  │         │       │  │
│  │  ┌──────────┐                    │         │       │  │
│  │  │ storage  │ ← ─ ─ ─ ─ ─ ─ ─ ─ ┴ ─ ─ ─ ─ ┘       │  │
│  │  │ (sqlite) │                                      │  │
│  │  └──────────┘                                      │  │
│  └─────────────┬────────────────────────┬─────────────┘  │
└────────────────┼────────────────────────┼────────────────┘
                 │ JSON-over-stdio        │ HTTP localhost
                 ▼                        ▼
       ┌─────────────────┐      ┌─────────────────┐
       │ Python sidecar  │      │ Ollama daemon   │
       │ faster-whisper  │      │ gemma3:4b       │
       │ (Türkçe)        │      │                 │
       └─────────────────┘      └─────────────────┘
```

## Veri akışı — dosya modu

1. User MP3'ü WebView'a sürükler
2. Frontend `process_file(path)` Tauri command'ını çağırır
3. Rust: symphonia ile decode → 16kHz mono PCM
4. Rust: Python sidecar'a yolla (stdin: ham PCM bytes veya WAV file path)
5. Python: faster-whisper.transcribe() → segments JSON
6. Rust: full transcript string'ini Ollama'ya prompt'a koyar
7. Ollama: kararlar + aksiyon maddeleri JSON döner
8. Rust: SQLite'a yaz, frontend'e event emit
9. Frontend: UI günceller

## Veri akışı — canlı mod

1. Frontend "kayda başla" → Tauri command `start_live_capture()`
2. Rust: cpal ile mikrofon ring buffer'a yazıyor
3. Background task: VAD ile konuşma segmentleri tespit ediyor
4. Tamamlanan konuşma chunk'ı (3-15 saniye) Python sidecar'a gönderilir
5. Whisper transcript döner → frontend'e canlı segment event
6. Her 60 saniyede bir: tüm transcript Ollama'ya → güncellenmiş aksiyonlar
7. Frontend her event'i pane'lere ekler

## Stack kararları ve gerekçeler

### Neden Tauri (Electron değil)?

- **Boyut:** Tauri ~10MB installer; Electron ~150MB
- **Bellek:** Tauri sistem WebView kullanır, RAM dostu
- **Native erişim:** Rust ile audio I/O daha temiz
- **Trade-off:** WebView platforma göre değişir (Edge/Webkit/...), tarayıcılar arası test gerek

### Neden faster-whisper (whisper.cpp değil)?

- Türkçe doğruluğu daha yüksek (CTranslate2 backend optimize)
- Python ekosisteminden faydalanır — model hub, transformers entegrasyonu
- **Trade-off:** Python sidecar bağımlılığı; whisper.cpp tek binary olurdu

Whisper.cpp Faz 3'te alternatif olarak değerlendirilecek. Eğer sidecar yönetimi can sıkarsa geçilir.

### Neden Ollama + Gemma 3 4B?

- Ollama API standart, hızlı kurulum (`ollama pull` yeterli)
- Gemma 3 4B Türkçe'de Llama 3.2 3B'den daha iyi (Gemini'den damıtılmış)
- CPU'da ~12 token/s; 1000 token aksiyon listesi ~80 saniye
- **Trade-off:** İlk indirme ~2.5GB; CPU'da yavaş

### Neden VAD (sürekli Whisper değil)?

- Whisper'ı boş sese sokmak israf — CPU yakar, hatalı transcript üretir
- VAD ile sadece konuşma anları işlenir — 5x daha hızlı
- webrtc-vad: hafif, eski. silero-vad: yeni, daha doğru, biraz daha büyük

İlk versiyon webrtc-vad ile; doğruluk yetersizse silero'ya geçilir.

### Neden SQLite (Postgres veya dosya değil)?

- Desktop app — tek dosya DB, dış servis yok
- rusqlite olgun ve Tauri ile temiz çalışır
- Geçmiş toplantı sorguları (tarih, etiket, arama) için SQL gücü yeter

## Kod prensipleri

Bu projede her commit **SOLID + DRY + KISS** filtresinden geçer. Genel teori değil — bu projede ne demek somut olarak:

### KISS — en üst kural

- **Flat klasör, derin değil.** İlk gün `components/`, `hooks/`, `utils/` açma. Üç dosya birikince ayırırsın.
- **Tek dosya = tek sorumluluk, ama dosya sayısını da abartma.** 50 satırlık bir modül için ayrı dosya açma.
- **Erken soyutlama yok.** Trait/interface yazma — somut tipler iki kez tekrar edince soyutla (rule of three).
- **Config patlatma yok.** Tek `Settings` struct'ı yeter; her ayar için ayrı modül açma.
- **YAGNI:** "İleride lazım olabilir" diye kod ekleme. Şu an gerekmiyorsa **silme bile yapma — yazma**.

### DRY — ama akıllıca

- Aynı kod **üç yerde** tekrar ederse soyutla. İki yerdeyse bırak — kopyala-yapıştır soyutlamadan ucuzdur.
- Type/enum tekrarı: Rust struct'larını TS tarafına `serde` + `ts-rs` veya `specta` ile otomatik üret. **El ile tip tutarlılığı sağlama**.
- Prompt'lar tek bir `prompts.md` veya `prompts.rs` dosyasında. UI'da, Rust kodunda dağıtma.

### SOLID — pratiğe inelim

- **S (Single Responsibility):** `audio.rs` sadece audio I/O yapar. STT'yi içine sokma — `stt.rs` ayrı. LLM'i de.
- **O (Open/Closed):** Yeni STT motoru eklemek istersen `stt.rs` interface'i ile genişlet — diğer modülleri değiştirme. Ama ilk versiyonda **interface yazma**, somut faster-whisper kullan; ikinci motor gelince soyutla.
- **L (Liskov):** Trait yazınca her implementasyonu beklenen davranışa uy. Bu projede ilk başta trait yok zaten.
- **I (Interface Segregation):** Şişman trait yerine küçük trait'ler. Tekrar — ilk versiyon trait'siz başlar.
- **D (Dependency Inversion):** Modüller somut tiplere değil, ihtiyaç duydukları fonksiyon imzalarına bağlı olmalı. Test edilebilir kalsın. **Ama ilk başta zorlama; tek implementasyon varken DI overkill**.

### Yorum ve dokümantasyon

- **Ne yaptığını anlatan yorum yazma.** İsimlendirme yeterli olmalı.
- **Niye yaptığını anlatan yorum yaz** — sadece sürpriz davranış, hidden constraint, bir bug-fix bağlamı varsa.
- Public fonksiyonlara docstring/doc-comment yaz; private'lere yazma.

### Test stratejisi (Faz 1'de minimal)

- Unit test: pure fonksiyonlar için (VAD threshold mantığı, prompt formatting). Audio/LLM I/O'yu mock'lama — entegrasyon test'i daha değerli.
- Entegrasyon test: 5 saniyelik bir Türkçe WAV ile uçtan uca pipeline çağır, çıktının formatını doğrula.
- E2E: Faz 3'te Tauri WebDriver ile.

### Karar listesi (özet)

| Soru | Cevap |
|---|---|
| Klasör derinliği? | 2 seviye max (`src/audio.rs` evet, `src/audio/decoder/mod.rs` hayır) |
| İlk versiyon trait'li mi? | Hayır. Tek somut tip, ikinci implementasyon gelince trait. |
| Hata tipleri? | Tek modülde `thiserror` ile `Error` enum. Anyhow'u sınırla. |
| Async runtime? | Tauri'nin getirdiği tokio. Başka runtime ekleme. |
| Logging? | `tracing` + basit `tracing-subscriber`. JSON logger henüz gerek yok. |

## Klasör yapısı

İlk başta **mümkün olduğunca düz**. Faz 1 sonunda büyürse alt-klasöre böl (rule of three).

```
voice-notes-tr/
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── main.rs         # Tauri setup + command handlers
│   │   ├── audio.rs        # Mic capture + dosya decode (cpal + symphonia)
│   │   ├── vad.rs          # Speech detection
│   │   ├── stt.rs          # Python sidecar IPC
│   │   ├── llm.rs          # Ollama HTTP client + prompt'lar
│   │   └── storage.rs      # SQLite şema + sorgular
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                    # React frontend (flat — büyürse böl)
│   ├── main.tsx
│   ├── App.tsx
│   └── api.ts              # Tauri command wrapper'ları
├── sidecar/                # Python Whisper sidecar
│   ├── whisper_sidecar.py
│   └── pyproject.toml
├── README.md
├── ROADMAP.md
├── ARCHITECTURE.md
├── package.json
└── .gitignore
```

**Ne yok ne neden yok:**

- ❌ `src/components/`, `src/hooks/` — Faz 1 sonunda 5+ component birikince ayır
- ❌ `docs/` alt-klasörü — prompt iterasyonları `llm.rs` içinde string constants olarak başlasın
- ❌ `lib/`, `utils/`, `helpers/` — "ne için olduğu belirsiz" klasörler. İçeriği başka yerde olsun
- ❌ `tests/` ayrı klasörü — Rust idiomatic: unit test'ler aynı dosyada `#[cfg(test)] mod tests`

## Build & dağıtım

- Geliştirme: `npm run tauri dev` (hot reload)
- Build: `npm run tauri build` → her platformda native installer
- Çapraz-derleme: GitHub Actions matrix (ubuntu, macos, windows) — Faz 3'te
- Imza/notarize: Mac için Apple Developer ID ($99/yıl) opsiyonel; ilk versiyon imzasız olabilir

## Performance hedefleri

| Metrik | Hedef | Niye |
|---|---|---|
| Dosya modu: 60 dakika kayıt → transcript | <5 dakika | Daha uzunsa kullanıcı bekleyemez |
| Canlı mod: konuşma → transcript ekranda | <8 saniye | "Canlı" hissi kaybolmasın |
| Aksiyon çıkarımı (60 dk transcript) | <90 saniye | Lokal LLM doğal limiti |
| Bellek (idle) | <300 MB | Desktop app standardı |
| Bellek (canlı kayıt) | <1 GB | Whisper + Ollama yüklü iken |
| Installer boyutu | <50 MB | Tauri'nin sözü; Whisper model ilk açılışta indirilir |
