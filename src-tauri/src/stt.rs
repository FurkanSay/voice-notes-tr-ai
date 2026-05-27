//! Python sidecar IPC for speech-to-text.
//!
//! Spawns the faster-whisper Python process and talks to it via
//! line-delimited JSON on stdin/stdout. See sidecar/main.py.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SttError {
    #[error("sidecar Python interpreter not found at {0}")]
    PythonMissing(PathBuf),
    #[error("sidecar entry script not found at {0}")]
    ScriptMissing(PathBuf),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sidecar stdout closed unexpectedly")]
    Eof,
    #[error("sidecar pipe not available")]
    NoPipe,
    #[error("sidecar returned error: {code} {message}")]
    Remote { code: i32, message: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscribeResult {
    pub language: String,
    pub language_probability: f64,
    pub duration: f64,
    pub segments: Vec<Segment>,
}

pub struct Sidecar {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Sidecar {
    /// Spawn the sidecar Python process.
    ///
    /// Dev mode only: uses the venv at ../sidecar/.venv. Production bundling
    /// (PyInstaller exe + Tauri externalBin) is a Faz 3 concern.
    pub fn spawn() -> Result<Self, SttError> {
        let python = dev_python_path();
        if !python.exists() {
            return Err(SttError::PythonMissing(python));
        }
        let script = dev_script_path();
        if !script.exists() {
            return Err(SttError::ScriptMissing(script));
        }

        let mut child = Command::new(&python)
            .arg(&script)
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().ok_or(SttError::NoPipe)?;
        let stdout = BufReader::new(child.stdout.take().ok_or(SttError::NoPipe)?);

        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 1,
        })
    }

    /// Shut down the sidecar process and free its CUDA context.
    ///
    /// Needed before invoking Ollama on a low-VRAM machine — even with the
    /// Whisper model unloaded, the Python process retains a CUDA context that
    /// makes Ollama's free-VRAM probe under-report available memory and fall
    /// back to CPU (which then fails for lack of system RAM). See
    /// DECISIONS.md #8.
    pub fn shutdown(mut self) {
        // Drop stdin → EOF → Python's `for line in sys.stdin` loop exits cleanly.
        drop(self.stdin.take());
        // Brief grace period for clean exit.
        for _ in 0..10 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Send a request and block until the response arrives.
    fn request<P: Serialize, R: for<'de> Deserialize<'de>>(
        &mut self,
        method: &str,
        params: P,
    ) -> Result<R, SttError> {
        let id = self.next_id.to_string();
        self.next_id += 1;

        let req = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });

        let stdin = self.stdin.as_mut().ok_or(SttError::NoPipe)?;
        writeln!(stdin, "{}", req)?;
        stdin.flush()?;

        let mut line = String::new();
        let n = self.stdout.read_line(&mut line)?;
        if n == 0 {
            return Err(SttError::Eof);
        }

        let resp: serde_json::Value = serde_json::from_str(&line)?;
        if let Some(err) = resp.get("error") {
            let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("(no message)")
                .to_string();
            return Err(SttError::Remote { code, message });
        }

        let result = resp
            .get("result")
            .cloned()
            .ok_or_else(|| SttError::Remote {
                code: 0,
                message: "missing result field".into(),
            })?;
        Ok(serde_json::from_value(result)?)
    }

    pub fn ping(&mut self) -> Result<bool, SttError> {
        let r: serde_json::Value = self.request("ping", serde_json::json!({}))?;
        Ok(r.get("pong").and_then(|v| v.as_bool()).unwrap_or(false))
    }

    /// Pre-warm the Whisper model so the first transcribe doesn't pay the
    /// 10-15s cold-start. Use before starting live recording.
    pub fn load_model(&mut self) -> Result<(), SttError> {
        let _: serde_json::Value = self.request("load_model", serde_json::json!({}))?;
        Ok(())
    }

    pub fn transcribe(&mut self, audio_path: &str) -> Result<TranscribeResult, SttError> {
        self.request(
            "transcribe",
            serde_json::json!({
                "audio_path": audio_path,
                "language": "tr",
            }),
        )
    }

    /// Transcribe raw PCM (16kHz mono i16) — for live recording chunks.
    pub fn transcribe_pcm(&mut self, pcm: &[i16]) -> Result<TranscribeResult, SttError> {
        use base64::Engine;
        // i16 LE → bytes
        let mut bytes = Vec::with_capacity(pcm.len() * 2);
        for &s in pcm {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let pcm_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        self.request(
            "transcribe_pcm",
            serde_json::json!({
                "pcm_b64": pcm_b64,
                "language": "tr",
            }),
        )
    }

    /// Free the Whisper model from RAM/VRAM. Lazy reload on next `transcribe`.
    pub fn release_model(&mut self) -> Result<(), SttError> {
        let _: serde_json::Value = self.request("release_model", serde_json::json!({}))?;
        Ok(())
    }
}

fn dev_python_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sidecar")
        .join(".venv")
        .join("Scripts")
        .join("python.exe")
}

fn dev_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sidecar")
        .join("main.py")
}
