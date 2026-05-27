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

## Faz 1 — Dosya modu MVP ✅ (tamamlandı 2026-05-28, tek günde)

Toplantı kaydını dosya olarak yükle → transkript + aksiyon maddeleri çıkar.

### Hafta 1
- [x] Tauri pencere + sürükle-bırak alanı (React, `getCurrentWebview().onDragDropEvent`)
- [x] ~~Rust tarafı: symphonia ile decode~~ — gereksizdi, faster-whisper m4a/mp3/wav'ı PyAV ile direkt okuyor
- [x] Python sidecar yapısı (`sidecar/main.py` — JSON-over-stdio, ping/transcribe/release_model)
- [x] Tauri Rust ↔ Python sidecar IPC (`stt.rs` — Sidecar struct, shutdown lifecycle)
- [x] Transcript UI: zaman damgalı segmentler

### Hafta 2 (ilk yarı)
- [x] Ollama HTTP client (`llm.rs` — reqwest, gemma3:4b, format=json, keep_alive=0s)
- [x] Prompt iterasyonu: Türkçe "aksiyon + karar" çıkarımı
- [x] Çıktı UI'da kararlar/aksiyonlar paneli (assignee badge'leri ile)
- [x] Markdown export butonu (clipboard, başlık + tarih + aksiyon + karar + transkript)
- [x] **Bonus:** Native dosya seçici (Tauri dialog plugin)

### Faz 1 sırasında keşfedilip çözülen ek sorunlar
- 4GB VRAM + 16GB RAM'de Whisper + Gemma birlikte sığmaz → sidecar full shutdown stratejisi
- Ollama 5dk keep_alive sonraki transcribe'ı `mkl_malloc` ile çakıştırıyor → `keep_alive: "0s"`
- Tauri sync command UI thread'i bloke ediyor → async + spawn_blocking
- Windows ctranslate2 nvidia DLL'leri bulamıyor → `_cuda_setup.py` preload helper

**Çıkış kriteri:** ✅ M4A/MP3 yükle, transkript çıkar, Türkçe aksiyon/karar al, Markdown'a kopyala. **V1 satılabilir.**

## Faz 2 — Canlı mod (1.5 hafta planlanmıştı, parça parça gidiyoruz)

### Faz 2a — Canlı transkript (LLM yok) ✅ (2026-05-28)

- [x] cpal ile mikrofon yakalama (varsayılan input device, F32/I16/U16 desteği)
- [x] Mono mix + rubato ile native rate → 16kHz resample, i16 dönüşüm
- [x] webrtc-vad ile chunking (VeryAggressive, 400ms silence, 1.5-8s chunk)
- [x] Sidecar yeni metot: `transcribe_pcm` (base64-encoded i16 LE → numpy float32)
- [x] Rust ↔ frontend event stream (`recording:level`, `recording:segment`, `recording:error`)
- [x] "Kayda Başla / Dur" butonları, VU meter, canlı büyüyen segment listesi
- [x] cpal Stream `!Send` çözümü: kayıt thread'i Stream'i sahip kalır, AppState sadece stop kanalını tutar

### Faz 2a sırasında öğrenilenler
- Windows'ta cpal Stream `!Send` — Tauri state'inde tutulamaz, dedikated thread şart
- React StrictMode dev'de useEffect iki kez çalışır → listener'lar duplicate olur → segment iki kere yazıldı → Promise.all + cancellation flag ile çözüldü
- Gürültülü ortamda VAD Quality modu hep "voice" der → chunk kapanmaz → VeryAggressive moda alındı
- Sürekli konuşmada uzun bekleme: MAX_CHUNK 15s → 8s, segment 8s'de bir gelir

### Faz 2b — Periyodik LLM çağrısı (sonraki oturum)

- [ ] Delta-style prompt (sadece yeni segmentler + önceki aksiyon listesi)
- [ ] N saniyede bir (örn. 60s) extract_actions çağrısı, paneli güncelle
- [ ] Bellek stratejisi: Whisper sürekli açıkken Gemma için RAM/VRAM nasıl açılır
- [ ] Canlı modda kaydı dosyaya da yaz (backup, .wav)

**Çıkış kriteri (Faz 2a):** ✅ Mikrofona konuş, ekranda canlı yazıya çevrildiğini gör.
**Çıkış kriteri (Faz 2b):** Mikrofona konuş, 60 saniye sonra aksiyon maddeleri güncellensin.

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
