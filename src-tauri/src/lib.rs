mod llm;
mod stt;

use std::sync::{Arc, Mutex};

use llm::ExtractionResult;
use stt::{Sidecar, TranscribeResult};

/// `Option` because we shut the sidecar down between transcribe and extract
/// passes to free its CUDA context — Ollama's VRAM probe underreports
/// available memory otherwise on a 4GB VRAM machine. Lazy respawn on next
/// transcribe.
struct AppState {
    sidecar: Arc<Mutex<Option<Sidecar>>>,
}

fn ensure_sidecar(guard: &mut Option<Sidecar>) -> Result<&mut Sidecar, String> {
    if guard.is_none() {
        *guard = Some(Sidecar::spawn().map_err(|e| e.to_string())?);
    }
    Ok(guard.as_mut().expect("just spawned"))
}

/// Transcribe an audio file via the Python sidecar.
#[tauri::command]
async fn transcribe_file(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<TranscribeResult, String> {
    let sidecar = Arc::clone(&state.sidecar);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = sidecar.lock().map_err(|e| e.to_string())?;
        ensure_sidecar(&mut guard)?
            .transcribe(&path)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Extract action items + decisions from a transcript via Ollama (Gemma 3 4B).
///
/// Shuts down the sidecar process first so its CUDA context is released —
/// otherwise Ollama under-detects free VRAM (sees only the leftover, decides
/// to fall back to CPU, then fails for lack of system RAM). The next
/// `transcribe_file` lazily respawns the sidecar (~10s model reload).
#[tauri::command]
async fn extract_actions(
    state: tauri::State<'_, AppState>,
    transcript: String,
) -> Result<ExtractionResult, String> {
    let sidecar = Arc::clone(&state.sidecar);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut guard = sidecar.lock().map_err(|e| e.to_string())?;
        if let Some(sc) = guard.take() {
            sc.shutdown();
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;

    llm::extract_actions(&transcript)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            sidecar: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![transcribe_file, extract_actions])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
