# Voice Notes TR

Türkçe sesli toplantı notu üretici — tamamen lokal, internet gerek yok.

## Ne yapar

- Bir toplantıyı **dinler** (canlı mikrofon) veya **kaydını alır** (MP3/WAV dosyası)
- Türkçe transkript çıkarır
- Transkripten **kararlar** ve **aksiyon maddeleri** üretir
- Markdown veya Notion'a export eder
- Hiçbir veri buluta gitmez

## Niye var

Otter, Fireflies, Microsoft Copilot gibi araçlar var ama:

- **Türkçe transkript kalitesi düşük** — modeller İngilizce odaklı
- **Bulut zorunlu** — KVKK / şirket içi gizli toplantı için kullanılamaz
- **Pahalı** — küçük takımlar $30/kullanıcı/ay ödeyemez

Bu proje üçünü birden çözmeyi hedefler: yerel dil, yerel işlem, sıfır maliyet.

## Stack

| Katman | Teknoloji |
|---|---|
| Desktop framework | Tauri (Rust + TS) |
| UI | React + TypeScript |
| Audio capture | cpal (Rust) |
| Audio decode | symphonia (Rust) |
| VAD | webrtc-vad veya silero-vad |
| Speech-to-text | faster-whisper (Python sidecar, large-v3-turbo Türkçe) |
| LLM | Ollama + Gemma 3 4B (lokal CPU) |
| Storage | SQLite (rusqlite) |
| Export | Markdown + Notion API |

## Çalıştırma (planlanmış — henüz uygulanmadı)

```bash
# 1. Ollama kur ve Gemma 3'ü indir
ollama pull gemma3:4b

# 2. Python sidecar (faster-whisper) için venv hazırla
cd sidecar && uv venv && uv pip install faster-whisper

# 3. Tauri dev modunda başlat
npm install
npm run tauri dev
```

## Durum

Faz 0 (setup + iskelet). Tauri v2 backend, Vite/React frontend ve Python sidecar iskeleti hazır. Kararlar [DECISIONS.md](./DECISIONS.md)'de, şema [SCHEMA.md](./SCHEMA.md)'de. Roadmap [ROADMAP.md](./ROADMAP.md)'de.

## Donanım gereksinimleri

| Seviye | CPU | RAM | GPU | Beklenen |
|---|---|---|---|---|
| Minimum | 4-core (AVX2) | 8 GB | yok | 60dk → ~20dk transcript, canlı mod marjinal |
| Önerilen | 8-core modern | 16 GB | yok | 60dk → ~8dk transcript, hedef latency'ler tutar |
| Optimal | 8-core modern | 16 GB | NVIDIA 4GB+ VRAM | 60dk → ~2dk transcript, canlı mod <3s |

Detay için bkz. [DECISIONS.md #8 (GPU/CPU stratejisi)](./DECISIONS.md#8-gpucpu-stratejisi-cpu-default-gpu-auto-detect).

## Lisans

MIT — bkz. [LICENSE](./LICENSE).
