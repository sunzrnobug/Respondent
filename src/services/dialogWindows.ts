import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isTauriRuntime } from "./realtimeBridge";

export type DialogWindowKind =
  | "appearance"
  | "conversation-history"
  | "providers"
  | "reply-style"
  | "save-session"
  | "documents";

type DialogWindowSpec = {
  label: string;
  title: string;
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
};

const DIALOG_SPECS: Record<DialogWindowKind, DialogWindowSpec> = {
  appearance: {
    label: "dialog-appearance",
    title: "外观",
    width: 440,
    height: 360,
    minWidth: 360,
    minHeight: 300,
  },
  "conversation-history": {
    label: "dialog-conversation-history",
    title: "会话历史",
    width: 860,
    height: 620,
    minWidth: 680,
    minHeight: 500,
  },
  providers: {
    label: "dialog-providers",
    title: "服务商配置",
    width: 640,
    height: 720,
    minWidth: 520,
    minHeight: 560,
  },
  "reply-style": {
    label: "dialog-reply-style",
    title: "回复风格",
    width: 480,
    height: 480,
    minWidth: 420,
    minHeight: 360,
  },
  "save-session": {
    label: "dialog-save-session",
    title: "保存会话",
    width: 440,
    height: 360,
    minWidth: 380,
    minHeight: 300,
  },
  documents: {
    label: "dialog-documents",
    title: "文档知识库",
    width: 460,
    height: 520,
    minWidth: 380,
    minHeight: 400,
  },
};

function dialogUrl(kind: DialogWindowKind): string {
  const url = new URL(window.location.href);
  url.searchParams.set("dialog", kind);
  url.hash = "";
  return `${url.pathname}${url.search}`;
}

export function dialogWindowOptions(kind: DialogWindowKind) {
  const spec = DIALOG_SPECS[kind];

  return {
    label: spec.label,
    window: {
      url: dialogUrl(kind),
      title: spec.title,
      width: spec.width,
      height: spec.height,
      minWidth: spec.minWidth,
      minHeight: spec.minHeight,
      resizable: true,
      center: true,
      focus: true,
      decorations: false,
      transparent: true,
      shadow: false,
      alwaysOnTop: true,
    },
  };
}

export async function openDialogWindow(
  kind: DialogWindowKind,
): Promise<boolean> {
  if (!isTauriRuntime()) return false;

  const options = dialogWindowOptions(kind);
  const existing = await WebviewWindow.getByLabel(options.label);
  if (existing) {
    await existing.show();
    await existing.setFocus();
    return true;
  }

  const dialog = new WebviewWindow(options.label, options.window);

  return new Promise((resolve, reject) => {
    void dialog.once("tauri://created", () => resolve(true));
    void dialog.once("tauri://error", (event) => reject(event.payload));
  });
}

export async function closeCurrentDialogWindow(): Promise<boolean> {
  if (!isTauriRuntime()) return false;

  const current = WebviewWindow.getCurrent();
  await current.close();
  return true;
}
