import { describe, expect, it } from "vitest";
import {
  estimateReplyStyleWindowHeight,
  REPLY_STYLE_TEXTAREA_MAX_HEIGHT_PX,
  REPLY_STYLE_TEXTAREA_MIN_HEIGHT_PX,
} from "./replyStyleLayout";

describe("reply style layout", () => {
  it("caps the estimated window height to the textarea maximum", () => {
    expect(
      estimateReplyStyleWindowHeight(REPLY_STYLE_TEXTAREA_MIN_HEIGHT_PX),
    ).toBeLessThan(
      estimateReplyStyleWindowHeight(REPLY_STYLE_TEXTAREA_MAX_HEIGHT_PX + 100),
    );
    expect(
      estimateReplyStyleWindowHeight(REPLY_STYLE_TEXTAREA_MAX_HEIGHT_PX + 100),
    ).toBe(estimateReplyStyleWindowHeight(REPLY_STYLE_TEXTAREA_MAX_HEIGHT_PX));
  });
});
