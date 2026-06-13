import dashscopeLogo from "./providers/dashscope.svg";
import openaiCompatibleLogo from "./providers/openai-compatible.svg";
import openaiLogo from "./providers/openai.svg";
import siliconflowLogo from "./providers/siliconflow.svg";
import zhipuLogo from "./providers/zhipu.svg";

export type ProviderLogoKey =
  | "openai"
  | "dashscope"
  | "zhipu"
  | "siliconflow"
  | "openai_compatible";

export const PROVIDER_LOGOS: Record<ProviderLogoKey, string> = {
  openai: openaiLogo,
  dashscope: dashscopeLogo,
  zhipu: zhipuLogo,
  siliconflow: siliconflowLogo,
  openai_compatible: openaiCompatibleLogo,
};

export function resolveProviderLogo(
  providerValue: string,
): string | undefined {
  if (providerValue in PROVIDER_LOGOS) {
    return PROVIDER_LOGOS[providerValue as ProviderLogoKey];
  }

  if (providerValue.startsWith("openai")) {
    return PROVIDER_LOGOS.openai;
  }

  if (providerValue.includes("bailian") || providerValue.includes("dashscope")) {
    return PROVIDER_LOGOS.dashscope;
  }

  if (providerValue.includes("siliconflow")) {
    return PROVIDER_LOGOS.siliconflow;
  }

  if (providerValue.includes("zhipu")) {
    return PROVIDER_LOGOS.zhipu;
  }

  return undefined;
}
