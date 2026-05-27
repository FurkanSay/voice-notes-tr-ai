mod audio;
mod llm;
mod stt;

use std::sync::{Arc, Mutex};

use crossbeam_channel::{bounded, Receiver, Sender};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use audio::{AudioEvent, Recorder};
use llm::ExtractionResult;
use stt::{Segment, Sidecar, TranscribeResult};

/// Send+Sync — the cpal Stream lives inside the recording thread, never here.
struct RecordingHandle {
    /// Drop sender → receiver disconnects → host thread exits cleanly.
    stop: Sender<()>,
}

struct AppState {
    sidecar: Arc<Mutex<Option<Sidecar>>>,
    recording: Arc<Mutex<Option<RecordingHandle>>>,
}

fn ensure_sidecar(guard: &mut Option<Sidecar>) -> Result<&mut Sidecar, String> {
    if guard.is_none() {
        *guard = Some(Sidecar::spawn().map_err(|e| e.to_string())?);
    }
    Ok(guard.as_mut().expect("just spawned"))
}

#[derive(Serialize, Clone)]
struct LiveSegment {
    offset_s: f64,
    duration_s: f64,
    text: String,
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

/// Extract action items + decisions via Ollama. Shuts down the sidecar first
/// to release its CUDA context (so Ollama can use the full GPU).
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

/// Start microphone capture. Spawns a host thread that owns the cpal Stream
/// (which is `!Send` on Windows) and emits transcription events.
#[tauri::command]
async fn start_recording(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut g = state.recording.lock().map_err(|e| e.to_string())?;
    if g.is_some() {
        return Err("zaten kayıtta".into());
    }

    let (stop_tx, stop_rx) = bounded::<()>(1);
    let sidecar = Arc::clone(&state.sidecar);

    std::thread::Builder::new()
        .name("recording-host".into())
        .spawn(move || run_recording_host(app, sidecar, stop_rx))
        .map_err(|e| e.to_string())?;

    *g = Some(RecordingHandle { stop: stop_tx });
    Ok(())
}

#[tauri::command]
async fn stop_recording(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut g = state.recording.lock().map_err(|e| e.to_string())?;
    if let Some(h) = g.take() {
        let _ = h.stop.send(());
    }
    Ok(())
}

/// Recording thread main loop. Owns the cpal Stream (via Recorder).
/// Consumes audio events, transcribes chunks, emits Tauri events.
fn run_recording_host(
    app: AppHandle,
    sidecar_state: Arc<Mutex<Option<Sidecar>>>,
    stop_rx: Receiver<()>,
) {
    let (recorder, events) = match Recorder::start() {
        Ok(pair) => pair,
        Err(e) => {
            let _ = app.emit("recording:error", e.to_string());
            return;
        }
    };

    let mut total_offset_s = 0.0_f64;

    loop {
        crossbeam_channel::select! {
            recv(events) -> ev_res => {
                let ev = match ev_res {
                    Ok(e) => e,
                    Err(_) => break,
                };
                match ev {
                    AudioEvent::Level(level) => {
                        let _ = app.emit("recording:level", level);
                    }
                    AudioEvent::Chunk { samples, duration_ms } => {
                        let dur_s = duration_ms as f64 / 1000.0;
                        let start_offset = total_offset_s;
                        total_offset_s += dur_s;

                        let result = {
                            let mut guard = match sidecar_state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    tracing::error!("sidecar mutex poisoned: {e}");
                                    continue;
                                }
                            };
                            let sc = match ensure_sidecar(&mut guard) {
                                Ok(sc) => sc,
                                Err(e) => {
                                    let _ = app.emit("recording:error", e);
                                    continue;
                                }
                            };
                            sc.transcribe_pcm(&samples)
                        };

                        match result {
                            Ok(r) => {
                                let text = join_segments(&r.segments);
                                if !text.trim().is_empty() {
                                    let _ = app.emit("recording:segment", LiveSegment {
                                        offset_s: start_offset,
                                        duration_s: dur_s,
                                        text,
                                    });
                                }
                            }
                            Err(e) => {
                                let _ = app.emit("recording:error", e.to_string());
                            }
                        }
                    }
                }
            }
            recv(stop_rx) -> _ => break,
        }
    }

    drop(recorder); // explicit; Stream dropped here, cpal stops
}

fn join_segments(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| s.text.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            sidecar: Arc::new(Mutex::new(None)),
            recording: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            transcribe_file,
            extract_actions,
            start_recording,
            stop_recording,
        ])
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
