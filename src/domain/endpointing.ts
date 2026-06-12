export type NoiseLevel = "low" | "medium" | "high";

export type EndpointPolicyInput = {
  noiseLevel: NoiseLevel;
  recentFalseCuts: number;
  utteranceMs: number;
};

export function chooseEndpointSilenceMs(input: EndpointPolicyInput): number {
  if (input.noiseLevel === "high" || input.recentFalseCuts >= 2) return 500;
  if (input.noiseLevel === "medium" || input.recentFalseCuts === 1) return 400;
  if (input.utteranceMs <= 900) return 250;
  return 300;
}
