import { useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  Copy,
  Pause,
  Play,
  Settings,
  Square,
} from "lucide-react";
import {
  endNativeSession,
  getProviderConfig,
  listAudioOutputDevices,
  saveProviderConfig,
  startNativeSession,
  type ProviderConfigSummary,
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

const LLM_DEFAULTS: Record<string, { baseUrl: string; model: string }> = {
  openai: { baseUrl: "", model: "gpt-5.4-mini" },
  dashscope: {
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus",
  },
  zhipu: {
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4-plus",
  },
  siliconflow: {
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "Qwen/Qwen3-8B",
  },
  openai_compatible: { baseUrl: "", model: "" },
};

const ASR_DEFAULTS: Record<string, { baseUrl: string; model: string }> = {
  openai_realtime: { baseUrl: "", model: "gpt-realtime-whisper" },
  bailian_realtime: { baseUrl: "", model: "fun-asr-realtime" },
  siliconflow_file: {
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "FunAudioLLM/SenseVoiceSmall",
  },
};

type ProviderForm = {
  llmProvider: string;
  llmApiKey: string;
  llmBaseUrl: string;
  llmModel: string;
  asrProvider: string;
  asrApiKey: string;
  asrBaseUrl: string;
  asrModel: string;
  asrLanguageHint: string;
  asrMaxSentenceSilenceMs: string;
  asrHeartbeat: boolean;
};

function defaultProviderForm(): ProviderForm {
  return {
    llmProvider: "openai",
    llmApiKey: "",
    llmBaseUrl: LLM_DEFAULTS.openai.baseUrl,
    llmModel: LLM_DEFAULTS.openai.model,
    asrProvider: "openai_realtime",
    asrApiKey: "",
    asrBaseUrl: ASR_DEFAULTS.openai_realtime.baseUrl,
    asrModel: ASR_DEFAULTS.openai_realtime.model,
    asrLanguageHint: "",
    asrMaxSentenceSilenceMs: "",
    asrHeartbeat: false,
  };
}

function formFromSummary(summary: ProviderConfigSummary): ProviderForm {
  const form = defaultProviderForm();
  if (summary.llm) {
    const defaults = LLM_DEFAULTS[summary.llm.provider] ?? LLM_DEFAULTS.openai;
    form.llmProvider = summary.llm.provider;
    form.llmBaseUrl = summary.llm.baseUrl ?? defaults.baseUrl;
    form.llmModel = summary.llm.model ?? defaults.model;
  }
  if (summary.asr) {
    const defaults =
      ASR_DEFAULTS[summary.asr.provider] ?? ASR_DEFAULTS.openai_realtime;
    form.asrProvider = summary.asr.provider;
    form.asrBaseUrl = summary.asr.baseUrl ?? defaults.baseUrl;
    form.asrModel = summary.asr.model ?? defaults.model;
    form.asrLanguageHint = summary.asr.languageHint ?? "";
    form.asrMaxSentenceSilenceMs =
      summary.asr.maxSentenceSilenceMs?.toString() ?? "";
    form.asrHeartbeat = summary.asr.heartbeat ?? false;
  }
  return form;
}

function optionalText(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function createSessionId() {
  return `session-${Date.now()}`;
}

export default function App() {
  const [session, setSession] = useState<SessionState>(() =>
    createInitialSessionState("idle"),
  );
  const [historyOpen, setHistoryOpen] = useState(false);
  const [configOpen, setConfigOpen] = useState(false);
  const [providerForm, setProviderForm] = useState<ProviderForm>(() =>
    defaultProviderForm(),
  );
  const [providerSummary, setProviderSummary] =
    useState<ProviderConfigSummary>({});
  const [configStatus, setConfigStatus] = useState("");
  const stopRef = useRef<null | (() => void)>(null);

  const isListening = session.status === "listening";
  const statusText = useMemo(() => {
    if (session.status === "listening") return "Listening";
    if (session.status === "paused") return "Paused";
    if (session.status === "ended") return "Saved";
    return "Ready";
  }, [session.status]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void getProviderConfig()
      .then((summary) => {
        setProviderSummary(summary);
        setProviderForm(formFromSummary(summary));
      })
      .catch((error) => {
        setConfigStatus(error instanceof Error ? error.message : String(error));
      });
  }, []);

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
          error instanceof Error ? error.message : String(error);
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

  function updateLlmProvider(provider: string) {
    const defaults = LLM_DEFAULTS[provider] ?? LLM_DEFAULTS.openai;
    setProviderForm((current) => ({
      ...current,
      llmProvider: provider,
      llmApiKey: "",
      llmBaseUrl: defaults.baseUrl,
      llmModel: defaults.model,
    }));
  }

  function updateAsrProvider(provider: string) {
    const defaults = ASR_DEFAULTS[provider] ?? ASR_DEFAULTS.openai_realtime;
    setProviderForm((current) => ({
      ...current,
      asrProvider: provider,
      asrApiKey: "",
      asrBaseUrl: defaults.baseUrl,
      asrModel: defaults.model,
      asrLanguageHint: "",
      asrMaxSentenceSilenceMs: "",
      asrHeartbeat: false,
    }));
  }

  async function saveProviders() {
    const maxSentence = optionalText(providerForm.asrMaxSentenceSilenceMs);
    const payload = {
      llm: {
        provider: providerForm.llmProvider,
        apiKey: optionalText(providerForm.llmApiKey),
        baseUrl: optionalText(providerForm.llmBaseUrl),
        model: optionalText(providerForm.llmModel),
      },
      asr: {
        provider: providerForm.asrProvider,
        apiKey: optionalText(providerForm.asrApiKey),
        baseUrl: optionalText(providerForm.asrBaseUrl),
        model: optionalText(providerForm.asrModel),
        languageHint: optionalText(providerForm.asrLanguageHint),
        maxSentenceSilenceMs: maxSentence ? Number(maxSentence) : null,
        heartbeat: providerForm.asrHeartbeat,
      },
    };

    if (isTauriRuntime()) {
      const summary = await saveProviderConfig(payload);
      setProviderSummary(summary);
      setProviderForm((current) => ({
        ...formFromSummary(summary),
        llmApiKey: current.llmApiKey ? "" : current.llmApiKey,
        asrApiKey: current.asrApiKey ? "" : current.asrApiKey,
      }));
    } else {
      setProviderSummary({
        llm: {
          provider: payload.llm.provider,
          hasApiKey: Boolean(payload.llm.apiKey),
          baseUrl: payload.llm.baseUrl,
          model: payload.llm.model,
        },
        asr: {
          provider: payload.asr.provider,
          hasApiKey: Boolean(payload.asr.apiKey),
          baseUrl: payload.asr.baseUrl,
          model: payload.asr.model,
          languageHint: payload.asr.languageHint,
          maxSentenceSilenceMs: payload.asr.maxSentenceSilenceMs,
          heartbeat: payload.asr.heartbeat,
        },
      });
    }
    setConfigStatus("Saved");
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
          <button
            type="button"
            onClick={() => setConfigOpen((value) => !value)}
            title="Configure providers"
          >
            <Settings size={16} />
          </button>
        </div>
      </header>

      {configOpen ? (
        <section className="configPanel">
          <div className="configHeader">
            <div>
              <div className="label">Providers</div>
              <div className="configStatus">
                LLM key {providerSummary.llm?.hasApiKey ? "set" : "not set"} ·
                ASR key {providerSummary.asr?.hasApiKey ? "set" : "not set"}
              </div>
            </div>
            <button type="button" onClick={saveProviders}>
              Save
            </button>
          </div>

          <div className="configGrid">
            <label>
              <span>LLM provider</span>
              <select
                aria-label="LLM provider"
                value={providerForm.llmProvider}
                onChange={(event) => updateLlmProvider(event.target.value)}
              >
                <option value="openai">OpenAI</option>
                <option value="dashscope">DashScope</option>
                <option value="zhipu">Zhipu/Z.ai</option>
                <option value="siliconflow">SiliconFlow</option>
                <option value="openai_compatible">OpenAI Compatible</option>
              </select>
            </label>
            <label>
              <span>LLM API key</span>
              <input
                aria-label="LLM API key"
                type="password"
                value={providerForm.llmApiKey}
                onChange={(event) =>
                  setProviderForm((current) => ({
                    ...current,
                    llmApiKey: event.target.value,
                  }))
                }
              />
            </label>
            <label>
              <span>LLM base URL</span>
              <input
                aria-label="LLM base URL"
                value={providerForm.llmBaseUrl}
                onChange={(event) =>
                  setProviderForm((current) => ({
                    ...current,
                    llmBaseUrl: event.target.value,
                  }))
                }
              />
            </label>
            <label>
              <span>LLM model</span>
              <input
                aria-label="LLM model"
                value={providerForm.llmModel}
                onChange={(event) =>
                  setProviderForm((current) => ({
                    ...current,
                    llmModel: event.target.value,
                  }))
                }
              />
            </label>
          </div>

          <div className="configGrid">
            <label>
              <span>ASR provider</span>
              <select
                aria-label="ASR provider"
                value={providerForm.asrProvider}
                onChange={(event) => updateAsrProvider(event.target.value)}
              >
                <option value="openai_realtime">OpenAI Realtime</option>
                <option value="bailian_realtime">DashScope Realtime</option>
                <option value="siliconflow_file">SiliconFlow File</option>
              </select>
            </label>
            <label>
              <span>ASR API key</span>
              <input
                aria-label="ASR API key"
                type="password"
                value={providerForm.asrApiKey}
                onChange={(event) =>
                  setProviderForm((current) => ({
                    ...current,
                    asrApiKey: event.target.value,
                  }))
                }
              />
            </label>
            <label>
              <span>ASR base URL</span>
              <input
                aria-label="ASR base URL"
                value={providerForm.asrBaseUrl}
                onChange={(event) =>
                  setProviderForm((current) => ({
                    ...current,
                    asrBaseUrl: event.target.value,
                  }))
                }
              />
            </label>
            <label>
              <span>ASR model</span>
              <input
                aria-label="ASR model"
                value={providerForm.asrModel}
                onChange={(event) =>
                  setProviderForm((current) => ({
                    ...current,
                    asrModel: event.target.value,
                  }))
                }
              />
            </label>
            {providerForm.asrProvider === "bailian_realtime" ? (
              <>
                <label>
                  <span>Language hint</span>
                  <input
                    aria-label="Language hint"
                    value={providerForm.asrLanguageHint}
                    onChange={(event) =>
                      setProviderForm((current) => ({
                        ...current,
                        asrLanguageHint: event.target.value,
                      }))
                    }
                  />
                </label>
                <label>
                  <span>Silence ms</span>
                  <input
                    aria-label="Silence ms"
                    inputMode="numeric"
                    value={providerForm.asrMaxSentenceSilenceMs}
                    onChange={(event) =>
                      setProviderForm((current) => ({
                        ...current,
                        asrMaxSentenceSilenceMs: event.target.value,
                      }))
                    }
                  />
                </label>
                <label className="toggleField">
                  <input
                    aria-label="Heartbeat"
                    type="checkbox"
                    checked={providerForm.asrHeartbeat}
                    onChange={(event) =>
                      setProviderForm((current) => ({
                        ...current,
                        asrHeartbeat: event.target.checked,
                      }))
                    }
                  />
                  <span>Heartbeat</span>
                </label>
              </>
            ) : null}
          </div>
          {configStatus ? <div className="configStatus">{configStatus}</div> : null}
        </section>
      ) : null}

      <section className="panel">
        <div className="label">Subtitle</div>
        <p className={session.liveSubtitle ? "subtitle partial" : "subtitle"}>
          {session.liveSubtitle ||
            session.transcript.at(-1) ||
            session.systemMessages.at(-1) ||
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
