"""Whisper model benchmark for Turkish audio.

Aynı ses dosyasını birden çok Whisper modelinde koşturup karşılaştırma çıkarır.
Karar: DECISIONS.md #9 (Whisper model seçimi).

Kullanım:
    python benchmark.py path/to/turkish_sample.wav
    python benchmark.py path/to/turkish_sample.wav --models tiny,small,medium
    python benchmark.py path/to/turkish_sample.wav --device cpu

Çıktı: süre, ses uzunluğu, hız oranı (xRT), tahmini text karşılaştırması.
"""
from __future__ import annotations

# Must be imported before anything that loads ctranslate2 on Windows.
import _cuda_setup  # noqa: F401

import argparse
import gc
import json
import sys
import time
from pathlib import Path


def bench_one(audio_path: str, model_name: str, device: str, compute_type: str) -> dict:
    """Load model, transcribe, return timing + text. Frees model on exit."""
    from faster_whisper import WhisperModel

    print(f"\n=== {model_name} ({device}, {compute_type}) ===", file=sys.stderr)

    load_t0 = time.time()
    model = WhisperModel(model_name, device=device, compute_type=compute_type)
    load_dur = time.time() - load_t0
    print(f"  load: {load_dur:.1f}s", file=sys.stderr)

    transcribe_t0 = time.time()
    segments_iter, info = model.transcribe(
        audio_path,
        language="tr",
        beam_size=5,
        vad_filter=True,
    )
    segments = list(segments_iter)
    transcribe_dur = time.time() - transcribe_t0
    print(f"  transcribe: {transcribe_dur:.1f}s for {info.duration:.1f}s audio "
          f"({info.duration / transcribe_dur:.1f}xRT)", file=sys.stderr)

    text = " ".join(s.text.strip() for s in segments)

    # free model + cuda cache
    del model
    gc.collect()
    try:
        import torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except ImportError:
        pass

    return {
        "model": model_name,
        "device": device,
        "compute_type": compute_type,
        "load_s": round(load_dur, 2),
        "transcribe_s": round(transcribe_dur, 2),
        "audio_duration_s": round(info.duration, 2),
        "xRT": round(info.duration / transcribe_dur, 2),
        "language_probability": round(info.language_probability, 3),
        "segment_count": len(segments),
        "text": text,
        "text_chars": len(text),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Whisper TR benchmark")
    parser.add_argument("audio", help="Path to Turkish WAV/MP3 file")
    parser.add_argument(
        "--models",
        default="small,medium,large-v3-turbo",
        help="Comma-separated model names (default: small,medium,large-v3-turbo)",
    )
    parser.add_argument("--device", default="cuda", choices=["cuda", "cpu"])
    parser.add_argument(
        "--compute-type",
        default=None,
        help="Override compute type (default: float16 on cuda, int8 on cpu)",
    )
    parser.add_argument("--output", default=None, help="Save JSON results to file")
    args = parser.parse_args()

    if not Path(args.audio).exists():
        sys.exit(f"Audio file not found: {args.audio}")

    compute_type = args.compute_type or ("float16" if args.device == "cuda" else "int8")
    models = [m.strip() for m in args.models.split(",") if m.strip()]

    results = []
    for model_name in models:
        try:
            result = bench_one(args.audio, model_name, args.device, compute_type)
            results.append(result)
        except Exception as e:  # noqa: BLE001
            print(f"  ERROR: {e}", file=sys.stderr)
            results.append({"model": model_name, "error": str(e)})

    # Tablo özeti — stderr'a, JSON stdout'a (pipe-friendly)
    print("\n=== Summary ===", file=sys.stderr)
    print(f"{'Model':<22} {'Load':>8} {'Time':>8} {'xRT':>6} {'Chars':>8}", file=sys.stderr)
    for r in results:
        if "error" in r:
            print(f"{r['model']:<22} ERROR: {r['error']}", file=sys.stderr)
        else:
            print(
                f"{r['model']:<22} {r['load_s']:>7.1f}s {r['transcribe_s']:>7.1f}s "
                f"{r['xRT']:>5.1f}x {r['text_chars']:>8}",
                file=sys.stderr,
            )

    output_json = json.dumps(results, ensure_ascii=False, indent=2)
    if args.output:
        Path(args.output).write_text(output_json, encoding="utf-8")
        print(f"\nResults written to {args.output}", file=sys.stderr)
    else:
        print(output_json)


if __name__ == "__main__":
    main()
