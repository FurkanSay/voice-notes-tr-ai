import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { invoke } from "@tauri-apps/api/core";
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

function todayIso(): string {
  return new Date().toISOString().slice(0, 10);
}

function toMarkdown(
  file: DroppedFile,
  transcript: TranscribeResult,
  extraction: ExtractionResult | null,
): string {
  const title = file.name.replace(/\.[^.]+$/, "");
  const lines: string[] = [];
  lines.push(`# ${title}`);
  lines.push("");
  lines.push(`**Tarih:** ${todayIso()}`);
  lines.push(`**Süre:** ${formatTime(transcript.duration)}`);
  lines.push(
    `**Dil:** ${transcript.language} (${Math.round(transcript.language_probability * 100)}%)`,
  );
  lines.push("");

  if (extraction && extraction.actions.length > 0) {
    lines.push("## Aksiyonlar");
    lines.push("");
    for (const a of extraction.actions) {
      const prefix = a.assignee ? `**${a.assignee}** — ` : "";
      lines.push(`- ${prefix}${a.text}`);
    }
    lines.push("");
  }

  if (extraction && extraction.decisions.length > 0) {
    lines.push("## Kararlar");
    lines.push("");
    for (const d of extraction.decisions) {
      lines.push(`- ${d}`);
    }
    lines.push("");
  }

  lines.push("## Transkript");
  lines.push("");
  for (const s of transcript.segments) {
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
      setExtractError(String(e));
    } finally {
      setExtracting(false);
    }
  }

  function retryExtract() {
    if (transcript) {
      void runExtract(joinTranscript(transcript.segments));
    }
  }

  async function copyMarkdown() {
    if (!file || !transcript) return;
    const md = toMarkdown(file, transcript, extraction);
    try {
      await navigator.clipboard.writeText(md);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard API erişimi engellenmişse: prompt fallback
      window.prompt("Kopyala (Ctrl+C):", md);
    }
  }

  return (
    <main className="app">
      <h1>Voice Notes TR</h1>
      <p className="hint">Toplantı kaydını pencereye sürükleyip bırak.</p>

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

      {transcript && (
        <div className="export">
          <button
            className={"btn btn--primary" + (copied ? " btn--success" : "")}
            onClick={copyMarkdown}
            disabled={transcribing || extracting}
            type="button"
          >
            {copied ? "✓ Kopyalandı" : "Markdown'a Kopyala"}
          </button>
          <span className="export__hint">
            Notion · Obsidian · Slack — yapıştır ve gönder
          </span>
        </div>
      )}

      {transcript && (
        <>
          <section className="panel">
            <header className="panel__header">
              <h2>Aksiyonlar &amp; Kararlar</h2>
              <button
                className="btn btn--small"
                onClick={retryExtract}
                disabled={extracting}
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
                  {extraction.actions.length} aksiyon ·{" "}
                  {extraction.decisions.length} karar
                  {extractMs !== null && (
                    <> · {(extractMs / 1000).toFixed(1)}s</>
                  )}
                </div>

                {extraction.actions.length > 0 ? (
                  <ul className="actions">
                    {extraction.actions.map((a, i) => (
                      <li key={i} className="action">
                        <span className="action__bullet">▸</span>
                        <span className="action__text">{a.text}</span>
                        {a.assignee && (
                          <span className="action__assignee">
                            {a.assignee}
                          </span>
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
