import type { ProviderForm } from "../components/ProviderPanel";
import type { ProviderProfileListItem } from "../services/tauriApi";

export type ProviderValidationContext = {
  profileName: string;
  editingProfileId: string | null;
  profiles: ProviderProfileListItem[];
  form: ProviderForm;
};

export const PROVIDER_FIELD_TOOLTIPS = {
  profileName:
    "为这套服务商方案命名，保存后可在多套配置之间快速切换。",
  llmProvider: "选择用于生成建议回复的大模型服务商。",
  llmApiKey:
    "所选 LLM 服务商的 API Key，用于鉴权调用。编辑已有配置时，留空可保留已保存的密钥。",
  llmBaseUrl:
    "LLM API 的 HTTP 地址。OpenAI 兼容模式必填；其他服务商通常已预填默认值。",
  llmModel: "具体模型名称，例如 gpt-4o-mini、qwen-plus、glm-4-plus。",
  asrProvider: "选择用于实时语音转写的服务商。",
  asrApiKey:
    "所选 ASR 服务商的 API Key，用于鉴权调用。编辑已有配置时，留空可保留已保存的密钥。",
  asrBaseUrl:
    "ASR 接口地址。DashScope 实时与 OpenAI 实时可留空；SiliconFlow 等通常已预填默认值。",
  asrModel: "语音识别模型名称，例如 fun-asr-realtime、gpt-realtime-whisper。",
  asrLanguageHint:
    "提示主要使用的语言，例如 zh（中文）、en（英文）。留空则由模型自动判断。",
  asrMaxSentenceSilenceMs:
    "静音多少毫秒后判定一句话结束。留空则使用服务商默认值。",
  asrHeartbeat:
    "长时间无语音时是否发送心跳，防止 WebSocket 连接断开。一般场景无需开启。",
} as const;

function resolveTargetProfile(
  context: ProviderValidationContext,
): ProviderProfileListItem | null {
  const trimmedName = context.profileName.trim();
  if (context.editingProfileId) {
    return (
      context.profiles.find((profile) => profile.id === context.editingProfileId) ??
      null
    );
  }
  if (!trimmedName) return null;
  return context.profiles.find((profile) => profile.name === trimmedName) ?? null;
}

function hasStoredApiKey(
  profile: ProviderProfileListItem | null,
  kind: "llm" | "asr",
  provider: string,
): boolean {
  const summary = kind === "llm" ? profile?.summary.llm : profile?.summary.asr;
  return summary?.hasApiKey === true && summary.provider === provider;
}

function isLlmBaseUrlRequired(provider: string): boolean {
  return provider === "openai_compatible";
}

export function validateProviderForm(context: ProviderValidationContext): {
  valid: boolean;
  missingLabels: string[];
} {
  const { form } = context;
  const targetProfile = resolveTargetProfile(context);
  const missingLabels: string[] = [];

  if (!context.profileName.trim()) {
    missingLabels.push("配置名称");
  }

  const llmApiKeyProvided =
    Boolean(form.llmApiKey.trim()) ||
    hasStoredApiKey(targetProfile, "llm", form.llmProvider);
  if (!llmApiKeyProvided) {
    missingLabels.push("LLM API 密钥");
  }

  if (isLlmBaseUrlRequired(form.llmProvider) && !form.llmBaseUrl.trim()) {
    missingLabels.push("LLM 接口地址");
  }

  if (!form.llmModel.trim()) {
    missingLabels.push("LLM 模型");
  }

  const asrApiKeyProvided =
    Boolean(form.asrApiKey.trim()) ||
    hasStoredApiKey(targetProfile, "asr", form.asrProvider);
  if (!asrApiKeyProvided) {
    missingLabels.push("ASR API 密钥");
  }

  if (!form.asrModel.trim()) {
    missingLabels.push("ASR 模型");
  }

  return {
    valid: missingLabels.length === 0,
    missingLabels,
  };
}

export function isProviderFieldRequired(
  field:
    | "profileName"
    | "llmProvider"
    | "llmApiKey"
    | "llmBaseUrl"
    | "llmModel"
    | "asrProvider"
    | "asrApiKey"
    | "asrBaseUrl"
    | "asrModel",
  form: ProviderForm,
): boolean {
  switch (field) {
    case "profileName":
    case "llmProvider":
    case "llmApiKey":
    case "llmModel":
    case "asrProvider":
    case "asrApiKey":
    case "asrModel":
      return true;
    case "llmBaseUrl":
      return isLlmBaseUrlRequired(form.llmProvider);
    case "asrBaseUrl":
      return false;
    default:
      return false;
  }
}
