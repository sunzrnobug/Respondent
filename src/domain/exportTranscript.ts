export type SessionExportEvent = {
  type: "transcript" | "suggestion" | "system";
  text: string;
  atMs: number;
};

export type SessionExport = {
  title: string;
  startedAt: string;
  endedAt: string;
  events: SessionExportEvent[];
};

function formatTimestamp(atMs: number): string {
  const minutes = Math.floor(atMs / 60_000)
    .toString()
    .padStart(2, "0");
  const seconds = Math.floor((atMs % 60_000) / 1_000)
    .toString()
    .padStart(2, "0");
  const milliseconds = Math.floor(atMs % 1_000)
    .toString()
    .padStart(3, "0");

  return `${minutes}:${seconds}.${milliseconds}`;
}

function eventLabel(type: SessionExportEvent["type"]): string {
  if (type === "transcript") return "转写";
  if (type === "suggestion") return "建议回复";
  return "系统";
}

function eventLine(event: SessionExportEvent): string {
  return `[${formatTimestamp(event.atMs)}] ${eventLabel(event.type)}：${event.text}`;
}

export function exportPlainText(session: SessionExport): string {
  const header = `${session.title}\n开始：${session.startedAt}\n结束：${session.endedAt}\n\n`;
  const body = session.events.map(eventLine).join("\n");

  return `${header}${body}\n`;
}

export function exportMarkdown(session: SessionExport): string {
  return [
    `## ${session.title}`,
    "",
    `- 开始：${session.startedAt}`,
    `- 结束：${session.endedAt}`,
    "",
    "### 时间线",
    "",
    ...session.events.map((event) => `- ${eventLine(event)}`),
    "",
  ].join("\n");
}
