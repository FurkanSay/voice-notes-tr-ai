"""Make NVIDIA CUDA runtime DLLs discoverable on Windows.

ctranslate2 4.x'in Windows __init__'i sadece kendi paket dizinini DLL search'e
ekliyor — pip ile gelen nvidia-cublas-cu12 / nvidia-cudnn-cu12 paketlerinin
DLL'leri site-packages/nvidia/*/bin altında kaldığı için bulunamıyor ve
"Library cublas64_12.dll is not found" hatası alıyoruz.

İki katmanlı çözüm:
1) os.add_dll_directory ile DLL search yoluna ekle (Python 3.8+)
2) cuBLAS/cuDNN DLL'lerini ctypes.WinDLL ile preload et — ctranslate2 daha
   sonra LoadLibrary çağırdığında bu sembolleri zaten process'e bağlı bulur.

Bu modülü herhangi bir ctranslate2 import'undan ÖNCE import et. Linux'ta no-op.
"""
from __future__ import annotations

import os
import sys
import ctypes
import importlib.util


# cuBLAS önce, cuDNN onun üstüne (cuDNN cuBLAS'a bağımlı).
_PRELOAD_DLLS = [
    ("cublas", "cublas64_12.dll"),
    ("cublas", "cublasLt64_12.dll"),
    ("cuda_nvrtc", "nvrtc64_120_0.dll"),
    ("cudnn", "cudnn64_9.dll"),
]


def _setup() -> None:
    if sys.platform != "win32":
        return

    spec = importlib.util.find_spec("nvidia")
    if spec is None or not spec.submodule_search_locations:
        return

    nvidia_root = spec.submodule_search_locations[0]
    if not os.path.isdir(nvidia_root):
        return

    # Step 1 — add nvidia/*/bin to DLL search path
    for sub in os.listdir(nvidia_root):
        bin_dir = os.path.join(nvidia_root, sub, "bin")
        if os.path.isdir(bin_dir):
            try:
                os.add_dll_directory(bin_dir)
            except OSError:
                pass

    # Step 2 — preload critical DLLs (best-effort, silent on miss)
    for subpkg, dll_name in _PRELOAD_DLLS:
        dll_path = os.path.join(nvidia_root, subpkg, "bin", dll_name)
        if os.path.isfile(dll_path):
            try:
                ctypes.WinDLL(dll_path)
            except OSError:
                pass


_setup()
