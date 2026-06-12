import { describe, expect, it } from "vitest";
import { exportMarkdown, exportPlainText, type SessionExport } from "./exportTranscript";

const session: SessionExport = {
  title: "Customer call",
  startedAt: "2026-06-12T08:00:00.000Z",
  endedAt: "2026-06-12T08:05:00.000Z",
  events: [
    { type: "transcript", text: "What is the timeline?", atMs: 1200 },
    { type: "suggestion", text: "We can deliver the first draft by Friday.", atMs: 2100 },
  ],
};

describe("session export", () => {
  it("exports Markdown with timestamps and suggestions", () => {
    expect(exportMarkdown(session)).toContain("## Customer call");
    expect(exportMarkdown(session)).toContain("[00:01.200] Transcript: What is the timeline?");
    expect(exportMarkdown(session)).toContain(
      "[00:02.100] Suggestion: We can deliver the first draft by Friday.",
    );
  });

  it("exports plain text", () => {
    expect(exportPlainText(session)).toBe(
      "Customer call\nStarted: 2026-06-12T08:00:00.000Z\nEnded: 2026-06-12T08:05:00.000Z\n\n[00:01.200] Transcript: What is the timeline?\n[00:02.100] Suggestion: We can deliver the first draft by Friday.\n",
    );
  });
});
