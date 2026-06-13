import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("main window capability", () => {
  it("allows the frontend to resize the transparent main window", () => {
    const capability = JSON.parse(
      readFileSync("src-tauri/capabilities/default.json", "utf8"),
    );

    expect(capability.permissions).toContain("core:window:allow-set-size");
    expect(capability.permissions).toContain("core:window:allow-inner-size");
    expect(capability.permissions).toContain("core:window:allow-scale-factor");
  });
});
