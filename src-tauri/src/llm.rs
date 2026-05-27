//! Ollama HTTP client for action item + decision extraction.
//!
//! Talks to the locally running Ollama daemon on port 11434. Uses Gemma 3 4B
//! (Q4_K_M) in JSON-constrained mode so the response is always valid JSON.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
const MODEL: &str = "gemma3:4b";
const KEEP_ALIVE: &str = "0s";

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("ollama returned malformed JSON: {raw}")]
    BadResponse { raw: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActionItem {
    pub text: String,
    #[serde(default)]
    pub assignee: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ExtractionResult {
    #[serde(default)]
    pub actions: Vec<ActionItem>,
    #[serde(default)]
    pub decisions: Vec<String>,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
    format: &'a str,
    options: GenerateOptions,
    /// "0s" tells Ollama to drop the model immediately after the response.
    /// Otherwise it caches Gemma for ~5 min, which on a 16GB RAM / 4GB VRAM
    /// machine prevents the next Whisper transcribe from finding memory.
    /// See DECISIONS.md #8.
    keep_alive: &'a str,
}

#[derive(Serialize)]
struct GenerateOptions {
    temperature: f32,
    /// Force Ollama to put the model on GPU. We've just shut down the sidecar
    /// (which freed ~1.6GB VRAM); free system RAM might still be tight, so
    /// without this Ollama falls back to CPU and dies on `mkl_malloc`. 99 =
    /// "as many layers as fit", which for Gemma 3 4B Q4 is the whole model.
    num_gpu: i32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

pub async fn extract_actions(transcript: &str) -> Result<ExtractionResult, LlmError> {
    let req = GenerateRequest {
        model: MODEL,
        prompt: build_prompt(transcript),
        stream: false,
        format: "json", // Ollama'nın JSON modu — sözdizimi garantili
        options: GenerateOptions {
            temperature: 0.2,
            num_gpu: 99,
        },
        keep_alive: KEEP_ALIVE,
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    let raw: GenerateResponse = client
        .post(OLLAMA_URL)
        .json(&req)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    serde_json::from_str(&raw.response).map_err(|_| LlmError::BadResponse { raw: raw.response })
}

fn build_prompt(transcript: &str) -> String {
    format!(
        r#"Aşağıdaki Türkçe toplantı transkriptinden alınması gereken aksiyon maddelerini ve verilen kararları JSON formatında çıkar.

Tanımlar:
- Aksiyon maddesi: gelecekte bir kişinin yapması gereken iş ("raporu cuma günü gönder", "slaytları hazırla").
- Karar: toplantıda netleştirilmiş bir sonuç ("Bütçe 50 bin lira olarak onaylandı", "X kütüphanesini kullanacağız").
- Plan veya öneri AKSIYON ya da KARAR DEĞİLDİR (örn. "haftaya konuşuruz" karar değildir).

Kişi adı geçiyorsa assignee alanına yaz, geçmiyorsa null bırak.
Hiçbir aksiyon veya karar yoksa boş liste döndür.
Sadece JSON döndür, başka açıklama yapma.

Format:
{{
  "actions": [{{"text": "...", "assignee": "..." veya null}}],
  "decisions": ["..."]
}}

Transkript:
{}"#,
        transcript
    )
}
