import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("window chrome styles", () => {
  it("does not draw an outer shell shadow in the transparent window", () => {
    const css = readFileSync("src/styles.css", "utf8");
    const shellRule = css.match(/\.shell\s*\{[\s\S]*?\n\}/)?.[0] ?? "";

    expect(shellRule).not.toContain("0 24px 70px");
  });
});
