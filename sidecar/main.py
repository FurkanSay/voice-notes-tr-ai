"""Voice Notes TR — Whisper sidecar.

JSON-over-stdio daemon. Rust parent process writes one JSON request per line
to our stdin, we write one JSON response per line to stdout. Anything we want
to log goes to stderr — never stdout (that would corrupt the protocol).

Request shape:  {"id": "...", "method": "ping" | "transcribe", "params": {...}}
Response shape: {"id": "...", "result": {...}} | {"id": "...", "error": {"code": int, "message": str}}
"""
from __future__ import annotations

# Must be imported before anything that loads ctranslate2 (faster_whisper) on Windows.
import _cuda_setup  # noqa: F401

import json
import os
import sys
import traceback
from typing import Any

# faster-whisper is heavy — we lazy-import so that `ping` works instantly
# (useful for the Rust supervisor's healthcheck before model load).
_MODEL = None


def log(msg: str) -> None:
    """Write a line to stderr. Never touch stdout — that's the protocol."""
    print(msg, file=sys.stderr, flush=True)


def get_model():
    """Lazy-load the Whisper model. Reads model name + device from env."""
    global _MODEL
    if _MODEL is not None:
        return _MODEL

    from faster_whisper import WhisperModel

    model_name = os.environ.get("WHISPER_MODEL", "large-v3-turbo")
    device = os.environ.get("WHISPER_DEVICE", "cuda")
    compute_type = os.environ.get(
        "WHISPER_COMPUTE_TYPE",
        "float16" if device == "cuda" else "int8",
    )

    log(f"Loading Whisper model={model_name} device={device} compute_type={compute_type}")
    _MODEL = WhisperModel(model_name, device=device, compute_type=compute_type)
    log("Model loaded.")
    return _MODEL


def handle_ping(_params: dict[str, Any]) -> dict[str, Any]:
    return {"pong": True}


def handle_load_model(_params: dict[str, Any]) -> dict[str, Any]:
    """Pre-warm the Whisper model. Used before live recording so the first
    chunk doesn't stall behind a 10-15s cold-start model load."""
    get_model()
    return {"loaded": True}


def handle_release_model(_params: dict[str, Any]) -> dict[str, Any]:
    """Drop the loaded Whisper model — frees both VRAM and RAM.

    Needed because Whisper + Gemma together exceed our budget on a 4GB VRAM /
    16GB RAM machine. The next `transcribe` reloads the model (~10s penalty).
    See DECISIONS.md #8 (GPU/CPU stratejisi).
    """
    global _MODEL
    released = _MODEL is not None
    if released:
        log("Releasing Whisper model from memory")
        _MODEL = None  # drop reference
        import gc
        gc.collect()
        try:
            import torch  # noqa: PLC0415 — optional
            if torch.cuda.is_available():
                torch.cuda.empty_cache()
        except ImportError:
            pass
    return {"released": released}


def handle_transcribe(params: dict[str, Any]) -> dict[str, Any]:
    """Transcribe an audio file. Returns list of segments + detected language."""
    audio_path = params["audio_path"]
    language = params.get("language", "tr")
    beam_size = int(params.get("beam_size", 5))
    vad_filter = bool(params.get("vad_filter", True))

    model = get_model()
    segments_iter, info = model.transcribe(
        audio_path,
        language=language,
        beam_size=beam_size,
        vad_filter=vad_filter,
    )
    segments = [
        {"start": s.start, "end": s.end, "text": s.text}
        for s in segments_iter
    ]
    return {
        "language": info.language,
        "language_probability": info.language_probability,
        "duration": info.duration,
        "segments": segments,
    }


def handle_transcribe_pcm(params: dict[str, Any]) -> dict[str, Any]:
    """Transcribe raw PCM bytes (live recording chunks).

    Expects:
      - pcm_b64: base64-encoded little-endian int16 mono samples at 16kHz
      - language: ISO code (default "tr")
    Returns:
      - language, language_probability, duration, segments (start/end in s)
    """
    import base64
    import numpy as np

    pcm_b64 = params["pcm_b64"]
    language = params.get("language", "tr")

    raw = base64.b64decode(pcm_b64)
    # int16 little-endian → float32 in [-1, 1]
    pcm_i16 = np.frombuffer(raw, dtype=np.int16)
    audio = pcm_i16.astype(np.float32) / 32768.0

    model = get_model()
    # vad_filter=True: Rust tarafı zaten webrtc-vad ile chunk sınırını çıkardı,
    # ama faster-whisper'ın dahili Silero VAD'ı chunk kenarındaki gürültüyü
    # ve sessiz kuyrukları temizleyerek Whisper'ın halüsinasyonunu önler
    # ("dinlediğiniz için teşekkürler" uydurması, vb.). Maliyet ~50-100ms/chunk.
    segments_iter, info = model.transcribe(
        audio,
        language=language,
        beam_size=5,
        vad_filter=True,
    )
    segments = [
        {"start": s.start, "end": s.end, "text": s.text}
        for s in segments_iter
    ]
    return {
        "language": info.language,
        "language_probability": info.language_probability,
        "duration": info.duration,
        "segments": segments,
    }


METHODS = {
    "ping": handle_ping,
    "load_model": handle_load_model,
    "transcribe": handle_transcribe,
    "transcribe_pcm": handle_transcribe_pcm,
    "release_model": handle_release_model,
}


def dispatch(request: dict[str, Any]) -> dict[str, Any]:
    req_id = request.get("id")
    method_name = request.get("method")
    params = request.get("params", {}) or {}

    handler = METHODS.get(method_name)
    if handler is None:
        return {"id": req_id, "error": {"code": -32601, "message": f"Unknown method: {method_name}"}}

    try:
        result = handler(params)
        return {"id": req_id, "result": result}
    except Exception as e:  # noqa: BLE001 — sidecar must never die on a bad request
        log(f"Handler error for {method_name}: {e}\n{traceback.format_exc()}")
        return {"id": req_id, "error": {"code": -32000, "message": str(e)}}


def main() -> None:
    log("Sidecar starting. Waiting for requests on stdin.")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as e:
            response = {"id": None, "error": {"code": -32700, "message": f"Parse error: {e}"}}
        else:
            response = dispatch(request)

        sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
