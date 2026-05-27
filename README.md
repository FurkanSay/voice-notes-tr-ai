# Voice Notes TR

Türkçe sesli toplantı notu üretici — **tamamen lokal**, internet gerek yok.

Bir ses dosyası yükle veya mikrofondan canlı kaydet → Türkçe transkript + aksiyon maddeleri + kararlar → Markdown'a kopyala → Notion / Obsidian / Slack'e yapıştır.

![Voice Notes TR demo](docs/screenshot.png)

## Ne yapar

**İki modu var, ikisi de tamamen offline:**

### 📁 Dosya modu
- Pencereye bir ses dosyası **sürükle-bırak** ya da **Dosya Seç…**
- Desteklenen formatlar: M4A, MP3, WAV, FLAC, OGG, AAC, OPUS, WEBM
- Whisper Türkçe transkripti çıkarır (`large-v3-turbo`, ~11× realtime GPU'da)
- Otomatik olarak Gemma 3 4B aksiyon maddeleri + kararları yakalar
- **Markdown'a Kopyala** ile tek tıkla dışa aktar

### 🎙️ Canlı mod
- **Kayda Başla** → mikrofona konuş → canlı segmentler ekranda akar (VAD-gated, 1.5-8s chunk'lar)
- **VU meter** ile sesini gerçek zamanlı gör
- **Kayda Dur** → otomatik aksiyon/karar çıkarımı
- Markdown export yine tek tıkla

### Neyi vaat etmiyor (artık)
- Sürekli LLM (her N saniyede otomatik) — canlı modda extract sadece durunca tetikleniyor
- Konuşmacı ayrımı (kim ne dedi) — V2 işi
- Bulut entegrasyonu — bu proje **bu özelliği istemiyor**, tüm akış lokal

## Niye var

Otter, Fireflies, Microsoft Copilot gibi araçlar mevcut ama:

- **Türkçe transkript kalitesi düşük** — modeller İngilizce odaklı
- **Bulut zorunlu** — KVKK / şirket içi gizli toplantı için kullanılamaz
- **Pahalı** — küçük takımlar $30/kullanıcı/ay ödeyemez

Bu proje üçünü birden çözer.

## Durum

**V0.3 yayında** — dosya modu ve canlı mod ikisi de çalışıyor uçtan uca. Bkz. [ROADMAP.md](./ROADMAP.md).

| Faz | İçerik | Durum |
|---|---|---|
| Faz 0 | Setup, iskelet, donanım benchmark | ✅ |
| Faz 1 | Dosya modu MVP | ✅ |
| Faz 2a | Canlı transkript | ✅ |
| Faz 2b | Canlı modda auto-extract | ✅ |
| Faz 3 | SQLite geçmiş, installer, Notion export, CI | (gelecek — gerekirse) |

## Kurulum (geliştirici)

> Şu an Tauri build (MSI/DMG/DEB) yok; dev modunda çalıştır.

### 1. Ön gereksinimler

| Şart | Versiyon | Niye |
|---|---|---|
| [Node.js](https://nodejs.org/) | 18+ | frontend (Vite + React) |
| [Rust](https://rustup.rs/) | stable | Tauri backend derleme |
| [Python](https://python.org/) | 3.10 - 3.12 | Whisper sidecar |
| [uv](https://docs.astral.sh/uv/) | son | Python deps (`pip install uv`) |
| [Ollama](https://ollama.com/download) | 0.24+ | LLM runtime |

### 2. Modelleri indir

```bash
ollama pull gemma3:4b   # 3.3 GB
```

(Whisper modeli ilk transkribe çağrısında HuggingFace'den otomatik iner ~1.5GB.)

### 3. Python sidecar'ı kur

```powershell
cd sidecar
uv venv --python 3.10
uv pip install --python .venv\Scripts\python.exe faster-whisper
# Windows + NVIDIA GPU için ek (CUDA 12 runtime libs):
uv pip install --python .venv\Scripts\python.exe nvidia-cublas-cu12 'nvidia-cudnn-cu12>=9.0,<10.0'
```

### 4. Frontend deps + Tauri dev

```powershell
cd ..
npm install
npm run tauri:dev
```

İlk Tauri build 5-10 dk sürer (Rust + 400+ crate). Sonraki çalıştırmalar saniyeler.

## Kullanım

### Dosya modundan
1. Pencereye bir ses dosyası sürükle (veya **Dosya Seç…**)
2. **Transkribe Et** → ilk açılışta Whisper modeli ~10s yüklenir, sonra 30sn ses için ~3s transcribe
3. Otomatik aksiyon çıkarımı başlar (~15-30s)
4. **Markdown'a Kopyala** → clipboard'a düşer

### Canlı moddan
1. **Kayda Başla** → buton "Hazırlanıyor…" olur (sidecar + model pre-warm)
2. Hazır olunca buton **■ Kayda Dur**'a döner, VU meter çalışır
3. Konuş — her 1.5-8 saniyelik konuşma chunk'ı transkribe olur, ekrana akar
4. **■ Kayda Dur** → otomatik aksiyon çıkarımı (~15-30s)
5. **Markdown'a Kopyala** → live segmentler + aksiyonlar/kararlar

## Stack

| Katman | Teknoloji |
|---|---|
| Desktop framework | [Tauri v2](https://tauri.app/) (Rust + TS) |
| UI | React 18 + TypeScript + Vite |
| Audio capture | [cpal](https://github.com/RustAudio/cpal) (cross-platform) |
| Resampling | [rubato](https://github.com/HEnquist/rubato) (sinc interpolation) |
| VAD | [webrtc-vad](https://docs.rs/webrtc-vad) (VeryAggressive mode) |
| Speech-to-text | [faster-whisper](https://github.com/SYSTRAN/faster-whisper) Python sidecar — `large-v3-turbo` |
| LLM | [Ollama](https://ollama.com/) + [Gemma 3 4B](https://ollama.com/library/gemma3) (Q4_K_M) |
| IPC | JSON-over-stdio (sidecar) + HTTP (Ollama) |
| Audio decode | faster-whisper PyAV (m4a/mp3/wav/...) |

Mimari kararların **niye**si: [DECISIONS.md](./DECISIONS.md). Genel akış: [ARCHITECTURE.md](./ARCHITECTURE.md).

## Donanım gereksinimleri

| Seviye | CPU | RAM | GPU | Dosya modu | Canlı mod |
|---|---|---|---|---|---|
| Minimum | 4-core (AVX2) | 8 GB | yok | 60dk → ~20dk | transkript OK, extract yavaş |
| Önerilen | 8-core modern | 16 GB | yok | 60dk → ~8dk | transkript OK, extract orta |
| **İyi** | 8-core modern | 16 GB | NVIDIA 4GB+ VRAM | 60dk → ~2dk | transkript hızlı, extract %70-90 |
| Optimal | 8-core modern | 16 GB | NVIDIA 6GB+ VRAM | 60dk → ~2dk | hepsi sorunsuz |

**Referans makine:** i7-12700H + RTX 3050 Ti Laptop (4GB VRAM). 4GB VRAM'de Whisper (1.6GB) + Gemma (3.1GB) toplam 4.7GB — fit etmek için bazı bellek manevraları gerekiyor (sidecar shutdown, keep_alive=0s). Detay [DECISIONS.md #8](./DECISIONS.md).

**4GB VRAM kullanıcısı ipucu:** Canlı modda auto-extract sırasında NVIDIA Broadcast / Discord overlay gibi başka CUDA uygulamalarını kapatmak fragmentation'ı azaltır. Auto-extract ara sıra başarısız olursa transkript ve Markdown export her durumda çalışır; "Tekrar Çıkar" butonu da var.

## Bilinen sınırlar

- **4GB VRAM canlı mod auto-extract'i ara sıra başarısız olur** (fragmentation) — transkript hep hazır, manuel retry var
- **Konuşmacı ayrımı yok** (V2 işi — pyannote.audio eklenir)
- **Tauri installer yok** — repo clone + dev mode gerekiyor; Faz 3'te eklenebilir
- **İlk açılış 4GB+ model indirme** (Gemma 3.3GB + Whisper 1.5GB) — tek seferlik

## Geliştirici notları

- Kararlar: [DECISIONS.md](./DECISIONS.md) — 19+ ana mimari karar (STT, Tauri v2, GPU/CPU stratejisi, sidecar lifecycle, vb.)
- DB şema (V1: kullanılmıyor, V2'de aktif): [SCHEMA.md](./SCHEMA.md)
- Yol haritası: [ROADMAP.md](./ROADMAP.md)
- Sidecar protokolü: [sidecar/README.md](./sidecar/README.md)

## Lisans

MIT — bkz. [LICENSE](./LICENSE).
