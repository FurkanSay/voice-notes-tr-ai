//! Live microphone capture → 16kHz mono i16 → VAD-gated chunks.
//!
//! Pipeline (all on dedicated threads, never touches Tauri's UI thread):
//!
//!   cpal callback ─[f32 native rate, N channels]─► producer thread
//!       │                                              │
//!       │                                              ▼
//!       │                                       mono mix + rubato resample
//!       │                                              │
//!       │                                              ▼
//!       │                                       crossbeam channel ──► VAD/chunker
//!       │                                                                │
//!       │                                                                ▼
//!       │                                                       Vec<i16> chunk
//!       └─ also pushes mean-abs level samples ──► level channel
//!
//! VAD state machine: IDLE → SPEAKING → MAYBE_END → emit chunk → IDLE.
//! Chunk constraints: min 1.5s of speech, max 15s total. Silence threshold
//! 600ms before closing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use thiserror::Error;
use webrtc_vad::{SampleRate as VadSampleRate, Vad, VadMode};

pub const TARGET_RATE: u32 = 16_000;
const VAD_FRAME_MS: u32 = 20;
const VAD_FRAME_SAMPLES: usize = (TARGET_RATE / 1000 * VAD_FRAME_MS) as usize; // 320

const MIN_CHUNK_SAMPLES: usize = TARGET_RATE as usize * 3 / 2; // 1.5s
const MAX_CHUNK_SAMPLES: usize = TARGET_RATE as usize * 8; // 8s — sürekli konuşmada periyodik segment için
const SILENCE_FRAMES_TO_CLOSE: usize = 20; // 20 × 20ms = 400ms

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no input device found")]
    NoDevice,
    #[error("default input config error: {0}")]
    DefaultConfig(#[from] cpal::DefaultStreamConfigError),
    #[error("stream build error: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),
    #[error("stream play error: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
    #[error("rubato error: {0}")]
    Resampler(#[from] rubato::ResamplerConstructionError),
    #[error("vad init error: {0}")]
    VadInit(String),
}

/// What the chunker emits to its consumer.
pub enum AudioEvent {
    /// 16kHz mono i16 chunk of detected speech.
    Chunk { samples: Vec<i16>, duration_ms: u32 },
    /// 0.0..=1.0 audio level for the UI VU meter (sampled ~30Hz).
    Level(f32),
}

/// Handle to a running recording session. The event receiver is returned
/// separately by `start` so there's a single consumer.
pub struct Recorder {
    _stream: Stream,
    stop_flag: Arc<AtomicBool>,
}

impl Recorder {
    /// Open default input device, start capturing. Returns (Recorder, events).
    /// Drop the Recorder to stop capture; the processor thread will then close
    /// the events channel.
    pub fn start() -> Result<(Self, Receiver<AudioEvent>), AudioError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(AudioError::NoDevice)?;
        let supported = device.default_input_config()?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        let in_rate = config.sample_rate.0;
        let channels = config.channels as usize;

        let (raw_tx, raw_rx) = bounded::<Vec<f32>>(64); // small backlog OK
        let (out_tx, out_rx) = bounded::<AudioEvent>(64);
        let stop_flag = Arc::new(AtomicBool::new(false));

        let stream = build_input_stream(&device, &config, sample_format, raw_tx)?;
        stream.play()?;

        spawn_processor(raw_rx, out_tx, in_rate, channels, Arc::clone(&stop_flag));

        Ok((
            Self {
                _stream: stream,
                stop_flag,
            },
            out_rx,
        ))
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
    }
}

fn build_input_stream(
    device: &Device,
    config: &StreamConfig,
    fmt: SampleFormat,
    tx: Sender<Vec<f32>>,
) -> Result<Stream, AudioError> {
    let err_fn = |e| tracing::error!("cpal stream error: {e}");

    // Per-arm closure receives its own clone of the sender. cpal's callback
    // runs on the audio driver thread; never block, never allocate beyond
    // this small Vec push.
    let stream = match fmt {
        SampleFormat::F32 => {
            let tx = tx;
            device.build_input_stream(
                config,
                move |d: &[f32], _| {
                    if let Err(TrySendError::Full(_)) = tx.try_send(d.to_vec()) {
                        tracing::warn!("audio channel full, dropping frame");
                    }
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::I16 => {
            let tx = tx;
            device.build_input_stream(
                config,
                move |d: &[i16], _| {
                    let f: Vec<f32> = d.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                    let _ = tx.try_send(f);
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::U16 => {
            let tx = tx;
            device.build_input_stream(
                config,
                move |d: &[u16], _| {
                    let f: Vec<f32> = d
                        .iter()
                        .map(|s| (*s as f32 - 32768.0) / 32768.0)
                        .collect();
                    let _ = tx.try_send(f);
                },
                err_fn,
                None,
            )?
        }
        other => {
            return Err(AudioError::VadInit(format!(
                "unsupported sample format: {other:?}"
            )));
        }
    };
    Ok(stream)
}

fn spawn_processor(
    raw_rx: Receiver<Vec<f32>>,
    out_tx: Sender<AudioEvent>,
    in_rate: u32,
    channels: usize,
    stop_flag: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name("audio-processor".into())
        .spawn(move || {
            if let Err(e) = run_processor(raw_rx, out_tx, in_rate, channels, stop_flag) {
                tracing::error!("audio processor exited: {e}");
            }
        })
        .expect("spawn audio thread");
}

fn run_processor(
    raw_rx: Receiver<Vec<f32>>,
    out_tx: Sender<AudioEvent>,
    in_rate: u32,
    channels: usize,
    stop_flag: Arc<AtomicBool>,
) -> Result<(), AudioError> {
    let need_resample = in_rate != TARGET_RATE;
    let resample_ratio = TARGET_RATE as f64 / in_rate as f64;

    // rubato sinc resampler — high quality, mono only here
    let mut resampler: Option<SincFixedIn<f32>> = if need_resample {
        let params = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 64,
            window: WindowFunction::Blackman,
        };
        Some(SincFixedIn::new(resample_ratio, 2.0, params, 1024, 1)?)
    } else {
        None
    };

    // VeryAggressive filtering: arkaplan gürültüsü 'silence' sayılır, kullanıcının
    // konuşması yine kolayca 'voice' yakalanır. Gürültülü ortamlarda Quality
    // modu yanlışlıkla ambient sesi voice olarak işaretliyor → chunk hiç kapanmıyor.
    let mut vad = Vad::new_with_rate_and_mode(VadSampleRate::Rate16kHz, VadMode::VeryAggressive);

    let mut pending_16k: Vec<i16> = Vec::with_capacity(VAD_FRAME_SAMPLES * 8);
    let mut chunk_buf: Vec<i16> = Vec::with_capacity(MAX_CHUNK_SAMPLES);
    let mut state = ChunkState::Idle;
    let mut silence_run = 0usize;

    // Level meter: emit ~30Hz so the UI animates smoothly
    let level_interval_samples = TARGET_RATE as usize / 30;
    let mut level_samples_seen = 0usize;
    let mut level_sum_abs: f32 = 0.0;
    let mut level_count: usize = 0;

    while !stop_flag.load(Ordering::Relaxed) {
        let frame = match raw_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(f) => f,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };

        // Downmix to mono
        let mono: Vec<f32> = if channels == 1 {
            frame
        } else {
            frame
                .chunks(channels)
                .map(|c| c.iter().sum::<f32>() / channels as f32)
                .collect()
        };

        // Resample to 16kHz (if needed)
        let f32_16k: Vec<f32> = if let Some(rs) = resampler.as_mut() {
            // rubato SincFixedIn wants a fixed input length (1024 here). Feed
            // it in 1024-sample chunks, buffer the remainder for next round.
            let mut tail = mono;
            let mut out = Vec::with_capacity(tail.len() * 2);
            let chunk_size = 1024;
            // Stash incomplete tail in a static-ish way via re-use
            static_buffer_resample(rs, &mut tail, chunk_size, &mut out);
            out
        } else {
            mono
        };

        // Convert to i16
        let i16_samples: Vec<i16> = f32_16k
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();

        // Update VU meter (running average of absolute values)
        for &s in &i16_samples {
            level_sum_abs += (s as f32).abs() / i16::MAX as f32;
            level_count += 1;
            level_samples_seen += 1;
            if level_samples_seen >= level_interval_samples {
                let level = (level_sum_abs / level_count as f32).min(1.0);
                let _ = out_tx.try_send(AudioEvent::Level(level));
                level_sum_abs = 0.0;
                level_count = 0;
                level_samples_seen = 0;
            }
        }

        pending_16k.extend_from_slice(&i16_samples);

        // Run VAD on each complete 20ms frame
        while pending_16k.len() >= VAD_FRAME_SAMPLES {
            let frame: Vec<i16> = pending_16k.drain(..VAD_FRAME_SAMPLES).collect();
            let is_voice = vad.is_voice_segment(&frame).unwrap_or(false);

            match state {
                ChunkState::Idle => {
                    if is_voice {
                        state = ChunkState::Speaking;
                        chunk_buf.clear();
                        chunk_buf.extend_from_slice(&frame);
                        silence_run = 0;
                    }
                }
                ChunkState::Speaking => {
                    chunk_buf.extend_from_slice(&frame);
                    if is_voice {
                        silence_run = 0;
                    } else {
                        silence_run = 1;
                        state = ChunkState::MaybeEnd;
                    }
                    if chunk_buf.len() >= MAX_CHUNK_SAMPLES {
                        emit_chunk(&out_tx, &mut chunk_buf);
                        state = ChunkState::Idle;
                        silence_run = 0;
                    }
                }
                ChunkState::MaybeEnd => {
                    chunk_buf.extend_from_slice(&frame);
                    if is_voice {
                        state = ChunkState::Speaking;
                        silence_run = 0;
                    } else {
                        silence_run += 1;
                        if silence_run >= SILENCE_FRAMES_TO_CLOSE {
                            if chunk_buf.len() >= MIN_CHUNK_SAMPLES {
                                emit_chunk(&out_tx, &mut chunk_buf);
                            } else {
                                // Too short to bother transcribing
                                chunk_buf.clear();
                            }
                            state = ChunkState::Idle;
                            silence_run = 0;
                        }
                    }
                    if chunk_buf.len() >= MAX_CHUNK_SAMPLES {
                        emit_chunk(&out_tx, &mut chunk_buf);
                        state = ChunkState::Idle;
                        silence_run = 0;
                    }
                }
            }
        }
    }

    // On stop: if we were mid-utterance, emit what we have
    if !chunk_buf.is_empty() && chunk_buf.len() >= MIN_CHUNK_SAMPLES {
        emit_chunk(&out_tx, &mut chunk_buf);
    }

    Ok(())
}

fn emit_chunk(tx: &Sender<AudioEvent>, buf: &mut Vec<i16>) {
    let samples: Vec<i16> = std::mem::take(buf);
    let duration_ms = (samples.len() as u32 * 1000) / TARGET_RATE;
    eprintln!("[audio] EMIT chunk: {} samples ({} ms)", samples.len(), duration_ms);
    let _ = tx.try_send(AudioEvent::Chunk {
        samples,
        duration_ms,
    });
}

#[derive(Debug, Clone, Copy)]
enum ChunkState {
    Idle,
    Speaking,
    MaybeEnd,
}

/// Push `tail` into `rs` in fixed-size blocks, accumulating output into `out`.
/// Leftover tail (<chunk_size) is appended to a thread-local carry buffer.
fn static_buffer_resample(
    rs: &mut SincFixedIn<f32>,
    tail: &mut Vec<f32>,
    chunk_size: usize,
    out: &mut Vec<f32>,
) {
    use std::cell::RefCell;
    thread_local! {
        static CARRY: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    }

    CARRY.with(|carry| {
        let mut carry = carry.borrow_mut();
        carry.append(tail);
        while carry.len() >= chunk_size {
            let block: Vec<f32> = carry.drain(..chunk_size).collect();
            let input = vec![block];
            if let Ok(resampled) = rs.process(&input, None) {
                out.extend_from_slice(&resampled[0]);
            }
        }
    });
}
