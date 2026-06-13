import { useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  Copy,
  FileText,
  History,
  Pause,
  Play,
  Settings,
  SlidersHorizontal,
  Square,
} from "lucide-react";
import { AppearancePanel } from "./components/AppearancePanel";
import { ConversationHistoryPanel, type ExportStatus } from "./components/ConversationHistoryPanel";
import { DocumentsPanel } from "./components/DocumentsPanel";
import { MarkdownContent } from "./components/MarkdownContent";
import { SaveSessionPanel } from "./components/SaveSessionPanel";
import { ProviderPanel, type ProviderForm } from "./components/ProviderPanel";
import { validateProviderForm } from "./domain/providerFormValidation";
import {
  activateProviderProfile,
  deleteProviderProfile,
  endNativeSession,
  listAudioOutputDevices,
  listProviderProfiles,
  loadDocument,
  saveMarkdownFile,
  revealFileInFolder,
  unloadDocument,
  saveProviderProfile,
  startNativeSession,
  withTimeout,
  type DocumentSummary,
  type ProviderConfigSummary,
  type ProviderProfileListItem,
  type ProviderProfilesResponse,
} from "./services/tauriApi";
import {
  activateLocalProviderProfile,
  deleteLocalProviderProfile,
  listLocalProviderProfiles,
  saveLocalProviderProfile,
} from "./state/providerProfiles";
import { runMockRealtimeSession } from "./services/mockRealtime";
import {
  closeCurrentDialogWindow,
  openDialogWindow,
  type DialogWindowKind,
} from "./services/dialogWindows";
import {
  listenAppearanceSettings,
  publishAppearanceSettings,
  readAppearanceSettings,
} from "./services/appearanceBridge";
import { buildAppearanceShellStyle } from "./state/appearanceSettings";
import {
  isTauriRuntime,
  listenNativeRealtimeEvents,
} from "./services/realtimeBridge";
import { setupEnterVisibilityToggle } from "./services/windowVisibility";
import { setupMainWindowFit } from "./services/windowFit";
import {
  buildSessionTurns,
  createSavedSession,
  exportSavedSessionMarkdown,
  loadSavedSessions,
  persistSavedSessions,
  removeSavedSession,
  type SavedSession,
} from "./state/sessionHistory";
import {
  createInitialSessionState,
  reduceSessionEvent,
  type SessionState,
} from "./state/sessionStore";
import "./styles.css";

const PENDING_SAVE_SESSION_STORAGE_KEY = "respondent.pendingSaveSession";

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
  return `session-${crypto.randomUUID()}`;
}

function loadInitialSavedSessions(): SavedSession[] {
  if (typeof window === "undefined") return [];
  return loadSavedSessions(window.localStorage);
}

function loadPendingSaveSession(): SessionState | null {
  if (typeof window === "undefined") return null;
  const raw = window.localStorage.getItem(PENDING_SAVE_SESSION_STORAGE_KEY);
  if (!raw) return null;

  try {
    const parsed = JSON.parse(raw) as SessionState;
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      Array.isArray(parsed.transcript) &&
      Array.isArray(parsed.suggestions) &&
      Array.isArray(parsed.systemMessages)
    ) {
      return parsed;
    }
  } catch {
    return null;
  }

  return null;
}

function storePendingSaveSession(session: SessionState) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(
    PENDING_SAVE_SESSION_STORAGE_KEY,
    JSON.stringify(session),
  );
}

function clearPendingSaveSession() {
  if (typeof window === "undefined") return;
  window.localStorage.removeItem(PENDING_SAVE_SESSION_STORAGE_KEY);
}

function getDialogKind(): DialogWindowKind | null {
  if (typeof window === "undefined") return null;
  const kind = new URLSearchParams(window.location.search).get("dialog");
  if (
    kind === "appearance" ||
    kind === "conversation-history" ||
    kind === "providers" ||
    kind === "save-session" ||
    kind === "documents"
  ) {
    return kind;
  }
  return null;
}

function hasSessionContent(session: SessionState): boolean {
  return (
    session.transcript.length > 0 ||
    session.suggestions.length > 0 ||
    session.currentSuggestion.trim().length > 0 ||
    session.systemMessages.length > 0
  );
}

export default function App() {
  const dialogKind = useMemo(() => getDialogKind(), []);
  const [session, setSession] = useState<SessionState>(() =>
    createInitialSessionState("idle"),
  );
  const [historyOpen, setHistoryOpen] = useState(false);
  const [currentHistoryExpanded, setCurrentHistoryExpanded] = useState(false);
  const [conversationHistoryOpen, setConversationHistoryOpen] = useState(false);
  const [savePromptOpen, setSavePromptOpen] = useState(false);
  const [pendingSaveSession, setPendingSaveSession] =
    useState<SessionState | null>(null);
  const [savedSessions, setSavedSessions] = useState<SavedSession[]>(() =>
    loadInitialSavedSessions(),
  );
  const [selectedSession, setSelectedSession] = useState<SavedSession | null>(
    null,
  );
  const [exportStatus, setExportStatus] = useState<ExportStatus | null>(null);
  const [exportStatusSessionId, setExportStatusSessionId] = useState<
    string | null
  >(null);
  const [configOpen, setConfigOpen] = useState(false);
  const [documentsOpen, setDocumentsOpen] = useState(false);
  const [documents, setDocuments] = useState<DocumentSummary[]>([]);
  const [appearanceOpen, setAppearanceOpen] = useState(false);
  const [windowOpacity, setWindowOpacity] = useState(
    () => readAppearanceSettings().windowOpacity,
  );
  const [windowBlur, setWindowBlur] = useState(
    () => readAppearanceSettings().windowBlur,
  );
  const [appearanceTheme, setAppearanceTheme] = useState<
    "dark" | "light"
  >(() => readAppearanceSettings().appearanceTheme);
  const skipAppearancePublishRef = useRef(false);
  const [providerForm, setProviderForm] = useState<ProviderForm>(() =>
    defaultProviderForm(),
  );
  const [providerProfiles, setProviderProfiles] = useState<
    ProviderProfileListItem[]
  >([]);
  const [profileName, setProfileName] = useState("");
  const [editingProfileId, setEditingProfileId] = useState<string | null>(null);
  const [configStatus, setConfigStatus] = useState("");
  const [endpointSilenceMs, setEndpointSilenceMs] = useState<number>(() => {
    const stored = localStorage.getItem("respondent.endpointSilenceMs");
    const parsed = stored ? parseInt(stored, 10) : NaN;
    return isNaN(parsed) ? 300 : Math.min(3000, Math.max(300, parsed));
  });
  const stopRef = useRef<null | (() => void)>(null);
  const isTransitioningRef = useRef(false);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);
  const shellRef = useRef<HTMLElement>(null);

  const isListening = session.status === "listening";
  const statusText = useMemo(() => {
    if (session.status === "listening") return "聆听中";
    if (session.status === "paused") return "已暂停";
    if (session.status === "ended") return "已结束";
    return "就绪";
  }, [session.status]);
  const currentTurns = useMemo(() => buildSessionTurns(session), [session]);
  const historyTurns = currentTurns.slice(0, -1);
  const previousTurn = historyTurns.at(-1);
  const latestCompletedSuggestion =
    [...session.suggestions].reverse().find((s) => s.trim().length > 0) ?? "";
  const displaySuggestion = session.currentSuggestion || latestCompletedSuggestion;
  const isGeneratingNewSuggestion =
    !!session.currentGenerationId &&
    !session.currentSuggestion &&
    !!latestCompletedSuggestion;
  const activeSavedSession = selectedSession ?? savedSessions[0] ?? null;
  const visibleExportStatus =
    exportStatus && exportStatusSessionId === activeSavedSession?.id
      ? exportStatus
      : null;
  const shellStyle = buildAppearanceShellStyle({
    windowOpacity,
    windowBlur,
    appearanceTheme,
  });

  useEffect(() => {
    document.documentElement.classList.toggle(
      "themeLight",
      appearanceTheme === "light",
    );
  }, [appearanceTheme]);

  useEffect(() => {
    const refreshSavedSessions = () => {
      setSavedSessions(loadInitialSavedSessions());
    };
    window.addEventListener("focus", refreshSavedSessions);
    return () => window.removeEventListener("focus", refreshSavedSessions);
  }, []);

  useEffect(() => setupEnterVisibilityToggle(), []);

  useEffect(() => {
    if (dialogKind) return;
    return setupMainWindowFit(shellRef.current);
  }, [dialogKind]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const stored = JSON.parse(
      localStorage.getItem("respondent.documents") ?? "[]",
    ) as Array<{ name: string; content: string }>;
    for (const doc of stored) {
      void loadDocument(doc.name, doc.content)
        .then((summary) =>
          setDocuments((prev) => [
            ...prev.filter((d) => d.name !== summary.name),
            summary,
          ]),
        )
        .catch(() => {});
    }
  }, []);

  useEffect(() => {
    let dispose = () => {};

    void listenAppearanceSettings((settings) => {
      skipAppearancePublishRef.current = true;
      setWindowOpacity(settings.windowOpacity);
      setWindowBlur(settings.windowBlur);
      setAppearanceTheme(settings.appearanceTheme);
    }).then((unlisten) => {
      dispose = unlisten;
    });

    return () => dispose();
  }, []);

  useEffect(() => {
    if (skipAppearancePublishRef.current) {
      skipAppearancePublishRef.current = false;
      return;
    }

    void publishAppearanceSettings({
      windowOpacity,
      windowBlur,
      appearanceTheme,
    });
  }, [appearanceTheme, windowBlur, windowOpacity]);

  useEffect(() => {
    void loadProviderProfiles().catch((error) => {
      setConfigStatus(error instanceof Error ? error.message : String(error));
    });
  }, []);

  async function loadProviderProfiles() {
    if (isTauriRuntime()) {
      applyProviderProfilesResponse(await listProviderProfiles());
      return;
    }
    applyProviderProfilesResponse(listLocalProviderProfiles());
  }

  function applyProviderProfilesResponse(response: ProviderProfilesResponse) {
    const activeProfile =
      response.profiles.find((profile) => profile.isActive) ?? null;
    setProviderProfiles(response.profiles);
    setProfileName(activeProfile?.name ?? "");
    setEditingProfileId(activeProfile?.id ?? null);
    setProviderForm({
      ...formFromSummary(response.active),
      llmApiKey: "",
      asrApiKey: "",
    });
  }

  function buildProviderPayload() {
    const maxSentence = optionalText(providerForm.asrMaxSentenceSilenceMs);
    return {
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
  }

  async function activateSavedProviderProfile(profileId: string) {
    try {
      const response = isTauriRuntime()
        ? await activateProviderProfile(profileId)
        : activateLocalProviderProfile(profileId);
      applyProviderProfilesResponse(response);
      setConfigStatus("已切换配置");
    } catch (error) {
      setConfigStatus(error instanceof Error ? error.message : String(error));
    }
  }

  async function deleteSavedProviderProfile(profileId: string) {
    try {
      const response = isTauriRuntime()
        ? await deleteProviderProfile(profileId)
        : deleteLocalProviderProfile(profileId);
      applyProviderProfilesResponse(response);
      setConfigStatus("已删除配置");
    } catch (error) {
      setConfigStatus(error instanceof Error ? error.message : String(error));
    }
  }

  function createFreshListeningState(logicalId: string, nativeId: string): SessionState {
    return {
      ...createInitialSessionState(logicalId),
      nativeSessionId: nativeId,
      status: "listening",
    };
  }

  async function start() {
    if (isTransitioningRef.current) return;
    isTransitioningRef.current = true;
    setIsTransitioning(true);
    try {
      stopRef.current?.();
      stopRef.current = null;
      setCurrentHistoryExpanded(false);
      setSavePromptOpen(false);
      setPendingSaveSession(null);

      if (isTauriRuntime()) {
        let stopNativeEvents: (() => void) | null = null;
        try {
          setSession({
            ...createInitialSessionState(createSessionId()),
            status: "idle",
            systemMessages: ["正在启动原生会话…"],
          });
          stopNativeEvents = await listenNativeRealtimeEvents((event) => {
            if (mountedRef.current) setSession((current) => reduceSessionEvent(current, event));
          });
          const devices = await withTimeout(
            listAudioOutputDevices(),
            5_000,
            "读取音频输出设备超时",
          );
          const device = devices.find((item) => item.is_default) ?? devices[0];
          if (!device) throw new Error("没有可用的音频输出设备");
          const nativeId = await withTimeout(
            startNativeSession("会议", device.id, endpointSilenceMs),
            20_000,
            "启动原生会话超时，请检查网络连接和 ASR 服务商配置",
          );
          if (!mountedRef.current) {
            stopNativeEvents?.();
            void endNativeSession(nativeId);
            return;
          }
          const logicalId = createSessionId();
          setSession(createFreshListeningState(logicalId, nativeId));
          stopRef.current = () => {
            stopNativeEvents?.();
            void endNativeSession(nativeId);
          };
        } catch (error) {
          stopNativeEvents?.();
          const message = error instanceof Error ? error.message : String(error);
          if (mountedRef.current) {
            setSession({
              ...createInitialSessionState("idle"),
              status: "idle",
              systemMessages: [`启动会话失败：${message}`],
            });
          }
        }
        return;
      }

      const logicalId = createSessionId();
      setSession(createFreshListeningState(logicalId, logicalId));
      stopRef.current = runMockRealtimeSession(logicalId, (event) => {
        if (mountedRef.current) setSession((current) => reduceSessionEvent(current, event));
      });
    } finally {
      isTransitioningRef.current = false;
      setIsTransitioning(false);
    }
  }

  async function pause() {
    if (session.status !== "listening" || isTransitioningRef.current) return;
    isTransitioningRef.current = true;
    setIsTransitioning(true);
    try {
      stopRef.current?.();
      stopRef.current = null;
      setSession((current) => ({
        ...current,
        status: "paused",
        nativeSessionId: null,
        liveSubtitle: "",
        currentGenerationId: null,
        currentSuggestion: "",
      }));
    } finally {
      isTransitioningRef.current = false;
      setIsTransitioning(false);
    }
  }

  async function resume() {
    if (session.status !== "paused" || isTransitioningRef.current) return;
    isTransitioningRef.current = true;
    setIsTransitioning(true);
    try {
      stopRef.current?.();
      stopRef.current = null;

      if (!isTauriRuntime()) {
        setSession((current) => ({
          ...current,
          status: "listening",
          liveSubtitle: "",
          currentGenerationId: null,
          currentSuggestion: "",
        }));
        return;
      }

      let stopNativeEvents: (() => void) | null = null;
      try {
        stopNativeEvents = await listenNativeRealtimeEvents((event) => {
          if (mountedRef.current) setSession((current) => reduceSessionEvent(current, event));
        });
        const devices = await withTimeout(
          listAudioOutputDevices(),
          5_000,
          "读取音频输出设备超时",
        );
        const device = devices.find((d) => d.is_default) ?? devices[0];
        if (!device) throw new Error("没有可用的音频输出设备");
        const nativeId = await withTimeout(
          startNativeSession("会议", device.id, endpointSilenceMs),
          20_000,
          "启动原生会话超时，请检查网络连接和 ASR 服务商配置",
        );
        if (!mountedRef.current) {
          stopNativeEvents?.();
          void endNativeSession(nativeId);
          return;
        }
        setSession((current) => ({
          ...current,
          nativeSessionId: nativeId,
          status: "listening",
          liveSubtitle: "",
          currentGenerationId: null,
          currentSuggestion: "",
        }));
        stopRef.current = () => {
          stopNativeEvents?.();
          void endNativeSession(nativeId);
        };
      } catch (error) {
        stopNativeEvents?.();
        const message = error instanceof Error ? error.message : String(error);
        if (mountedRef.current) {
          setSession((current) => ({
            ...current,
            status: "paused",
            systemMessages: [
              ...current.systemMessages.slice(-20),
              `恢复会话失败：${message}`,
            ],
          }));
        }
      }
    } finally {
      isTransitioningRef.current = false;
      setIsTransitioning(false);
    }
  }

  function togglePlayPause() {
    if (session.status === "listening") { void pause(); return; }
    if (session.status === "paused")    { void resume(); return; }
    void start();
  }

  function openFloatingDialog(
    kind: DialogWindowKind,
    fallback: () => void,
  ) {
    if (!isTauriRuntime()) {
      fallback();
      return;
    }

    void openDialogWindow(kind).catch((error) => {
      console.error(`Failed to open ${kind} window`, error);
    });
  }

  function end() {
    stopRef.current?.();
    stopRef.current = null;
    const ended = { ...session, status: "ended" as const, liveSubtitle: "" };
    setSession(ended);
    setPendingSaveSession(ended);
    storePendingSaveSession(ended);
    openFloatingDialog("save-session", () => setSavePromptOpen(true));
  }

  function getPendingSaveSource() {
    return pendingSaveSession ?? loadPendingSaveSession() ?? session;
  }

  async function saveEndedSession() {
    const source = getPendingSaveSource();
    if (!hasSessionContent(source)) {
      setSavePromptOpen(false);
      setPendingSaveSession(null);
      clearPendingSaveSession();
      if (dialogKind) {
        await closeCurrentDialogWindow();
      }
      return;
    }
    const saved = createSavedSession(source);
    setSavedSessions((current) => {
      const next = [
        saved,
        ...current.filter((item) => item.id !== saved.id),
      ];
      persistSavedSessions(window.localStorage, next);
      return next;
    });
    setSelectedSession(saved);
    setSavePromptOpen(false);
    setPendingSaveSession(null);
    clearPendingSaveSession();
    if (dialogKind) {
      await closeCurrentDialogWindow();
    }
  }

  async function discardEndedSession() {
    setSavePromptOpen(false);
    setPendingSaveSession(null);
    clearPendingSaveSession();
    if (dialogKind) {
      await closeCurrentDialogWindow();
    }
  }

  function deleteSavedSessionById(sessionId: string) {
    setSavedSessions((current) => {
      const next = removeSavedSession(
        window.localStorage,
        current,
        sessionId,
      );
      setSelectedSession((selected) =>
        selected?.id === sessionId ? (next[0] ?? null) : selected,
      );
      return next;
    });
  }

  async function copySuggestion() {
    await navigator.clipboard.writeText(displaySuggestion);
  }

  async function handleUploadDocument(file: File) {
    const content = await file.text();
    const summary = isTauriRuntime()
      ? await loadDocument(file.name, content)
      : ({
          name: file.name,
          chunkCount: 0,
          charCount: content.length,
        } as DocumentSummary);
    setDocuments((prev) => [
      ...prev.filter((d) => d.name !== summary.name),
      summary,
    ]);
    const stored = JSON.parse(
      localStorage.getItem("respondent.documents") ?? "[]",
    ) as Array<{ name: string; content: string }>;
    localStorage.setItem(
      "respondent.documents",
      JSON.stringify([
        ...stored.filter((d) => d.name !== file.name),
        { name: file.name, content },
      ]),
    );
  }

  async function handleUnloadDocument(name: string) {
    if (isTauriRuntime()) {
      await unloadDocument(name);
    }
    setDocuments((prev) => prev.filter((d) => d.name !== name));
    const stored = JSON.parse(
      localStorage.getItem("respondent.documents") ?? "[]",
    ) as Array<{ name: string; content: string }>;
    localStorage.setItem(
      "respondent.documents",
      JSON.stringify(stored.filter((d) => d.name !== name)),
    );
  }

  function browserDownloadMarkdown(filename: string, markdown: string) {
    const blob = new Blob([markdown], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    link.style.display = "none";
    document.body.appendChild(link);
    try {
      link.click();
    } finally {
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
    }
  }

  async function exportMarkdown(saved: SavedSession) {
    const markdown = exportSavedSessionMarkdown(saved);
    const filename = `${saved.title.replace(/[\\/:*?"<>|]/g, "-")}.md`;
    setExportStatusSessionId(saved.id);
    setExportStatus({ kind: "loading", message: "正在导出 Markdown…" });
    try {
      if (isTauriRuntime()) {
        const path = await saveMarkdownFile(filename, markdown);
        const exportedFilename = path.split(/[/\\]/).pop() ?? filename;
        setExportStatus({ kind: "success", path, filename: exportedFilename });
        return;
      }
      browserDownloadMarkdown(filename, markdown);
      setExportStatus({ kind: "info", message: "已开始下载 Markdown" });
    } catch (error) {
      setExportStatus({
        kind: "error",
        message: `导出失败：${error instanceof Error ? error.message : String(error)}`,
      });
    }
  }

  function revealExportedFile(path: string) {
    void revealFileInFolder(path).catch((error) => {
      setExportStatus({
        kind: "error",
        message: `打开文件位置失败：${error instanceof Error ? error.message : String(error)}`,
      });
    });
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
    const validation = validateProviderForm({
      profileName,
      editingProfileId,
      profiles: providerProfiles,
      form: providerForm,
    });
    if (!validation.valid) {
      setConfigStatus(`请填写必填项：${validation.missingLabels.join("、")}`);
      return;
    }

    const name = profileName.trim();

    const selectedProfile = editingProfileId
      ? providerProfiles.find((profile) => profile.id === editingProfileId)
      : null;
    const profileId =
      selectedProfile && selectedProfile.name === name
        ? selectedProfile.id
        : providerProfiles.find((profile) => profile.name === name)?.id ?? null;

    try {
      const response = isTauriRuntime()
        ? await saveProviderProfile(name, profileId, buildProviderPayload())
        : saveLocalProviderProfile(name, profileId, buildProviderPayload());
      applyProviderProfilesResponse(response);
      setConfigStatus("已保存");
    } catch (error) {
      setConfigStatus(error instanceof Error ? error.message : String(error));
    }
  }

  if (dialogKind) {
    const savePromptSource = getPendingSaveSource();

    return (
      <main
        className={`dialogWindowRoot ${
          appearanceTheme === "light" ? "themeLight" : ""
        }`}
        style={shellStyle}
      >
        {dialogKind === "appearance" ? (
          <AppearancePanel
            className="modalPanel appearancePanel detachedPanel"
            windowOpacity={windowOpacity}
            windowBlur={windowBlur}
            appearanceTheme={appearanceTheme}
            onWindowOpacityChange={setWindowOpacity}
            onWindowBlurChange={setWindowBlur}
            onAppearanceThemeChange={setAppearanceTheme}
            onClose={() => void closeCurrentDialogWindow()}
          />
        ) : null}

        {dialogKind === "providers" ? (
          <ProviderPanel
            className="modalPanel providerPanel detachedPanel"
            providerForm={providerForm}
            providerProfiles={providerProfiles}
            profileName={profileName}
            editingProfileId={editingProfileId}
            configStatus={configStatus}
            onProfileNameChange={(value) => {
              setProfileName(value);
              const selected = editingProfileId
                ? providerProfiles.find((profile) => profile.id === editingProfileId)
                : null;
              if (selected && selected.name !== value.trim()) {
                setEditingProfileId(null);
              }
            }}
            onUpdateLlmProvider={updateLlmProvider}
            onUpdateAsrProvider={updateAsrProvider}
            onProviderFormChange={setProviderForm}
            onSave={() => void saveProviders()}
            onActivateProfile={(profileId) =>
              void activateSavedProviderProfile(profileId)
            }
            onDeleteProfile={(profileId) =>
              void deleteSavedProviderProfile(profileId)
            }
            onClose={() => void closeCurrentDialogWindow()}
          />
        ) : null}

        {dialogKind === "conversation-history" ? (
          <ConversationHistoryPanel
            className="modalPanel conversationHistoryPanel detachedPanel"
            savedSessions={savedSessions}
            activeSession={activeSavedSession}
            onSelectSession={setSelectedSession}
            onDeleteSession={deleteSavedSessionById}
            onExportMarkdown={exportMarkdown}
            onRevealExportedFile={revealExportedFile}
            exportStatus={visibleExportStatus}
            onClose={() => void closeCurrentDialogWindow()}
          />
        ) : null}

        {dialogKind === "documents" ? (
          <DocumentsPanel
            className="modalPanel documentsPanel detachedPanel"
            documents={documents}
            onUpload={(file) => void handleUploadDocument(file)}
            onRemove={(name) => void handleUnloadDocument(name)}
            onClose={() => void closeCurrentDialogWindow()}
          />
        ) : null}

        {dialogKind === "save-session" ? (
          <SaveSessionPanel
            className="modalPanel saveSessionPanel detachedPanel"
            session={savePromptSource}
            onSave={() => void saveEndedSession()}
            onDiscard={() => void discardEndedSession()}
            onClose={() => void discardEndedSession()}
          />
        ) : null}
      </main>
    );
  }

  return (
    <main
      ref={shellRef}
      className={`shell ${appearanceTheme === "light" ? "themeLight" : ""}`}
      style={shellStyle}
    >
      <header className="topbar">
        <div className="identity">
          <div className="status">
            <span className={isListening ? "dot dotLive" : "dot"} />
            <span>{statusText}</span>
          </div>
          <span className="meta">系统音频</span>
        </div>
        <div className="actions">
          <button
            className={
              isListening ? "sessionControlPause" : "sessionControlStart"
            }
            type="button"
            onClick={togglePlayPause}
            disabled={isTransitioning}
            title={isTransitioning ? "处理中…" : isListening ? "暂停" : "开始"}
          >
            {isListening ? <Pause size={16} /> : <Play size={16} />}
          </button>
          <button
            className="sessionControlEnd"
            type="button"
            onClick={end}
            disabled={isTransitioning}
            title="结束"
          >
            <Square size={16} />
          </button>
          <button
            type="button"
            onClick={() => {
              openFloatingDialog("conversation-history", () => {
                setConversationHistoryOpen((value) => !value);
                setAppearanceOpen(false);
                setConfigOpen(false);
              });
            }}
            title="会话历史"
          >
            <History size={16} />
          </button>
          <button
            type="button"
            onClick={() => {
              openFloatingDialog("appearance", () => {
                setAppearanceOpen((value) => !value);
                setConfigOpen(false);
              });
            }}
            title="外观设置"
          >
            <SlidersHorizontal size={16} />
          </button>
          <button
            type="button"
            onClick={() => {
              openFloatingDialog("providers", () => {
                setConfigOpen((value) => !value);
                setAppearanceOpen(false);
              });
            }}
            title="服务商配置"
          >
            <Settings size={16} />
          </button>
          <button
            type="button"
            onClick={() => {
              openFloatingDialog("documents", () => {
                setDocumentsOpen((v) => !v);
                setConfigOpen(false);
                setAppearanceOpen(false);
              });
            }}
            title="文档知识库"
          >
            <FileText size={16} />
          </button>
        </div>
      </header>

      {!isTauriRuntime() && appearanceOpen ? (
        <div className="modalLayer">
          <AppearancePanel
            className="modalPanel appearancePanel"
            windowOpacity={windowOpacity}
            windowBlur={windowBlur}
            appearanceTheme={appearanceTheme}
            onWindowOpacityChange={setWindowOpacity}
            onWindowBlurChange={setWindowBlur}
            onAppearanceThemeChange={setAppearanceTheme}
            onClose={() => setAppearanceOpen(false)}
          />
        </div>
      ) : null}

      {!isTauriRuntime() && configOpen ? (
        <div className="modalLayer">
          <ProviderPanel
            providerForm={providerForm}
            providerProfiles={providerProfiles}
            profileName={profileName}
            editingProfileId={editingProfileId}
            configStatus={configStatus}
            onProfileNameChange={(value) => {
              setProfileName(value);
              const selected = editingProfileId
                ? providerProfiles.find((profile) => profile.id === editingProfileId)
                : null;
              if (selected && selected.name !== value.trim()) {
                setEditingProfileId(null);
              }
            }}
            onUpdateLlmProvider={updateLlmProvider}
            onUpdateAsrProvider={updateAsrProvider}
            onProviderFormChange={setProviderForm}
            onSave={() => void saveProviders()}
            onActivateProfile={(profileId) =>
              void activateSavedProviderProfile(profileId)
            }
            onDeleteProfile={(profileId) =>
              void deleteSavedProviderProfile(profileId)
            }
            onClose={() => setConfigOpen(false)}
          />
        </div>
      ) : null}

      {!isTauriRuntime() && documentsOpen ? (
        <div className="modalLayer">
          <DocumentsPanel
            className="modalPanel documentsPanel"
            documents={documents}
            onUpload={(file) => void handleUploadDocument(file)}
            onRemove={(name) => void handleUnloadDocument(name)}
            onClose={() => setDocumentsOpen(false)}
          />
        </div>
      ) : null}

      {!isTauriRuntime() && conversationHistoryOpen ? (
        <div className="modalLayer">
          <ConversationHistoryPanel
            aria-modal="true"
            className="modalPanel conversationHistoryPanel"
            savedSessions={savedSessions}
            activeSession={activeSavedSession}
            onSelectSession={setSelectedSession}
            onDeleteSession={deleteSavedSessionById}
            onExportMarkdown={exportMarkdown}
            onRevealExportedFile={revealExportedFile}
            exportStatus={visibleExportStatus}
            onClose={() => setConversationHistoryOpen(false)}
          />
        </div>
      ) : null}

      {!isTauriRuntime() && savePromptOpen ? (
        <div className="modalLayer">
          <SaveSessionPanel
            ariaModal="true"
            session={pendingSaveSession ?? session}
            onSave={() => void saveEndedSession()}
            onDiscard={() => void discardEndedSession()}
            onClose={() => void discardEndedSession()}
          />
        </div>
      ) : null}

      <section className="panel">
        <div className="label">实时字幕</div>
        <p className={session.liveSubtitle ? "subtitle partial" : "subtitle"}>
          {session.liveSubtitle ||
            session.transcript.at(-1) ||
            session.systemMessages.at(-1) ||
            "开始会话后即可看到实时字幕。"}
        </p>
      </section>

      <section className="panel replyPanel">
        <div className="row">
          <div className="label">
            建议回复
            {isGeneratingNewSuggestion && (
              <span className="replyGenerating">生成中…</span>
            )}
          </div>
          <button
            type="button"
            onClick={copySuggestion}
            disabled={!displaySuggestion}
            title="复制建议回复"
          >
            <Copy size={16} />
          </button>
        </div>
        {displaySuggestion ? (
          <MarkdownContent className="reply">{displaySuggestion}</MarkdownContent>
        ) : (
          <p className="reply replyPlaceholder">
            端点检测与最终转写完成后，建议回复将在此流式显示。
          </p>
        )}
      </section>

      <div className={`silenceRow${isListening ? " silenceRowDisabled" : ""}`}>
        <label className="silenceLabel" htmlFor="silenceSlider">
          端点静音
        </label>
        <input
          id="silenceSlider"
          type="range"
          className="silenceSlider"
          min={300}
          max={3000}
          step={100}
          value={endpointSilenceMs}
          disabled={isListening}
          title={isListening ? "会话进行中，暂停后可调整" : undefined}
          onChange={(e) => {
            const ms = parseInt(e.target.value, 10);
            setEndpointSilenceMs(ms);
            localStorage.setItem("respondent.endpointSilenceMs", String(ms));
          }}
        />
        <span className="silenceValue">
          {(endpointSilenceMs / 1000).toFixed(1)}s
        </span>
      </div>

      <section className="history">
        <button
          className="historyToggle"
          type="button"
          aria-label="展开或收起轮次记录"
          aria-expanded={historyOpen}
          aria-controls="current-turn-history-panel"
          onClick={() => {
            setHistoryOpen((open) => {
              if (open) {
                setCurrentHistoryExpanded(false);
              }
              return !open;
            });
          }}
        >
          <span>轮次记录</span>
          {historyOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
        </button>
        {historyOpen ? (
          historyTurns.length > 0 ? (
            <div
              id="current-turn-history-panel"
              aria-label="当前轮次记录"
              className={
                currentHistoryExpanded
                  ? "currentHistory expanded"
                  : "currentHistory"
              }
            >
              <div className="historySubhead">
                <span>{currentHistoryExpanded ? "全部轮次" : "上一轮"}</span>
                {historyTurns.length > 1 ? (
                  <button
                    className="textButton"
                    type="button"
                    onClick={() =>
                      setCurrentHistoryExpanded((value) => !value)
                    }
                  >
                    {currentHistoryExpanded ? "只显示最新" : "更多历史"}
                  </button>
                ) : null}
              </div>
              {(currentHistoryExpanded ? historyTurns : previousTurn ? [previousTurn] : []).map(
                (turn, index) => (
                  <article
                    className="currentTurn"
                    key={`${turn.transcript}-${index}`}
                  >
                    <p>转写：{turn.transcript}</p>
                    {turn.suggestion ? (
                      <p className="suggestionItem">
                        建议回复：<span>{turn.suggestion}</span>
                      </p>
                    ) : null}
                  </article>
                ),
              )}
            </div>
          ) : (
            <div className="historyBody" aria-label="展开的当前轮次">
              <p>当前轮次结束后，历史将在此显示。</p>
            </div>
          )
        ) : null}
      </section>
    </main>
  );
}
