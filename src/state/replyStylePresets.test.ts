import { describe, expect, it } from "vitest";
import {
  deleteReplyStylePreset,
  listReplyStylePresets,
  saveReplyStylePreset,
} from "./replyStylePresets";

function createStorage() {
  const storage = new Map<string, string>();
  return {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => {
      storage.set(key, value);
    },
  };
}

describe("reply style presets", () => {
  it("saves, lists, updates, and deletes named presets", () => {
    const storage = createStorage();

    saveReplyStylePreset("面试回答", "先结论后原因", null, storage);
    saveReplyStylePreset("技术答辩", "解释设计与风险", null, storage);

    let presets = listReplyStylePresets(storage);
    expect(presets).toHaveLength(2);
    expect(presets.map((preset) => preset.name).sort()).toEqual([
      "技术答辩",
      "面试回答",
    ]);

    const interviewPreset = presets.find((preset) => preset.name === "面试回答");
    expect(interviewPreset).toBeDefined();

    saveReplyStylePreset(
      "面试强化",
      "先结论，再项目经验",
      interviewPreset!.id,
      storage,
    );

    presets = listReplyStylePresets(storage);
    expect(presets.find((preset) => preset.id === interviewPreset!.id)?.name).toBe(
      "面试强化",
    );

    deleteReplyStylePreset(interviewPreset!.id, storage);
    presets = listReplyStylePresets(storage);
    expect(presets).toHaveLength(1);
    expect(presets[0]?.name).toBe("技术答辩");
  });

  it("rejects duplicate names and empty prompts", () => {
    const storage = createStorage();

    saveReplyStylePreset("详细解释", "分点说明", null, storage);
    expect(() =>
      saveReplyStylePreset("详细解释", "另一段提示词", null, storage),
    ).toThrow("预设名称已存在");

    expect(() => saveReplyStylePreset("空白", "   ", null, storage)).toThrow(
      "请先填写回复风格提示词",
    );
  });
});
