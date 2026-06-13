import { invoke } from "@tauri-apps/api/core";

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
): Promise<string> {
  return invoke<string>("start_session", { title, outputDeviceId });
}

export async function endNativeSession(sessionId: string): Promise<void> {
  await invoke("end_session", { sessionId });
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

export async function getProviderConfig(): Promise<ProviderConfigSummary> {
  return invoke<ProviderConfigSummary>("get_provider_config");
}

export async function saveProviderConfig(
  payload: ProviderSettingsPayload,
): Promise<ProviderConfigSummary> {
  return invoke<ProviderConfigSummary>("save_provider_config", { payload });
}

export async function clearProviderConfig(
  scope?: "llm" | "asr",
): Promise<ProviderConfigSummary> {
  return invoke<ProviderConfigSummary>("clear_provider_config", { scope });
}
