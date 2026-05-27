# Roadmap

Tahmini başlangıç: **2026-05-30** · Tahmini bitiş: **2026-07-10** (4-6 hafta).

Çalışma temposu: akşamlar + haftasonu, ~15 saat/hafta.

## Faz 0 — Setup, kararlar & iskelet (3-4 gün)

Her üç bileşenin tek tek çalıştığını doğrula + repo iskeleti + karar belgeleme.

### Kurulum & doğrulama
- [x] Repo init: git, .gitignore, MIT LICENSE, README zaten vardı
- [x] uv (Python paket yöneticisi) kuruldu (`pip install uv`)
- [x] Rust toolchain (rustup, rustc/cargo 1.95.0)
- [x] Tauri CLI v2.11.2 (`cargo install tauri-cli --version "^2.0.0" --locked`)
- [x] faster-whisper venv (`sidecar/.venv`, Python 3.10.8)
- [x] GPU smoke test: faster-whisper `tiny` CUDA üzerinde yüklendi (RTX 3050 Ti, 4GB VRAM)
- [ ] Ollama kurulumu (Windows installer) — kullanıcı yapacak
- [ ] `ollama pull gemma3:4b` (~2.5GB)
- [ ] Ollama API smoke test: `curl http://localhost:11434/api/generate` ile Türkçe çıktı al

### İskelet & kararlar
- [x] DECISIONS.md yazıldı (19 karar: STT, Tauri v2, GPU stratejisi, sidecar bundling, vb.)
- [x] SCHEMA.md yazıldı (SQLite tabloları, V2'ye hazırlık `speaker_id` alanı ile)
- [x] Tauri v2 backend iskeleti (`src-tauri/`)
- [x] Vite + React + TS frontend iskeleti (`src/`, `vite.config.ts`, `tsconfig.json`)
- [x] Root `package.json` (Tauri scripts: `npm run tauri:dev`, `npm run tauri:build`)
- [x] Sidecar Python iskeleti (`sidecar/main.py` — JSON-over-stdio daemon, ping çalışıyor)
- [ ] İlk commit (bu paketin tamamı)

### Türkçe model benchmark
- [ ] 5dk'lık Türkçe sample WAV indir (kullanıcı sağlayabilir veya HF dataset)
- [ ] `sidecar/benchmark.py` yaz: aynı sample üzerinde `small`, `medium`, `large-v3-turbo` koştur
- [ ] Çıktılar: WER (manuel ground truth varsa), süre (saniye/dakika ses), VRAM kullanımı
- [ ] DECISIONS.md karar #9'a sayıları işle, model seçimini gerekçelendir

**Çıkış kriteri:** 3 bileşen ayrı ayrı çalışıyor, ilk commit yapıldı, model seçimi sayısal.

## Faz 1 — Dosya modu MVP (1.5 hafta)

Toplantı kaydını dosya olarak yükle → transkript + aksiyon maddeleri çıkar.

### Hafta 1
- [ ] Tauri pencere + sürükle-bırak alanı (React)
- [ ] Rust tarafı: dosya path'ini al, symphonia ile decode et, 16kHz mono WAV üret
- [ ] Python sidecar yapısı: stdin'den path al, stdout'a JSON transcript bas
- [ ] Tauri Rust ↔ Python sidecar IPC: `tauri::process::Command`
- [ ] Transcript UI: zaman damgalı segmentler

### Hafta 2 (ilk yarı)
- [ ] Ollama HTTP client (Rust): `reqwest` ile `/api/generate`
- [ ] Prompt iterasyonu: "kararlar + aksiyon maddeleri" çıkarımı için
- [ ] Çıktı UI'da kararlar/aksiyonlar paneli
- [ ] Markdown export butonu

**Çıkış kriteri:** MP3 yükle, transkript ve aksiyonlarla Markdown çıktı al.

## Faz 2 — Canlı mod (1.5 hafta)

Mikrofondan canlı dinle, gerçek zamanlı transkript ve aksiyon güncellemesi.

### Hafta 2 (ikinci yarı)
- [ ] cpal ile mikrofon yakalama (16kHz mono, ring buffer)
- [ ] VAD entegrasyonu (önce webrtc-vad dene, gerekirse silero-vad'a geç)
- [ ] Konuşma chunk'ları → faster-whisper'a parçalar halinde gönder
- [ ] Live transcript akışı UI'a (event-based)

### Hafta 3
- [ ] N saniyede bir (örn. 60s) LLM çağrısı → güncel aksiyon listesi
- [ ] "Kayda başla / dur" butonları, görsel ses seviyesi göstergesi
- [ ] Canlı modda kaydı dosyaya da yaz (backup)

**Çıkış kriteri:** Mikrofona konuş, ekranda canlı yazıya çevrildiğini gör, 1 dakika sonra aksiyon maddeleri güncellensin.

## Faz 3 — Polish + dağıtım (1 hafta)

- [ ] SQLite şeması: meetings, segments, action_items tabloları
- [ ] Geçmiş toplantılar listesi UI (sidebar)
- [ ] Manuel düzeltme: transcript satırını tıkla, düzenle
- [ ] Notion API ile export (opsiyonel — API key gerekir)
- [ ] Hata yönetimi: Whisper crash, Ollama down, mikrofon yok senaryoları
- [ ] Tauri build: Win exe (.msi), Mac (.dmg), Linux (.deb)
- [ ] README'ye demo GIF/video
- [ ] GitHub Actions: CI ile build artifact üret

**Çıkış kriteri:** GitHub release'te 3 platform installer var, biri kurup kullanabiliyor.

## Risk noktaları

| Risk | Olasılık | Hafiflet |
|---|---|---|
| Rust'tan Python sidecar yönetimi karmaşık | Yüksek | JSON-over-stdio basit, server-modu sidecar yapma |
| Gemma 3 Türkçe çıktısı tutarsız | Orta | Llama 3.2 3B / Mistral 7B'ye fallback hazır tut |
| webrtc-vad ayarı kötü | Orta | silero-vad'a geç (daha yeni, daha doğru) |
| Tauri+cpal Win'de sürücü sorunu | Orta | sounddevice (Python) ile fallback path |
| 4-6 haftada yetişmez, motivasyon biter | Düşük-orta | Faz 1 bitince ara karar: V1 yayınla, V2'yi sonra |

## Faz 1 yayın hedefi

Eğer 4 hafta sonunda Faz 2'de takılırsan, **Faz 1 bittiği gibi V1 olarak yayınla**. Dosya modu zaten satılabilir bir ürün — canlı mod bonus.
