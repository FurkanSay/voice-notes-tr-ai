# Roadmap

Tahmini başlangıç: **2026-05-30** · Tahmini bitiş: **2026-07-10** (4-6 hafta).

Çalışma temposu: akşamlar + haftasonu, ~15 saat/hafta.

## Faz 0 — Setup, kararlar & iskelet (3-4 gün)

Her üç bileşenin tek tek çalıştığını doğrula + repo iskeleti + karar belgeleme.

### Kurulum & doğrulama
- [x] Repo init: git, .gitignore, MIT LICENSE, README
- [x] uv (Python paket yöneticisi) kuruldu (`pip install uv`)
- [x] Rust toolchain (rustup, rustc/cargo 1.95.0)
- [x] Tauri CLI v2.11.2 (`cargo install tauri-cli --version "^2.0.0" --locked`)
- [x] faster-whisper venv (`sidecar/.venv`, Python 3.10.8)
- [x] Windows CUDA fix: nvidia-cublas-cu12 + nvidia-cudnn-cu12 + `_cuda_setup.py` helper (DLL preload)
- [x] GPU smoke test: faster-whisper CUDA çalışıyor (RTX 3050 Ti, 4GB VRAM)
- [x] Ollama 0.24.0 kurulumu
- [x] `ollama pull gemma3:4b` (3.34 GB)
- [x] Ollama API smoke test: Türkçe toplantı transkripti → 3/3 action item + assignee doğru, 22.2 t/s GPU

### İskelet & kararlar
- [x] DECISIONS.md yazıldı (19 karar; #9 benchmark sayıları, #10 Ollama smoke test sonuçları ile güncellendi)
- [x] SCHEMA.md yazıldı (SQLite tabloları, V2'ye hazırlık `speaker_id` alanı ile)
- [x] Tauri v2 backend iskeleti (`src-tauri/`)
- [x] Vite + React + TS frontend iskeleti (`src/`, `vite.config.ts`, `tsconfig.json`)
- [x] Root `package.json` (Tauri scripts: `npm run tauri:dev`, `npm run tauri:build`)
- [x] Sidecar Python iskeleti (`sidecar/main.py` — JSON-over-stdio daemon, ping çalışıyor)
- [x] GitHub repo (FurkanSay/voice-notes-tr-ai), 2 commit push'landı

### Türkçe model benchmark (tamamlandı 2026-05-27)
- [x] 34.75s Türkçe mikrofon test kaydı (`voice/Recording.m4a`)
- [x] `sidecar/benchmark.py` yazıldı (small/medium/large-v3-turbo, GPU)
- [x] Sayılar (xRT, kalite gözlemi) DECISIONS.md karar #9'a işlendi
- [x] **Sonuç:** large-v3-turbo hem en kaliteli hem en hızlı (3.1s, 11.1× realtime) — varsayılan kalır

**Çıkış kriteri:** ✅ 3 bileşen çalışıyor, model seçimi sayısal, kararlar belgeli, repo public.

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
