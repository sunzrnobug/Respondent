export const REPLY_STYLE_PRESETS_STORAGE_KEY = "respondent.replyStylePresets";

const MAX_PRESET_COUNT = 20;
const MAX_PRESET_NAME_CHARS = 40;
const MAX_USER_PROMPT_CHARS = 2000;

export type ReplyStylePreset = {
  id: string;
  name: string;
  userPrompt: string;
  updatedAtMs: number;
};

type ReplyStylePresetStore = {
  presets: ReplyStylePreset[];
};

function loadStore(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): ReplyStylePresetStore {
  const raw = storage.getItem(REPLY_STYLE_PRESETS_STORAGE_KEY);
  if (!raw) {
    return { presets: [] };
  }
  try {
    const parsed = JSON.parse(raw) as ReplyStylePresetStore;
    if (!Array.isArray(parsed.presets)) {
      return { presets: [] };
    }
    return {
      presets: parsed.presets.filter(
        (preset) =>
          typeof preset.id === "string" &&
          typeof preset.name === "string" &&
          typeof preset.userPrompt === "string",
      ),
    };
  } catch {
    return { presets: [] };
  }
}

function saveStore(
  store: ReplyStylePresetStore,
  storage: Pick<Storage, "setItem"> = window.localStorage,
) {
  storage.setItem(REPLY_STYLE_PRESETS_STORAGE_KEY, JSON.stringify(store));
}

function normalizeName(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) {
    throw new Error("预设名称不能为空");
  }
  if (trimmed.length > MAX_PRESET_NAME_CHARS) {
    throw new Error(`预设名称不能超过 ${MAX_PRESET_NAME_CHARS} 字符`);
  }
  return trimmed;
}

function normalizePrompt(userPrompt: string): string {
  const trimmed = userPrompt.trim();
  if (!trimmed) {
    throw new Error("请先填写回复风格提示词");
  }
  if (trimmed.length > MAX_USER_PROMPT_CHARS) {
    throw new Error(`回复风格提示词不能超过 ${MAX_USER_PROMPT_CHARS} 字符`);
  }
  return trimmed;
}

export function listReplyStylePresets(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): ReplyStylePreset[] {
  return [...loadStore(storage).presets].sort(
    (left, right) => right.updatedAtMs - left.updatedAtMs,
  );
}

export function saveReplyStylePreset(
  name: string,
  userPrompt: string,
  presetId: string | null = null,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): ReplyStylePreset[] {
  const store = loadStore(storage);
  const normalizedName = normalizeName(name);
  const normalizedPrompt = normalizePrompt(userPrompt);
  const now = Date.now();

  if (presetId) {
    const index = store.presets.findIndex((preset) => preset.id === presetId);
    if (index < 0) {
      throw new Error("未找到该回复风格预设");
    }
    if (
      store.presets.some(
        (preset) => preset.id !== presetId && preset.name === normalizedName,
      )
    ) {
      throw new Error("预设名称已存在");
    }
    store.presets[index] = {
      ...store.presets[index],
      name: normalizedName,
      userPrompt: normalizedPrompt,
      updatedAtMs: now,
    };
  } else {
    if (store.presets.some((preset) => preset.name === normalizedName)) {
      throw new Error("预设名称已存在");
    }
    if (store.presets.length >= MAX_PRESET_COUNT) {
      throw new Error(`最多保存 ${MAX_PRESET_COUNT} 个回复风格预设`);
    }
    store.presets.push({
      id: crypto.randomUUID(),
      name: normalizedName,
      userPrompt: normalizedPrompt,
      updatedAtMs: now,
    });
  }

  saveStore(store, storage);
  return listReplyStylePresets(storage);
}

export function deleteReplyStylePreset(
  presetId: string,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): ReplyStylePreset[] {
  const store = loadStore(storage);
  const nextPresets = store.presets.filter((preset) => preset.id !== presetId);
  if (nextPresets.length === store.presets.length) {
    throw new Error("未找到该回复风格预设");
  }
  saveStore({ presets: nextPresets }, storage);
  return listReplyStylePresets(storage);
}
