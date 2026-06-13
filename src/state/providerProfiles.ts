import type {
  ProviderConfigSummary,
  ProviderSettingsPayload,
} from "../services/tauriApi";

export const PROVIDER_PROFILES_STORAGE_KEY = "respondent.providerProfiles";

export type ProviderProfileRecord = {
  id: string;
  name: string;
  settings: ProviderSettingsPayload;
  updatedAtMs: number;
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

type ProviderProfileStore = {
  activeProfileId: string | null;
  profiles: ProviderProfileRecord[];
};

function summaryFromSettings(
  settings: ProviderSettingsPayload,
): ProviderConfigSummary {
  return {
    llm: settings.llm
      ? {
          provider: settings.llm.provider,
          hasApiKey: Boolean(settings.llm.apiKey?.trim()),
          baseUrl: settings.llm.baseUrl ?? null,
          model: settings.llm.model ?? null,
        }
      : null,
    asr: settings.asr
      ? {
          provider: settings.asr.provider,
          hasApiKey: Boolean(settings.asr.apiKey?.trim()),
          baseUrl: settings.asr.baseUrl ?? null,
          model: settings.asr.model ?? null,
          languageHint: settings.asr.languageHint ?? null,
          maxSentenceSilenceMs: settings.asr.maxSentenceSilenceMs ?? null,
          heartbeat: settings.asr.heartbeat ?? null,
        }
      : null,
  };
}

function mergeSettings(
  existing: ProviderSettingsPayload,
  update: ProviderSettingsPayload,
): ProviderSettingsPayload {
  const mergeLlm = () => {
    if (!update.llm) return existing.llm ?? null;
    const current = existing.llm;
    if (!current || current.provider !== update.llm.provider) {
      return update.llm;
    }
    return {
      ...update.llm,
      apiKey: update.llm.apiKey?.trim() ? update.llm.apiKey : current.apiKey,
    };
  };

  const mergeAsr = () => {
    if (!update.asr) return existing.asr ?? null;
    const current = existing.asr;
    if (!current || current.provider !== update.asr.provider) {
      return update.asr;
    }
    return {
      ...update.asr,
      apiKey: update.asr.apiKey?.trim() ? update.asr.apiKey : current.apiKey,
    };
  };

  return {
    llm: mergeLlm(),
    asr: mergeAsr(),
  };
}

function loadStore(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): ProviderProfileStore {
  const raw = storage.getItem(PROVIDER_PROFILES_STORAGE_KEY);
  if (!raw) {
    return { activeProfileId: null, profiles: [] };
  }
  try {
    return JSON.parse(raw) as ProviderProfileStore;
  } catch {
    return { activeProfileId: null, profiles: [] };
  }
}

function saveStore(
  store: ProviderProfileStore,
  storage: Pick<Storage, "setItem"> = window.localStorage,
) {
  storage.setItem(PROVIDER_PROFILES_STORAGE_KEY, JSON.stringify(store));
}

function toResponse(store: ProviderProfileStore): ProviderProfilesResponse {
  const activeId = store.activeProfileId;
  const activeProfile = store.profiles.find((profile) => profile.id === activeId);
  return {
    profiles: store.profiles.map((profile) => ({
      id: profile.id,
      name: profile.name,
      isActive: profile.id === activeId,
      summary: summaryFromSettings(profile.settings),
    })),
    active: summaryFromSettings(
      activeProfile?.settings ?? { llm: null, asr: null },
    ),
  };
}

function normalizeName(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) {
    throw new Error("服务商配置名称不能为空");
  }
  return trimmed;
}

export function listLocalProviderProfiles(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): ProviderProfilesResponse {
  return toResponse(loadStore(storage));
}

export function saveLocalProviderProfile(
  name: string,
  profileId: string | null,
  payload: ProviderSettingsPayload,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): ProviderProfilesResponse {
  const store = loadStore(storage);
  const normalizedName = normalizeName(name);
  const now = Date.now();

  if (profileId) {
    const index = store.profiles.findIndex((profile) => profile.id === profileId);
    if (index < 0) {
      throw new Error("未找到该服务商配置");
    }
    if (
      store.profiles.some(
        (profile) => profile.id !== profileId && profile.name === normalizedName,
      )
    ) {
      throw new Error("服务商配置名称已存在");
    }
    store.profiles[index] = {
      ...store.profiles[index],
      name: normalizedName,
      settings: mergeSettings(store.profiles[index].settings, payload),
      updatedAtMs: now,
    };
    store.activeProfileId = profileId;
  } else {
    if (store.profiles.some((profile) => profile.name === normalizedName)) {
      throw new Error("服务商配置名称已存在");
    }
    const profile: ProviderProfileRecord = {
      id: crypto.randomUUID(),
      name: normalizedName,
      settings: payload,
      updatedAtMs: now,
    };
    store.profiles.push(profile);
    store.activeProfileId = profile.id;
  }

  saveStore(store, storage);
  return toResponse(store);
}

export function activateLocalProviderProfile(
  profileId: string,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): ProviderProfilesResponse {
  const store = loadStore(storage);
  if (!store.profiles.some((profile) => profile.id === profileId)) {
    throw new Error("未找到该服务商配置");
  }
  store.activeProfileId = profileId;
  saveStore(store, storage);
  return toResponse(store);
}

export function deleteLocalProviderProfile(
  profileId: string,
  storage: Pick<Storage, "getItem" | "setItem"> = window.localStorage,
): ProviderProfilesResponse {
  const store = loadStore(storage);
  const originalLength = store.profiles.length;
  store.profiles = store.profiles.filter((profile) => profile.id !== profileId);
  if (store.profiles.length === originalLength) {
    throw new Error("未找到该服务商配置");
  }
  if (store.activeProfileId === profileId) {
    store.activeProfileId = store.profiles[0]?.id ?? null;
  }
  saveStore(store, storage);
  return toResponse(store);
}
