import { invoke } from "@tauri-apps/api/core";

export function withTimeout<T>(
  promise: Promise<T>,
  ms: number,
  message: string,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

export type OutputDevice = {
  id: string;
  name: string;
  is_default: boolean;
};

export async function listAudioOutputDevices(): Promise<OutputDevice[]> {
  return invoke<OutputDevice[]>("list_audio_output_devices");
}

export async function startNativeSession(
  title: string,
  outputDeviceId: string,
  endpointerSilenceMs?: number | null,
): Promise<string> {
  return invoke<string>("start_session", { title, outputDeviceId, endpointerSilenceMs });
}

export async function endNativeSession(sessionId: string): Promise<void> {
  await invoke("end_session", { sessionId });
}

export async function retryReply(sessionId: string): Promise<void> {
  await invoke("retry_reply", { sessionId });
}

export async function saveMarkdownFile(
  filename: string,
  content: string,
): Promise<string> {
  return invoke<string>("save_markdown_file", { filename, content });
}

export async function revealFileInFolder(path: string): Promise<void> {
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

export type LlmProviderSettings = {
  provider: string;
  apiKey?: string | null;
  baseUrl?: string | null;
  model?: string | null;
};

export type AsrProviderSettings = {
  provider: string;
  apiKey?: string | null;
  baseUrl?: string | null;
  model?: string | null;
  languageHint?: string | null;
  maxSentenceSilenceMs?: number | null;
  heartbeat?: boolean | null;
};

export type ProviderSettingsPayload = {
  llm?: LlmProviderSettings | null;
  asr?: AsrProviderSettings | null;
};

export type LlmProviderSummary = {
  provider: string;
  hasApiKey: boolean;
  baseUrl?: string | null;
  model?: string | null;
};

export type AsrProviderSummary = {
  provider: string;
  hasApiKey: boolean;
  baseUrl?: string | null;
  model?: string | null;
  languageHint?: string | null;
  maxSentenceSilenceMs?: number | null;
  heartbeat?: boolean | null;
};

export type ProviderConfigSummary = {
  llm?: LlmProviderSummary | null;
  asr?: AsrProviderSummary | null;
};

export type ProviderProfileListItem = {
  id: string;
  name: string;
  isActive: boolean;
  summary: ProviderConfigSummary;
};

export type ProviderProfilesResponse = {
  profiles: ProviderProfileListItem[];
  active: ProviderConfigSummary;
};

export async function getProviderConfig(): Promise<ProviderConfigSummary> {
  return invoke<ProviderConfigSummary>("get_provider_config");
}

export async function listProviderProfiles(): Promise<ProviderProfilesResponse> {
  return invoke<ProviderProfilesResponse>("list_provider_profiles");
}

export async function saveProviderConfig(
  payload: ProviderSettingsPayload,
): Promise<ProviderConfigSummary> {
  return invoke<ProviderConfigSummary>("save_provider_config", { payload });
}

export async function saveProviderProfile(
  name: string,
  profileId: string | null,
  payload: ProviderSettingsPayload,
): Promise<ProviderProfilesResponse> {
  return invoke<ProviderProfilesResponse>("save_provider_profile", {
    name,
    profileId,
    payload,
  });
}

export async function activateProviderProfile(
  profileId: string,
): Promise<ProviderProfilesResponse> {
  return invoke<ProviderProfilesResponse>("activate_provider_profile", {
    profileId,
  });
}

export async function deleteProviderProfile(
  profileId: string,
): Promise<ProviderProfilesResponse> {
  return invoke<ProviderProfilesResponse>("delete_provider_profile", {
    profileId,
  });
}

export async function clearProviderConfig(
  scope?: "llm" | "asr",
): Promise<ProviderConfigSummary> {
  return invoke<ProviderConfigSummary>("clear_provider_config", { scope });
}

// ── Document knowledge base ───────────────────────────────────────────────────

export type DocumentSummary = {
  name: string;
  chunkCount: number;
  charCount: number;
};

export async function loadDocument(
  name: string,
  content: string,
): Promise<DocumentSummary> {
  return invoke<DocumentSummary>("load_document", { name, content });
}

export async function unloadDocument(name: string): Promise<void> {
  return invoke<void>("unload_document", { name });
}

export async function listDocuments(): Promise<DocumentSummary[]> {
  return invoke<DocumentSummary[]>("list_documents");
}

// ── Reply style ────────────────────────────────────────────────────────────────

export type ReplyStyleSettings = {
  userPrompt: string;
};

export async function getReplyStyleSettings(): Promise<ReplyStyleSettings> {
  return invoke<ReplyStyleSettings>("get_reply_style_settings");
}

export async function saveReplyStyleSettings(
  settings: ReplyStyleSettings,
): Promise<ReplyStyleSettings> {
  return invoke<ReplyStyleSettings>("save_reply_style_settings", { settings });
}

export type SavedSessionTurn = {
  transcript: string;
  suggestion?: string | null;
};

export type SavedSessionRecord = {
  id: string;
  title: string;
  date: string;
  startedAt: string;
  endedAt: string;
  turns: SavedSessionTurn[];
  systemMessages: string[];
};

export async function listSavedSessions(): Promise<SavedSessionRecord[]> {
  return invoke<SavedSessionRecord[]>("list_saved_sessions");
}

export async function upsertSavedSession(
  session: SavedSessionRecord,
): Promise<void> {
  await invoke("upsert_saved_session", { session });
}

export async function deleteSavedSession(sessionId: string): Promise<void> {
  await invoke("delete_saved_session", { sessionId });
}

export async function importLegacySavedSessions(
  sessions: SavedSessionRecord[],
): Promise<number> {
  return invoke<number>("import_legacy_saved_sessions", { sessions });
}
