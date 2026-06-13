import { useMemo, useRef, useState } from "react";
import { Copy, Pause, Play, Square, ChevronDown, ChevronUp } from "lucide-react";
import {
  endNativeSession,
  listAudioOutputDevices,
  startNativeSession,
} from "./services/tauriApi";
import { runMockRealtimeSession } from "./services/mockRealtime";
import {
  isTauriRuntime,
  listenNativeRealtimeEvents,
} from "./services/realtimeBridge";
import {
  createInitialSessionState,
  reduceSessionEvent,
  type SessionState,
} from "./state/sessionStore";
import "./styles.css";

function createSessionId() {
  return `session-${Date.now()}`;
}

export default function App() {
  const [session, setSession] = useState<SessionState>(() =>
    createInitialSessionState("idle"),
  );
  const [historyOpen, setHistoryOpen] = useState(false);
  const stopRef = useRef<null | (() => void)>(null);

  const isListening = session.status === "listening";
  const statusText = useMemo(() => {
    if (session.status === "listening") return "Listening";
    if (session.status === "paused") return "Paused";
    if (session.status === "ended") return "Saved";
    return "Ready";
  }, [session.status]);

  async function start() {
    const sessionId = createSessionId();
    stopRef.current?.();
    stopRef.current = null;

    if (isTauriRuntime()) {
      let stopNativeEvents: (() => void) | null = null;
      try {
        stopNativeEvents = await listenNativeRealtimeEvents((event) => {
          setSession((current) => reduceSessionEvent(current, event));
        });
        const devices = await listAudioOutputDevices();
        const device = devices.find((item) => item.is_default) ?? devices[0];
        if (!device) {
          throw new Error("No output device available");
        }
        const nativeSessionId = await startNativeSession("Meeting", device.id);
        setSession(createInitialSessionState(nativeSessionId));
        stopRef.current = () => {
          stopNativeEvents?.();
          void endNativeSession(nativeSessionId);
        };
      } catch (error) {
        stopNativeEvents?.();
        const message =
          error instanceof Error ? error.message : "Failed to start session";
        setSession({
          ...createInitialSessionState("idle"),
          status: "idle",
          systemMessages: [message],
        });
      }
      return;
    }

    setSession(createInitialSessionState(sessionId));
    stopRef.current = runMockRealtimeSession(sessionId, (event) => {
      setSession((current) => reduceSessionEvent(current, event));
    });
  }

  function pause() {
    stopRef.current?.();
    stopRef.current = null;
    setSession((current) => ({ ...current, status: "paused" }));
  }

  function end() {
    stopRef.current?.();
    stopRef.current = null;
    setSession((current) => ({ ...current, status: "ended", liveSubtitle: "" }));
  }

  async function copySuggestion() {
    await navigator.clipboard.writeText(session.currentSuggestion);
  }

  return (
    <main className="shell">
      <header className="topbar">
        <div className="status">
          <span className={isListening ? "dot dotLive" : "dot"} />
          <span>{statusText}</span>
        </div>
        <div className="actions">
          <button type="button" onClick={start} title="Start">
            <Play size={16} />
          </button>
          <button type="button" onClick={pause} title="Pause">
            <Pause size={16} />
          </button>
          <button type="button" onClick={end} title="End">
            <Square size={16} />
          </button>
        </div>
      </header>

      <section className="panel">
        <div className="label">Subtitle</div>
        <p className={session.liveSubtitle ? "subtitle partial" : "subtitle"}>
          {session.liveSubtitle ||
            session.transcript.at(-1) ||
            "Start a session to see live subtitles."}
        </p>
      </section>

      <section className="panel replyPanel">
        <div className="row">
          <div className="label">Suggested reply</div>
          <button
            type="button"
            onClick={copySuggestion}
            disabled={!session.currentSuggestion}
            title="Copy suggestion"
          >
            <Copy size={16} />
          </button>
        </div>
        <p className="reply">
          {session.currentSuggestion ||
            "The reply will stream here after an endpoint and final transcript."}
        </p>
      </section>

      <section className="history">
        <button
          className="historyToggle"
          type="button"
          onClick={() => setHistoryOpen((value) => !value)}
        >
          <span>Session history</span>
          {historyOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
        </button>
        {historyOpen ? (
          <div className="historyBody">
            {session.transcript.map((text, index) => (
              <p key={`${text}-${index}`}>{text}</p>
            ))}
            {session.suggestions.map((text, index) => (
              <p className="suggestionItem" key={`${text}-${index}`}>
                {text}
              </p>
            ))}
          </div>
        ) : null}
      </section>
    </main>
  );
}
