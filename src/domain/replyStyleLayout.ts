export const REPLY_STYLE_TEXTAREA_MIN_HEIGHT_PX = 90;
export const REPLY_STYLE_TEXTAREA_MAX_HEIGHT_PX = 160;
export const REPLY_STYLE_PRESETS_PER_PAGE = 3;

export function estimateReplyStyleWindowHeight(textareaHeightPx: number): number {
  const textareaHeight = Math.min(
    REPLY_STYLE_TEXTAREA_MAX_HEIGHT_PX,
    Math.max(REPLY_STYLE_TEXTAREA_MIN_HEIGHT_PX, textareaHeightPx),
  );
  /** Header, footer, intro, hints, examples, preset controls, up to 3 presets, pagination. */
  const chromeHeightPx = 430;
  return chromeHeightPx + textareaHeight;
}
