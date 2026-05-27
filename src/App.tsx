import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

interface DroppedFile {
  path: string;
  name: string;
}

interface Segment {
  start: number;
  end: number;
  text: string;
}

interface TranscribeResult {
  language: string;
  language_probability: number;
  duration: number;
  segments: Segment[];
}

interface ActionItem {
  text: string;
  assignee: string | null;
}

interface ExtractionResult {
  actions: ActionItem[];
  decisions: string[];
}

interface LiveSegment {
  offset_s: number;
  duration_s: number;
  text: string;
}

const AUDIO_EXTS = ["m4a", "mp3", "wav", "flac", "ogg", "aac", "opus", "webm"];

function isAudioPath(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return AUDIO_EXTS.includes(ext);
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function joinTranscript(segments: Segment[]): string {
  return segments.map((s) => s.text.trim()).join(" ");
}

function friendlyExtractError(raw: string): string {
  const lower = raw.toLowerCase();
  if (
    lower.includes("500") ||
    lower.includes("memory") ||
    lower.includes("alloc")
  ) {
    return (
      "GPU/RAM dolu olduğu için aksiyon çıkarımı yapılamadı. " +
      "Transkript hazır — Markdown'a Kopyala ile dışa aktarabilir, " +
      "ya da arkaplan uygulamalarını (NVIDIA Broadcast vb.) kapatıp Tekrar Çıkar diyebilirsin."
    );
  }
  if (lower.includes("connection") || lower.includes("refused")) {
    return "Ollama'ya bağlanılamadı. Ollama çalışıyor mu? (Tray simgesini kontrol et.)";
  }
  return raw;
}

function todayIso(): string {
  return new Date().toISOString().slice(0, 10);
}

interface MarkdownSource {
  title: string;
  duration_s: number | null;
  language: string | null;
  language_probability: number | null;
  segments: { start: number; end: number; text: string }[];
  extraction: ExtractionResult | null;
}

function toMarkdown(src: MarkdownSource): string {
  const lines: string[] = [];
  lines.push(`# ${src.title}`);
  lines.push("");
  lines.push(`**Tarih:** ${todayIso()}`);
  if (src.duration_s !== null) {
    lines.push(`**Süre:** ${formatTime(src.duration_s)}`);
  }
  if (src.language && src.language_probability !== null) {
    lines.push(
      `**Dil:** ${src.language} (${Math.round(src.language_probability * 100)}%)`,
    );
  }
  lines.push("");

  if (src.extraction && src.extraction.actions.length > 0) {
    lines.push("## Aksiyonlar");
    lines.push("");
    for (const a of src.extraction.actions) {
      const prefix = a.assignee ? `**${a.assignee}** — ` : "";
      lines.push(`- ${prefix}${a.text}`);
    }
    lines.push("");
  }

  if (src.extraction && src.extraction.decisions.length > 0) {
    lines.push("## Kararlar");
    lines.push("");
    for (const d of src.extraction.decisions) {
      lines.push(`- ${d}`);
    }
    lines.push("");
  }

  lines.push("## Transkript");
  lines.push("");
  for (const s of src.segments) {
    lines.push(
      `\`${formatTime(s.start)}–${formatTime(s.end)}\` ${s.text.trim()}`,
    );
  }
  lines.push("");

  return lines.join("\n");
}

export default function App() {
  const [isDragOver, setIsDragOver] = useState(false);
  const [file, setFile] = useState<DroppedFile | null>(null);
  const [warning, setWarning] = useState<string | null>(null);

  // Transcribe
  const [transcribing, setTranscribing] = useState(false);
  const [transcribeError, setTranscribeError] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<TranscribeResult | null>(null);
  const [transcribeMs, setTranscribeMs] = useState<number | null>(null);

  // Extraction
  const [extracting, setExtracting] = useState(false);
  const [extractError, setExtractError] = useState<string | null>(null);
  const [extraction, setExtraction] = useState<ExtractionResult | null>(null);
  const [extractMs, setExtractMs] = useState<number | null>(null);

  // Markdown export
  const [copied, setCopied] = useState(false);

  // Live recording
  const [recording, setRecording] = useState(false);
  const [startingRecording, setStartingRecording] = useState(false);
  const [recordingError, setRecordingError] = useState<string | null>(null);
  const [liveSegments, setLiveSegments] = useState<LiveSegment[]>([]);
  const [level, setLevel] = useState(0);
  const recordingStartedAt = useRef<number | null>(null);
  const [elapsed, setElapsed] = useState(0);

  // Live recording: listen for backend events
  useEffect(() => {
    // StrictMode mounts effects twice in dev — without explicit cancellation
    // the listener Promises resolve AFTER cleanup, leaving stale subscriptions.
    let cancelled = false;
    const cleanups: UnlistenFn[] = [];

    Promise.all([
      listen<number>("recording:level", (e) => setLevel(e.payload)),
      listen<LiveSegment>("recording:segment", (e) =>
        setLiveSegments((prev) => [...prev, e.payload]),
      ),
      listen<string>("recording:error", (e) => setRecordingError(e.payload)),
    ]).then((fns) => {
      if (cancelled) {
        for (const f of fns) f();
      } else {
        cleanups.push(...fns);
      }
    });

    return () => {
      cancelled = true;
      for (const f of cleanups) f();
    };
  }, []);

  // Elapsed timer while recording
  useEffect(() => {
    if (!recording) return;
    const id = window.setInterval(() => {
      const t0 = recordingStartedAt.current;
      if (t0 !== null) setElapsed((Date.now() - t0) / 1000);
    }, 250);
    return () => window.clearInterval(id);
  }, [recording]);

  useEffect(() => {
    const unlistenPromise = getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") {
        setIsDragOver(true);
      } else if (p.type === "leave") {
        setIsDragOver(false);
      } else if (p.type === "drop") {
        setIsDragOver(false);
        const audioPath = p.paths.find(isAudioPath);
        if (!audioPath) {
          setWarning(
            `Ses dosyası bulunamadı (${AUDIO_EXTS.join(", ")} bekleniyordu).`,
          );
          return;
        }
        setWarning(null);
        resetResults();
        setFile({ path: audioPath, name: baseName(audioPath) });
      }
    });
    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, []);

  function resetResults() {
    setTranscript(null);
    setTranscribeError(null);
    setTranscribeMs(null);
    setExtraction(null);
    setExtractError(null);
    setExtractMs(null);
  }

  async function pickFile() {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Ses Dosyaları", extensions: AUDIO_EXTS }],
    });
    if (typeof selected === "string") {
      setWarning(null);
      resetResults();
      setFile({ path: selected, name: baseName(selected) });
    }
  }

  async function runTranscribe() {
    if (!file) return;
    setWarning(null);
    setTranscribing(true);
    setTranscribeError(null);
    setTranscript(null);
    setExtraction(null);
    setExtractError(null);
    const t0 = performance.now();
    try {
      const r = await invoke<TranscribeResult>("transcribe_file", {
        path: file.path,
      });
      setTranscript(r);
      setTranscribeMs(performance.now() - t0);
      // Otomatik olarak aksiyon çıkarımına geç
      void runExtract(joinTranscript(r.segments));
    } catch (e) {
      setTranscribeError(String(e));
    } finally {
      setTranscribing(false);
    }
  }

  async function runExtract(transcriptText: string) {
    setExtracting(true);
    setExtractError(null);
    setExtraction(null);
    const t0 = performance.now();
    try {
      const r = await invoke<ExtractionResult>("extract_actions", {
        transcript: transcriptText,
      });
      setExtraction(r);
      setExtractMs(performance.now() - t0);
    } catch (e) {
      setExtractError(friendlyExtractError(String(e)));
    } finally {
      setExtracting(false);
    }
  }

  function retryExtractFromCurrent() {
    if (transcript) {
      void runExtract(joinTranscript(transcript.segments));
    } else if (liveSegments.length > 0) {
      const text = liveSegments.map((s) => s.text.trim()).join(" ");
      if (text.trim()) void runExtract(text);
    }
  }

  async function startRecording() {
    setRecordingError(null);
    setLiveSegments([]);
    setLevel(0);
    setElapsed(0);
    setExtraction(null);
    setExtractError(null);
    setExtractMs(null);
    setStartingRecording(true);
    try {
      await invoke("start_recording"); // Bekler: sidecar + model load
      recordingStartedAt.current = Date.now();
      setRecording(true);
    } catch (e) {
      setRecordingError(String(e));
    } finally {
      setStartingRecording(false);
    }
  }

  async function stopRecording() {
    try {
      await invoke("stop_recording");
    } catch (e) {
      setRecordingError(String(e));
    } finally {
      setRecording(false);
      recordingStartedAt.current = null;
    }
    // Kayıt sonunda biriken segmentler varsa otomatik aksiyon çıkarımı.
    // extract_actions sidecar'ı zaten kapatıyor → Gemma GPU'ya rahat yüklenir.
    // Kullanıcı isterse sonra "Tekrar Çıkar" ile yeniden çalıştırabilir.
    setTimeout(() => {
      setLiveSegments((current) => {
        if (current.length > 0) {
          const text = current.map((s) => s.text.trim()).join(" ");
          if (text.trim()) void runExtract(text);
        }
        return current;
      });
    }, 0);
  }

  function buildMarkdownSource(): MarkdownSource | null {
    if (transcript && file) {
      return {
        title: file.name.replace(/\.[^.]+$/, ""),
        duration_s: transcript.duration,
        language: transcript.language,
        language_probability: transcript.language_probability,
        segments: transcript.segments,
        extraction,
      };
    }
    if (liveSegments.length > 0) {
      const last = liveSegments[liveSegments.length - 1];
      const total = last.offset_s + last.duration_s;
      return {
        title: `Canlı kayıt — ${todayIso()}`,
        duration_s: total,
        language: "tr",
        language_probability: null,
        segments: liveSegments.map((s) => ({
          start: s.offset_s,
          end: s.offset_s + s.duration_s,
          text: s.text,
        })),
        extraction,
      };
    }
    return null;
  }

  async function copyMarkdown() {
    const src = buildMarkdownSource();
    if (!src) return;
    const md = toMarkdown(src);
    try {
      await navigator.clipboard.writeText(md);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      window.prompt("Kopyala (Ctrl+C):", md);
    }
  }

  return (
    <main className="app">
      <h1>Voice Notes TR</h1>
      <p className="hint">
        Toplantı kaydını sürükleyip bırak, ya da mikrofondan canlı kaydet.
      </p>

      <section className={"recorder" + (recording ? " recorder--active" : "")}>
        <div className="recorder__row">
          {!recording ? (
            <button
              className="btn btn--primary"
              onClick={startRecording}
              disabled={!!file || transcribing || startingRecording}
              type="button"
            >
              {startingRecording ? "Hazırlanıyor…" : "● Kayda Başla"}
            </button>
          ) : (
            <button
              className="btn btn--danger"
              onClick={stopRecording}
              type="button"
            >
              ■ Kayda Dur
            </button>
          )}
          {recording && (
            <>
              <div className="vumeter" aria-label="Ses seviyesi">
                <div
                  className="vumeter__bar"
                  style={{ width: `${Math.min(100, level * 200)}%` }}
                />
              </div>
              <span className="recorder__time">
                {formatTime(elapsed)} · {liveSegments.length} segment
              </span>
            </>
          )}
          {!recording && liveSegments.length === 0 && (
            <span className="recorder__hint">
              Mikrofonu konuşmaya hazırla, butona bas, konuşmayı bitirdiğinde
              tekrar bas.
            </span>
          )}
        </div>

        {recordingError && (
          <p className="warning warning--error">
            Kayıt hatası: {recordingError}
          </p>
        )}

        {liveSegments.length > 0 && (
          <ol className="live-segments">
            {liveSegments.map((s, i) => (
              <li key={i}>
                <span className="segments__ts">
                  {formatTime(s.offset_s)}–
                  {formatTime(s.offset_s + s.duration_s)}
                </span>
                <span className="segments__text">{s.text}</span>
              </li>
            ))}
          </ol>
        )}
      </section>

      <div
        className={
          "dropzone" +
          (isDragOver ? " dropzone--hover" : "") +
          (file ? " dropzone--filled" : "")
        }
      >
        {file ? (
          <div className="dropzone__file">
            <div className="dropzone__name">{file.name}</div>
            <div className="dropzone__path">{file.path}</div>
            <div className="dropzone__actions">
              <button
                className="btn btn--primary"
                onClick={runTranscribe}
                disabled={transcribing || extracting}
                type="button"
              >
                {transcribing
                  ? "Çevriliyor…"
                  : extracting
                    ? "Aksiyonlar çıkarılıyor…"
                    : "Transkribe Et"}
              </button>
              <button
                className="btn"
                onClick={() => {
                  setFile(null);
                  resetResults();
                }}
                disabled={transcribing || extracting}
                type="button"
              >
                Temizle
              </button>
            </div>
          </div>
        ) : (
          <div className="dropzone__idle">
            <div className="dropzone__icon">🎙️</div>
            <div>{isDragOver ? "Bırak" : "Ses dosyasını buraya sürükle"}</div>
            <div className="dropzone__or">veya</div>
            <button
              className="btn"
              onClick={pickFile}
              type="button"
            >
              Dosya Seç…
            </button>
            <div className="dropzone__exts">
              {AUDIO_EXTS.map((e) => `.${e}`).join("  ·  ")}
            </div>
          </div>
        )}
      </div>

      {warning && <p className="warning">{warning}</p>}
      {transcribeError && (
        <p className="warning warning--error">
          Transcribe hatası: {transcribeError}
        </p>
      )}

      {(transcript || liveSegments.length > 0) && (
        <div className="export">
          <button
            className={"btn btn--primary" + (copied ? " btn--success" : "")}
            onClick={copyMarkdown}
            disabled={transcribing || extracting || recording}
            type="button"
          >
            {copied ? "✓ Kopyalandı" : "Markdown'a Kopyala"}
          </button>
          <span className="export__hint">
            Notion · Obsidian · Slack — yapıştır ve gönder
          </span>
        </div>
      )}

      {(extraction || extracting || extractError) && (
        <section className="panel">
          <header className="panel__header">
            <h2>Aksiyonlar &amp; Kararlar</h2>
            <button
              className="btn btn--small"
              onClick={retryExtractFromCurrent}
              disabled={extracting || recording}
              type="button"
            >
              {extracting ? "Çıkarılıyor…" : "Tekrar Çıkar"}
            </button>
          </header>

          {extractError && (
            <p className="warning warning--error">
              Çıkarım hatası: {extractError}
            </p>
          )}

          {extraction && (
            <>
              <div className="panel__meta">
                {extraction.actions.length} aksiyon · {extraction.decisions.length} karar
                {extractMs !== null && <> · {(extractMs / 1000).toFixed(1)}s</>}
              </div>

              {extraction.actions.length > 0 ? (
                <ul className="actions">
                  {extraction.actions.map((a, i) => (
                    <li key={i} className="action">
                      <span className="action__bullet">▸</span>
                      <span className="action__text">{a.text}</span>
                      {a.assignee && (
                        <span className="action__assignee">{a.assignee}</span>
                      )}
                    </li>
                  ))}
                </ul>
              ) : (
                !extracting && (
                  <p className="panel__empty">Aksiyon maddesi bulunamadı.</p>
                )
              )}

              {extraction.decisions.length > 0 && (
                <>
                  <h3 className="panel__subhead">Kararlar</h3>
                  <ul className="decisions">
                    {extraction.decisions.map((d, i) => (
                      <li key={i}>
                        <span className="decisions__bullet">●</span> {d}
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </>
          )}
        </section>
      )}

      {transcript && (
        <>
          <section className="panel">
            <header className="panel__header">
              <h2>Transkript</h2>
              <div className="panel__meta">
                {transcript.language} · {formatTime(transcript.duration)} ·{" "}
                {transcript.segments.length} segment
                {transcribeMs !== null && (
                  <>
                    {" "}
                    · {(transcribeMs / 1000).toFixed(1)}s (
                    {(transcript.duration / (transcribeMs / 1000)).toFixed(1)}×)
                  </>
                )}
              </div>
            </header>
            <ol className="segments">
              {transcript.segments.map((s, i) => (
                <li key={i}>
                  <span className="segments__ts">
                    {formatTime(s.start)}–{formatTime(s.end)}
                  </span>
                  <span className="segments__text">{s.text.trim()}</span>
                </li>
              ))}
            </ol>
          </section>
        </>
      )}
    </main>
  );
}
