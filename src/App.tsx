import "./styles.css";

function App() {
  return (
    <main className="assistant-shell">
      <section className="topbar" aria-label="Session controls">
        <strong>Respondent</strong>
        <span className="status">Ready</span>
      </section>
      <section className="panel" aria-label="Live transcript">
        <h1>Low-latency meeting assistant</h1>
        <p>Desktop scaffold ready for system-output audio, subtitles, replies, and session history.</p>
      </section>
    </main>
  );
}

export default App;
