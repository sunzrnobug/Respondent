import { describe, expect, it } from "vitest";
import { isEditableTarget, isNumpadEnter } from "./windowVisibility";

describe("window visibility hotkey", () => {
  it("accepts numpad Enter", () => {
    expect(
      isNumpadEnter(
        new KeyboardEvent("keydown", {
          key: "Enter",
          code: "NumpadEnter",
          location: KeyboardEvent.DOM_KEY_LOCATION_NUMPAD,
        }),
      ),
    ).toBe(true);
  });

  it("ignores the main keyboard Enter key", () => {
    expect(
      isNumpadEnter(
        new KeyboardEvent("keydown", {
          key: "Enter",
          code: "Enter",
          location: KeyboardEvent.DOM_KEY_LOCATION_STANDARD,
        }),
      ),
    ).toBe(false);
  });

  it("treats form fields as editable targets", () => {
    const input = document.createElement("input");
    expect(isEditableTarget(input)).toBe(true);
  });
});
