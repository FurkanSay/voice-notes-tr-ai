# Sidecar — faster-whisper

Türkçe transkript için Whisper'ı çalıştıran Python daemon'u. Rust parent process JSON-over-stdio ile konuşur.

## Kurulum (dev)

```powershell
cd sidecar
uv venv --python 3.10
uv pip install --python .venv\Scripts\python.exe faster-whisper
```

### GPU (CUDA) için ek paketler — Windows

ctranslate2 4.x Windows'ta nvidia-cublas-cu12 / nvidia-cudnn-cu12 paketlerinin
DLL'lerini otomatik PATH'e eklemiyor. İki şey gerek:

```powershell
uv pip install --python .venv\Scripts\python.exe nvidia-cublas-cu12 'nvidia-cudnn-cu12>=9.0,<10.0'
```

`_cuda_setup.py` modülü import edildiğinde bu DLL'leri runtime'da yükler.
`main.py` ve `benchmark.py` başında zaten import ediliyor — manuel bir şey
yapmana gerek yok.

Linux'ta bu paketler genellikle CTranslate2 wheel'iyle birlikte ya da
sistem CUDA toolkit'i ile sağlanır; `_cuda_setup.py` Linux'ta no-op.

## Çalıştırma (dev)

```powershell
.\.venv\Scripts\python.exe main.py
```

Sonra stdin'e satır olarak JSON gönderin:

```json
{"id":"1","method":"ping"}
{"id":"2","method":"transcribe","params":{"audio_path":"C:\\path\\to\\sample.wav"}}
```

## Environment

| Değişken | Default | Notu |
|---|---|---|
| `WHISPER_MODEL` | `large-v3-turbo` | `tiny`, `small`, `medium`, `large-v3-turbo` |
| `WHISPER_DEVICE` | `cuda` | `cuda` veya `cpu` |
| `WHISPER_COMPUTE_TYPE` | `float16` (cuda), `int8` (cpu) | `int8`, `int8_float16`, `float16`, `float32` |

## Protokol

- **stdin / stdout** — line-delimited JSON, her satır bir mesaj.
- **stderr** — log. Stdout'a yazma asla.
- Request: `{"id": str, "method": str, "params": object}`
- Response: `{"id": str, "result": object}` ya da `{"id": str, "error": {"code": int, "message": str}}`

Hata kodları (JSON-RPC tarzı):
- `-32700` parse error
- `-32601` unknown method
- `-32000` handler exception

## Build (PyInstaller)

Production'da Tauri `externalBin` ile bundle edilir:

```powershell
.\.venv\Scripts\pyinstaller.exe --onefile --name whisper-sidecar-x86_64-pc-windows-msvc main.py
```

Çıktı `dist/` altına gider, Tauri build sırasında `src-tauri/binaries/` altına kopyalanır.
