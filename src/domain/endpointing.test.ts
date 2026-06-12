import { describe, expect, it } from "vitest";
import { chooseEndpointSilenceMs } from "./endpointing";

describe("chooseEndpointSilenceMs", () => {
  it("uses 300 ms for balanced clean speech", () => {
    expect(
      chooseEndpointSilenceMs({
        noiseLevel: "low",
        recentFalseCuts: 0,
        utteranceMs: 1800,
      }),
    ).toBe(300);
  });

  it("uses 250 ms for very short clean utterances", () => {
    expect(
      chooseEndpointSilenceMs({
        noiseLevel: "low",
        recentFalseCuts: 0,
        utteranceMs: 650,
      }),
    ).toBe(250);
  });

  it("widens to 500 ms after repeated false cuts", () => {
    expect(
      chooseEndpointSilenceMs({
        noiseLevel: "medium",
        recentFalseCuts: 2,
        utteranceMs: 2400,
      }),
    ).toBe(500);
  });
});
