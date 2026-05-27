import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

interface DroppedFile {
  path: string;
  name: string;
}

const AUDIO_EXTS = ["m4a", "mp3", "wav", "flac", "ogg", "aac", "opus", "webm"];

function isAudioPath(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return AUDIO_EXTS.includes(ext);
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export default function App() {
  const [isDragOver, setIsDragOver] = useState(false);
  const [file, setFile] = useState<DroppedFile | null>(null);
  const [warning, setWarning] = useState<string | null>(null);

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
        setFile({ path: audioPath, name: baseName(audioPath) });
      }
    });
    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, []);

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
            <button
              className="dropzone__clear"
              onClick={() => setFile(null)}
              type="button"
            >
              Temizle
            </button>
          </div>
        ) : (
          <div className="dropzone__idle">
            <div className="dropzone__icon">🎙️</div>
            <div>{isDragOver ? "Bırak" : "Ses dosyasını buraya sürükle"}</div>
            <div className="dropzone__exts">
              {AUDIO_EXTS.map((e) => `.${e}`).join("  ·  ")}
            </div>
          </div>
        )}
      </div>

      {warning && <p className="warning">{warning}</p>}

      <ul className="stack">
        <li>✅ Tauri v2 + Vite/React</li>
        <li>✅ faster-whisper GPU (large-v3-turbo, 11× realtime)</li>
        <li>✅ Ollama Gemma 3 4B (Türkçe, 22 t/s)</li>
        <li>⏳ Faz 1: Rust → sidecar IPC (sonraki adım)</li>
      </ul>
    </main>
  );
}
