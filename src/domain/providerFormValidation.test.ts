import { describe, expect, it } from "vitest";
import type { ProviderForm } from "../components/ProviderPanel";
import {
  isProviderFieldRequired,
  validateProviderForm,
} from "./providerFormValidation";

function createForm(overrides: Partial<ProviderForm> = {}): ProviderForm {
  return {
    llmProvider: "openai",
    llmApiKey: "",
    llmBaseUrl: "https://api.openai.com/v1",
    llmModel: "gpt-4o-mini",
    asrProvider: "openai_realtime",
    asrApiKey: "",
    asrBaseUrl: "",
    asrModel: "gpt-realtime-whisper",
    asrLanguageHint: "",
    asrMaxSentenceSilenceMs: "",
    asrHeartbeat: false,
    ...overrides,
  };
}

describe("validateProviderForm", () => {
  it("requires profile name and both API keys for a new profile", () => {
    const result = validateProviderForm({
      profileName: "",
      editingProfileId: null,
      profiles: [],
      form: createForm(),
    });

    expect(result.valid).toBe(false);
    expect(result.missingLabels).toEqual([
      "配置名称",
      "LLM API 密钥",
      "ASR API 密钥",
    ]);
  });

  it("accepts stored API keys when editing the same provider", () => {
    const result = validateProviderForm({
      profileName: "工作配置",
      editingProfileId: "profile-1",
      profiles: [
        {
          id: "profile-1",
          name: "工作配置",
          isActive: true,
          summary: {
            llm: { provider: "openai", hasApiKey: true },
            asr: { provider: "bailian_realtime", hasApiKey: true },
          },
        },
      ],
      form: createForm({
        asrProvider: "bailian_realtime",
      }),
    });

    expect(result.valid).toBe(true);
    expect(result.missingLabels).toEqual([]);
  });

  it("requires LLM base URL and model for OpenAI compatible mode", () => {
    const result = validateProviderForm({
      profileName: "兼容配置",
      editingProfileId: null,
      profiles: [],
      form: createForm({
        llmProvider: "openai_compatible",
        llmApiKey: "llm-key",
        llmBaseUrl: "",
        llmModel: "",
        asrApiKey: "asr-key",
      }),
    });

    expect(result.valid).toBe(false);
    expect(result.missingLabels).toEqual(["LLM 接口地址", "LLM 模型"]);
  });
});

describe("isProviderFieldRequired", () => {
  it("marks LLM base URL as required only for OpenAI compatible", () => {
    expect(
      isProviderFieldRequired(
        "llmBaseUrl",
        createForm({ llmProvider: "openai_compatible" }),
      ),
    ).toBe(true);
    expect(
      isProviderFieldRequired("llmBaseUrl", createForm({ llmProvider: "openai" })),
    ).toBe(false);
  });
});
