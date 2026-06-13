import { describe, expect, it } from "vitest";
import {
  activateLocalProviderProfile,
  deleteLocalProviderProfile,
  listLocalProviderProfiles,
  saveLocalProviderProfile,
} from "./providerProfiles";

describe("local provider profiles", () => {
  it("saves, activates, and deletes named profiles", () => {
    const storage = new Map<string, string>();
    const api = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
    };

    const first = saveLocalProviderProfile(
      "OpenAI",
      null,
      {
        llm: {
          provider: "openai",
          apiKey: "sk-test",
          baseUrl: null,
          model: "gpt-5.4-mini",
        },
        asr: null,
      },
      api,
    );
    expect(first.profiles).toHaveLength(1);
    expect(first.profiles[0]?.name).toBe("OpenAI");

    const second = saveLocalProviderProfile(
      "DashScope",
      null,
      {
        llm: {
          provider: "dashscope",
          apiKey: "ds-key",
          baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
          model: "qwen-plus",
        },
        asr: null,
      },
      api,
    );
    expect(second.profiles).toHaveLength(2);
    expect(second.profiles.find((profile) => profile.isActive)?.name).toBe(
      "DashScope",
    );

    const activeId = first.profiles[0]?.id;
    const switched = activateLocalProviderProfile(activeId, api);
    expect(switched.profiles.find((profile) => profile.isActive)?.name).toBe(
      "OpenAI",
    );

    const deleted = deleteLocalProviderProfile(
      second.profiles.find((profile) => profile.name === "DashScope")!.id,
      api,
    );
    expect(deleted.profiles).toHaveLength(1);
    expect(listLocalProviderProfiles(api).profiles[0]?.name).toBe("OpenAI");
  });
});
