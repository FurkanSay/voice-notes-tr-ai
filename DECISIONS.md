# Decisions

Mimari ve teknoloji kararları. Her karar için: **karar / alternatifler / niye / geri dönüş maliyeti**.

İlk yazım: 2026-05-27.

---

## 1. STT motoru: faster-whisper (Python sidecar)

**Karar:** Speech-to-text için `faster-whisper` Python sidecar'ı kullan. Rust ile JSON-over-stdio konuş.

**Alternatifler:**
- `whisper-rs` (whisper.cpp Rust binding) — Python'u tamamen elemiş olurduk
- `whisper.cpp` ayrı binary olarak sidecar — Rust bağımlılığı yok ama Python'unkine benzer dış-process problemi var

**Niye:** ARCHITECTURE.md bu yolu seçmişti. CTranslate2 backend INT8 quantization ile rekabetçi hız + olgun Python ekosistemi (HF Hub, transformers entegrasyonu). Sidecar yönetiminin maliyetini #5 ve #11 numaralı kararlarla bastırıyoruz.

**Geri dönüş maliyeti (faster-whisper → whisper-rs):** Orta. `stt.rs` modülü tek bir trait/fonksiyon imzası arkasında olduğu sürece, sidecar yerine in-process whisper-rs çağrısı sürebilir. Sidecar lifecycle/IPC kodu silinir; supervisor yok olur. Tahmini 2-3 günlük rewrite.

---

## 2. Tauri sürümü: v2

**Karar:** Tauri **2.x**. Yeni projede v1 başlamak teknik borç.

**Alternatifler:** Tauri v1 — daha eski, daha çok örnek var.

**Niye:** v2 mid-2025'ten beri GA. Yeni `tauri-plugin-shell` sidecar API'sı, daha iyi capability sistemi, mobile path açık.

**Geri dönüş maliyeti:** Yüksek. v2'den v1'e gitmenin neredeyse hiçbir mantığı yok — v2'de kalıyoruz.

---

## 3. Konuşmacı ayrımı (diarization): V2'ye

**Karar:** V1'de **yok**. Şema bugün `speaker_id INTEGER NULL` alanını içerir, V1 boyunca hep null.

**Alternatifler:**
- V1'e ekle (pyannote.audio)
- Hiç olmasın

**Niye:** Solo proje, 4-6 hafta zaten dolu. pyannote ayrı Python bağımlılığı + GPU memory + complex tuning. Şemayı baştan doğru yaparsak V2 eklemesi düz.

**Geri dönüş maliyeti:** Düşük. Şema hazır, sadece yeni Python kütüphanesi + UI'da renkli badge.

---

## 4. İlk açılışta model indirme (Whisper + Gemma): in-app wizard

**Karar:** İlk açılışta uygulama içi wizard ~4GB model indirir (Whisper ~1.5GB + Gemma 3 4B ~2.5GB). Resume desteği, AppData/Local'e saklar.

**Alternatifler:**
- Kullanıcıya manuel script çalıştırt (kötü UX)
- Modeli installer'a göm (50MB → 4GB+ installer; Tauri avantajını yok eder)

**Niye:** Tek temas noktası, kullanıcı uygulamayı açıp "İleri" basar. Whisper'ı kendimiz indiririz (HF Hub), Gemma için `ollama pull gemma3:4b` shell çağrısı.

**Geri dönüş maliyeti:** Orta. Installer + bundle'a model gömmeye dönmek = build pipeline değişir, installer 4GB+ olur. Yapmayız.

---

## 5. Python sidecar bundling: PyInstaller `--onefile`

**Karar:** Production'da PyInstaller ile single-file exe → `src-tauri/binaries/whisper-sidecar-{target-triple}.exe`. Tauri v2 `externalBin` config'i ile OS-aware bundle.

**Alternatifler:**
- Kullanıcıya `pip install` yaptır (kullanıcıdan Python ister, ekstra kurulum)
- `embedded-python` Rust crate'i (deneysel)
- Sidecar'ı kaldır → whisper-rs (#1 numaralı kararı tersine çevirmek demek)

**Niye:** Tek dosya, kullanıcı Python görmez. PyInstaller olgun, faster-whisper destekli.

**Boyut beklentisi:** PyInstaller + ctranslate2 + faster-whisper + onnxruntime ≈ **+200-300MB** installer. ARCHITECTURE'daki 50MB hedefi gerçekçi değil — **revize: ~250MB installer + 4GB ilk-açılış model indirme**.

**Geri dönüş maliyeti:** Düşük-orta. PyInstaller config sadece bir spec dosyası, değiştirmek kolay.

---

## 6. Sidecar IPC protokolü: line-delimited JSON over stdio

**Karar:** Her satır bir mesaj. Request `{id, method, params}`, response `{id, result | error}`. JSON-RPC stili ama hafif. stderr log için.

**Alternatifler:**
- Length-prefixed framing (binary, daha sağlam ama daha karmaşık)
- HTTP localhost server (port çakışması, firewall, gereksiz overhead)
- gRPC (overkill)

**Niye:** Debug edilebilir (cat ile gözle gör), implement basit, throughput yeterli (transcript I/O ses dosyası okumakla domine olur, JSON parse marjinal).

**Geri dönüş maliyeti:** Düşük. Tek protokol katmanı, değişirse hem Rust hem Python tarafında ~50 satır.

---

## 7. Sidecar lifecycle: long-running daemon + Rust supervisor

**Karar:** App başında bir kez sidecar başlat, app kapanınca durdur. Per-request başlatma yok (model RAM'e her seferinde yüklenmez). Rust tarafında supervisor: exit görürse 1s/3s/10s exponential backoff ile restart, frontend'e `sidecar:status` event'i emit.

**Alternatifler:**
- Per-request başlat (model load her seferinde 25s — kabul edilemez)
- Multiple sidecar workers (gereksiz karmaşıklık, single user app)

**Niye:** Model load maliyeti tek seferlik. Crash recovery şart — sandbox/OS sebepli Python crash'inde kullanıcıyı kaybetmeyelim.

**Geri dönüş maliyeti:** Düşük.

---

## 8. GPU/CPU stratejisi: CPU default, GPU auto-detect

**Karar:**
- **Faster-whisper:** `device="cuda", compute_type="float16"` eğer CUDA varsa, aksi halde `device="cpu", compute_type="int8"`. Tespit `ctranslate2.get_cuda_device_count()` ile.
- **Ollama:** zaten otomatik CUDA/Metal tespiti yapar; bizim tarafta kod yok.
- **Installer:** CPU-only build. CUDA isteyen kullanıcı NVIDIA CUDA Toolkit kurar + dokümandaki opt-in adımı izler.

**Bu makinedeki durum (referans):** RTX 3050 Ti Laptop, **4GB VRAM**. CUDA 12.8 driver. `ctranslate2.get_cuda_device_count()` → 1.

**VRAM bütçesi (4GB):**
| Yük | VRAM |
|---|---|
| Whisper large-v3-turbo INT8 | ~900MB |
| Whisper large-v3-turbo FP16 | ~1.6GB |
| Gemma 3 4B Q4_K_M | ~3GB |
| Whisper INT8 + Gemma Q4 birlikte | ~4GB (sınır) |

**Pratik kural (Faz 1 testi sırasında 2026-05-28'de iki kez revize edildi):**

İlk varsayım: dosya modu sıralı, Whisper biter VRAM serbest, Gemma rahatça yüklenir. **Yanlış çıktı.**

Gerçekte iki ayrı kaynak sıkışıyordu:

1. **ctranslate2 Whisper'ı transcribe sonrası VRAM+RAM'de tutuyor** (cache, mantıklı — sonraki call 25s yerine 3s). `release_model` ile Python tarafında modeli düşürmek yetmiyor çünkü…
2. …**Python process'inin kendisinin CUDA context'i** kalıyor. Ollama'nın free-VRAM probe'u bu yüzden eksik raporluyor (3.7GB free olmasına rağmen 0 görüyor), CPU fallback'ine düşüyor.
3. CPU fallback'inde Gemma ~3.6GB sistem RAM ister. 16GB makinede tipik kullanıcı %95+ dolu olduğu için bu da 500 düşürür.
4. Ek olarak: Ollama default `keep_alive=5min`. Bir önceki çıkarımdan kalan Gemma RAM/VRAM'i sonraki transcribe'ın `mkl_malloc` ile çakışmasına neden oluyor.

**Çözüm (uygulanan):**
- Aksiyon çıkarımına geçmeden **sidecar Python process'ini tamamen kapat** (`Sidecar::shutdown` → stdin EOF + grace + kill). CUDA context tamamen silinir, Ollama tüm VRAM'i görür, Gemma **GPU'da** yüklenir → 30s'de çıkarım.
- Sonraki transcribe sidecar'ı lazy respawn eder (~10s model reload penalty, kabul edilebilir).
- Ollama isteğine `keep_alive: "0s"` ekledik → Gemma cevaptan hemen sonra RAM+VRAM'den düşer, sonraki transcribe `mkl_malloc` ile çakışmaz.

**Canlı mod:** Whisper sürekli GPU'da, Gemma her 60s'de bir çağrılacak. O zaman bu shutdown numarası yapılamaz (latency kayıp). Karar Faz 2'ye: Gemma'yı CPU'ya zorla (`num_gpu: 0`), i7-12700H ile ~90s — canlı modda Ollama "delta-only" prompt olduğu için zaten kısa.

**Geri dönüş maliyeti:** Orta. Karar şimdi 3 yerde uygulanmış (stt.rs shutdown, lib.rs extract_actions, llm.rs keep_alive). 16GB'tan fazla RAM ya da 8GB+ VRAM'li donanımda bu hile gereksizleşir; gelecekte donanım algıla, eğer yeterli kaynak varsa shutdown'u atla.

### Canlı modda bilinen sınır (2026-05-28, Faz 2b sonrası)

4GB VRAM'li referans makinede canlı mod auto-extract %70-90 başarılı, ara sıra `Ollama 500 memory layout cannot be allocated` döndürüyor. Sebep: VRAM fragmentation. Whisper sidecar shutdown'dan sonra `nvidia-smi` 3.7 GiB free gösterse de, Gemma 3.1 GiB **contiguous** blok bulamıyor — özellikle NVIDIA Broadcast gibi başka CUDA uygulamaları sırada VRAM tutuyorsa. Üç pratik etki:

1. **Faz 2a değeri ulaşmış durumda:** canlı transkript + Markdown export her zaman çalışıyor.
2. **Auto-extract bonustur**, garanti değil. Frontend dostane hata mesajıyla yönlendiriyor.
3. **Kullanıcı "Tekrar Çıkar"a basabilir**, ya da "NVIDIA Broadcast'i kapat" gibi bir öneri görüyor.

**Donanım rehberi (README'ye eklendi):**
- 4GB VRAM: dosya modu sorunsuz, canlı mod transkript sorunsuz, auto-extract ara sıra
- 6GB+ VRAM: hepsi sorunsuz, fragmentation marjı geniş
- 8GB+ VRAM: shutdown numarası bile gereksiz, ileride donanım algılayıcı eklenebilir

**Gelecek opsiyonu:** Faz 3'te Gemma 3 1B fallback'i kullanıcı seçimi olarak sun (~1GB, kalite düşer ama 4GB VRAM'de garanti çalışır).

---

## 9. Whisper model seçimi: large-v3-turbo (varsayılan) — benchmark ile doğrulandı

**Karar:** Default `large-v3-turbo`. Faz 0 benchmark'ı doğruladı — hem kalite hem hız liderliği bu modelde.

### Faz 0 benchmark sonuçları (2026-05-27)

**Donanım:** RTX 3050 Ti Laptop, 4GB VRAM, CUDA 12, float16 compute_type.
**Audio:** 34.75s Türkçe mikrofon testi konuşması (gürültülü, kısa cümleler).

| Model | İlk yükleme | Transcribe | xRT | Karakter | Kalite notu |
|---|---|---|---|---|---|
| tiny | 26s | (manuel) | — | 312 | "biriyeti", "kayısız", "yapayacak" — uydurmuş kelimeler, tekrarlı |
| small | 5.4s | 4.2s | 8.2× | 276 | "ben nasıl fısaldım" (anlamsız), "bir öte" (mikrofon yerine), noktalama yok |
| medium | 16.0s | 5.2s | 6.7× | 269 | "yapay zeka" doğru, noktalama düzgün, "gürültü" (mikrofon yerine) |
| **large-v3-turbo** | 396s (1.5GB HF download) | **3.1s** | **11.1×** | 293 | Cümle yapısı kusursuz, noktalama mükemmel, sadece "gürültü" hatası kaldı |

### Şaşırtıcı bulgular

- **large-v3-turbo en hızlı transcribe!** Turbo varyantın decoder katmanları küçültülmüş — hem daha kaliteli hem 1.5-1.7× hızlı.
- İlk yükleme 6.5 dakika (HuggingFace download). Sonraki açılışlar disk cache'den ~10s.
- 4GB VRAM yeterli — float16 büyük model tek başına ~1.6GB.
- "gürültü" ↔ "mikrofon" hatası model bağımsız — kelime bazlı; prompt engineering veya post-processing gerek (V2).

### Niye `large-v3-turbo` (small/medium değil)

1. Kalite farkı dramatik — small'da yarım kelimeler var
2. Hız avantajı turbo'da (kalitesi yüksek + hızı en yüksek = double win)
3. VRAM yeterli (FP16 ile ~1.6GB)
4. İlk açılış maliyetini in-app wizard zaten karşılıyor (karar #4)

### Turbo'nun bilinen kısıtı — sadece transcription

`large-v3-turbo` decoder katmanları 32 → 4'e budanmış. Bu hız kazandırıyor ama **çeviri (translation) görevini yapamaz** — yalnızca `task="transcribe"`. Bizim akış "Türkçe ses → Türkçe metin" olduğu için sorun değil. Eğer ileride "Türkçe ses → İngilizce metin" gibi bir özellik istenirse `large-v3` (full) modeline geçmek gerek. ROADMAP'te yok, scope dışı.

**Geri dönüş maliyeti:** Düşük. `WHISPER_MODEL` env değişkeni ile kontrol ediyoruz. Yavaş CPU'lu kullanıcı `medium`'a düşebilir, kod değişmez.

---

## 10. LLM model: Gemma 3 4B (Ollama) — Türkçe testi doğruladı

**Karar:** `gemma3:4b` Ollama üzerinden. Q4_K_M kuantizasyon (Ollama default).

**Alternatifler:** Llama 3.2 3B, Mistral 7B, Qwen 2.5 7B

**Niye:** Gemma 3 Türkçe'de Llama 3.2'den iyi (Gemini'den damıtılmış). 4B sweet spot — 7B Q4 ~5GB VRAM, 4GB'ımız yetmez.

### Faz 0 smoke test (2026-05-27)

**Donanım:** RTX 3050 Ti Laptop, 4GB VRAM. **Donanım algılama:** Ollama otomatik CUDA gördü ve kullandı.

**Prompt:** 4 kişilik Türkçe toplantı transkripti → action_items + decisions JSON çıkarımı.

| Metrik | Değer | Notu |
|---|---|---|
| Toplam süre | 30.2s | cold-start dahil (model VRAM'e yüklendi) |
| Prompt eval | 152 token / 0.8s | prefill çok hızlı (~190 t/s) |
| Generate | 169 token / 7.6s | **22.2 t/s** |
| Aksiyon doğruluğu | 3/3 (atayan kişi dahil) | "yarınki sunum için slayt → Ahmet" gibi nüansı yakaladı |
| Karar doğruluğu | 1/2 — "Bütçe 50 bin lira" doğru | "Gerisini sonra görüşürüz" yanlışlıkla karar olarak listelendi (prompt engineering ile çözülür) |

**Hız sonucu:** ARCHITECTURE.md'deki "~12 t/s CPU" tahmininin 1.8× üzerinde (GPU). DECISIONS karar #18'deki hedef (<25s aksiyon, GPU) güzel tutuyor.

**Geri dönüş maliyeti:** Düşük. Ollama API ile model adı değiştirmek tek satır.

---

## 11. LLM context stratejisi (canlı modda): delta-style

**Karar:** Canlı modda her 60s'de bir LLM'i çağırırken, **tüm transkripti değil** önceki action_items listesi + son N saniyenin yeni segmentlerini gönder. Model "yeni action item var mı? Mevcut listeyi güncelle" diye prompt'lanır.

**Alternatifler:**
- Her 60s'de full transcript gönder (12K token × 60 = bir saatte 720K token = saçma savurganlık)
- Hiyerarşik özetleme (her 5 dakikada bir özetle, özetleri birleştir — Faz 3 değerlendirmesi)

**Niye:** Lokal LLM kıymetli. Delta ile 60s'lik chunk ≈ 200 token; prompt overhead'le 1K token altı. Saniye sayalı.

**Geri dönüş maliyeti:** Düşük. Prompt iterasyonu konusu, kod düz.

---

## 12. VAD: webrtc-vad ile başla, gerekirse silero-vad'a geç

**Karar:** İlk versiyon `webrtc-vad` (hafif, hızlı, eski). Doğruluk yetersizse `silero-vad` (ONNX, ~12MB ekstra).

**Niye:** webrtc-vad single-call C kütüphanesi, Rust binding'i temiz. Silero ONNX runtime ister, biraz daha ağır ama daha doğru.

**Geri dönüş maliyeti:** Düşük. `vad.rs` interface'i arkasında ya da modül swap. Onnxruntime zaten sidecar'da var (faster-whisper'ın VAD bağımlılığı), kullanılabilir.

---

## 13. Canlı mod chunking stratejisi: VAD-gated, min 3s max 15s

**Karar:** VAD konuşma başını tespit eder, buffer'a alır, N sessiz frame sonra "segment kapalı" der ve Whisper'a yollar. Çıkış kriteri: minimum 3s konuşma, maksimum 15s (uzun monolog'ları kes).

**Alternatifler:** Rolling window (her 30s'de Whisper çağır) — context daha iyi ama 8s latency hedefi tutmaz.

**Niye:** VAD doğal cümle sınırlarında keser → Whisper daha doğru, kullanıcı "canlı" hisseder. 8s hedefi tutar.

**Geri dönüş maliyeti:** Düşük.

---

## 14. SQLite şeması: Faz 0'da tasarla, Faz 1'de implement

**Karar:** Şema [SCHEMA.md](./SCHEMA.md)'de bugün tasarlandı. Implementation Faz 1 sonunda (dosya modu pipeline çalıştığında). Dosya modu ve canlı mod aynı tablolara yazar — Faz 2'de uyumsuzluk çıkmasın.

**Geri dönüş maliyeti:** Orta. Schema migration zor olabilir ama tek-user SQLite, hızlı toparlanır.

---

## 15. Frontend state: useState ile başla, Zustand'a "ihtiyaç olunca" geç

**Karar:** Faz 1 (dosya modu) için `useState` yeter. Faz 2 (canlı mod) event akışı patladığında Zustand ekle.

**Niye:** KISS + rule-of-three. Erken store eklemek = boşa karmaşıklık.

**Geri dönüş maliyeti:** Düşük. Refactor ~yarım gün.

---

## 16. Hata tipleri: `thiserror` modül başına Error enum

**Karar:** Her modül kendi `Error` enum'ını `thiserror::Error` ile tanımlar. Top-level / main'de `anyhow` kullanma sınırlı (sadece kompozisyon noktalarında).

**Niye:** Public API yüzeylerinde explicit hata = caller akıllı recovery yapabilir. Anyhow her yerde = "bir şey oldu, bilmem ne" — production'da yetersiz.

**Geri dönüş maliyeti:** Düşük.

---

## 17. Logging: tracing + tracing-subscriber

**Karar:** `tracing` crate'i + `tracing-subscriber` env_filter. JSON logger şimdilik gerek yok.

**Niye:** Tauri zaten tokio kullanıyor → tracing en doğru fit. Span-aware → async kod debug'ı çok daha kolay.

**Geri dönüş maliyeti:** Düşük.

---

## 18. Performance hedefleri (REVİZE)

ARCHITECTURE.md'deki hedefler "modern 8-core CPU + opsiyonel GPU" varsayımı altında geçerli. Eski/zayıf CPU için bu hedefler tutmaz.

| Metrik | Referans donanım | Hedef | ARCHITECTURE.md'deki eski hedef |
|---|---|---|---|
| 60dk transcript (dosya modu) | i7-12700H + RTX 3050 Ti | <2 dk (GPU), <8 dk (CPU) | <5 dk |
| Canlı mod chunk latency | i7-12700H + RTX 3050 Ti | <3s (GPU), <8s (CPU-only) | <8s |
| Aksiyon çıkarımı (60dk transcript) | i7-12700H | <90s (CPU), <25s (GPU) | <90s |
| Bellek (idle) | — | <300MB | <300MB |
| Bellek (canlı kayıt) | — | <1.5GB (Whisper+Gemma yüklü) | <1GB |
| **Installer boyutu** | — | **~250MB** (PyInstaller bundle) | <50MB (gerçekçi değildi) |
| İlk açılış model indirme | — | ~4GB tek seferlik | (yoktu) |

---

## 19. Test stratejisi (Faz 1'de minimal)

**Karar:**
- Unit test: pure fonksiyonlar (VAD threshold, prompt formatting). Audio/LLM/STT I/O'yu mock'lama.
- Entegrasyon test: 5sn'lik Türkçe WAV ile uçtan uca pipeline, output şeması doğrulanır.
- E2E (Tauri WebDriver): Faz 3'te.

**Test fixture'ları:** Birkaç Türkçe WAV. `test-fixtures/audio/` git LFS değil, **`download-fixtures.ps1` script** ile çekilir (HF dataset veya self-hosted URL'den). `.gitignore`'da `*.wav` zaten ignore.

**Geri dönüş maliyeti:** Düşük.

---

## 20. Türkçe WER darboğazı — model büyütmek yerine LoRA fine-tune (V2+)

**Karar:** V1/V2'de hazır `large-v3-turbo` kullanmaya devam et. Türkçe doğruluk kullanıcı için yetersiz hale gelirse, **model boyutunu büyütmek yerine** Türkçe veri ile LoRA fine-tune yapmaya geç.

### Bağlam

Off-the-shelf large-v3 Türkçe WER tipik olarak ~%12-14 (temiz akademik veri); gürültü/ağız varyasyonunda daha kötü (%18-25). Faz 0 benchmark'ımız bu rakamı sayısallaştırmadı — sadece kalite gözlemi yaptık (large-v3-turbo "iyi", small "kötü"). Türkçe için anlamlı bir WER ölçümü Faz 4 R&D olur.

### Alternatifler

1. **Daha büyük model** (`large-v3` full) — VRAM 1.6GB → 3.2GB, hız %50 daha yavaş, Türkçe WER iyileşmesi belirsiz/küçük
2. **LoRA fine-tune Türkçe veri ile** — raporlarda %50'ye varan WER düşüşü (~%14 → ~%7). Eğitim maliyeti tek seferlik, çıkarım maliyeti aynı, model boyutu marjinal artar (LoRA weights ~50MB)
3. **Whisper distilled Türkçe modeli** — community fine-tune'ları HF Hub'da var; kalite garantisiz, lisans takip et

### Niye LoRA tercih?

- Hesap maliyeti makul (~A100 GPU 24 saat, $50-100 cloud) — kişisel proje için yapılabilir
- WER iyileştirme darboğaza vurur (büyütmek vurmuyor)
- Çıkarım perf hız değişmiyor — kullanıcının makinesi aynı
- Açık kaynak portfolio'ya katkı: "Türkçe Whisper LoRA" kendi başına proje

### Maliyet

- **Veri toplama:** 100-500 saat Türkçe transkript (Common Voice TR ~80 saat + LibriVox TR + kendi kayıtların)
- **Eğitim:** PEFT + transformers, ~24-48 saat A100 (Lambda, Vast.ai, kişisel)
- **Doğrulama:** Holdout test seti üzerinde WER ölç, hazır model ile karşılaştır
- **Toplam:** 2-4 haftalık serious R&D, V1 kullanıcı akışını engellemez

### Ne zaman?

ROADMAP Faz 4+ (opsiyonel). V1 kullanıcı geri bildirimi "doğruluk yetersiz" yönündeyse priorite alır. Şu anki hipotez: large-v3-turbo Türkçe için "yeterince iyi" — kullanıcı raporu olmadan R&D yatırımı yapma.

**Geri dönüş maliyeti:** Eğitilmiş LoRA weights ayrı dosya, fine-tune'u opt-in setting yapabilirsin. Geri çevirilebilir.

---

## Karar değiştirmenin protokolü

Karar değişirse:
1. İlgili kararı **silme** — üstünü çiz (markdown `~~strikethrough~~`) ve "Bkz. Karar X" diye link ver
2. Yeni karar olarak alta ekle (sıradaki numara), tarih düş
3. Aynı commit'te değişiklik kodu ile birlikte gönder (history bütünleşik kalsın)
