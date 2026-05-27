export default function App() {
  return (
    <main className="app">
      <h1>Voice Notes TR</h1>
      <p className="hint">Faz 0 tamam — HMR testi başarılıysa bu satırı görüyorsun.</p>
      <ul className="stack">
        <li>✅ Tauri v2 + Vite/React</li>
        <li>✅ faster-whisper GPU (large-v3-turbo, 11× realtime)</li>
        <li>✅ Ollama Gemma 3 4B (Türkçe, 22 t/s)</li>
        <li>⏳ Faz 1: dosya sürükle-bırak alanı buraya gelecek</li>
      </ul>
    </main>
  );
}
