import { Download, Trash2, X } from "lucide-react";
import { MarkdownContent } from "./MarkdownContent";
import type { SavedSession } from "../state/sessionHistory";

export type ExportStatus =
  | { kind: "loading"; message: string }
  | { kind: "success"; path: string; filename: string }
  | { kind: "info"; message: string }
  | { kind: "error"; message: string };

type ConversationHistoryPanelProps = {
  savedSessions: SavedSession[];
  activeSession: SavedSession | null;
  onSelectSession: (session: SavedSession) => void;
  onDeleteSession: (sessionId: string) => void;
  onExportMarkdown: (session: SavedSession) => void;
  onRevealExportedFile?: (path: string) => void;
  onClose: () => void;
  exportStatus?: ExportStatus | null;
  closeTitle?: string;
  className?: string;
};

export function ConversationHistoryPanel({
  savedSessions,
  activeSession,
  onSelectSession,
  onDeleteSession,
  onExportMarkdown,
  onRevealExportedFile,
  onClose,
  exportStatus = null,
  closeTitle = "关闭会话历史",
  className = "modalPanel conversationHistoryPanel",
}: ConversationHistoryPanelProps) {
  return (
    <section
      aria-labelledby="conversation-history-title"
      className={className}
      role="dialog"
    >
      <div className="modalHeader">
        <div>
          <h2 id="conversation-history-title">会话历史</h2>
          <div className="configStatus">
            已保存 {savedSessions.length} 条长会话
          </div>
        </div>
        <button type="button" onClick={onClose} title={closeTitle}>
          <X size={16} />
        </button>
      </div>
      <div className="conversationHistoryLayout">
        <aside className="conversationList" aria-label="已保存的会话">
          {savedSessions.length === 0 ? (
            <p>暂无已保存的会话。</p>
          ) : (
            <ul className="conversationListItems">
              {savedSessions.map((item) => (
                <li className="conversationListItemRow" key={item.id}>
                  <button
                    className={
                      activeSession?.id === item.id
                        ? "conversationListItem selected"
                        : "conversationListItem"
                    }
                    type="button"
                    onClick={() => onSelectSession(item)}
                  >
                    <span>{item.title}</span>
                    <time dateTime={item.endedAt}>{item.date}</time>
                  </button>
                  <button
                    className="conversationListDelete"
                    type="button"
                    aria-label="删除"
                    title={`删除 ${item.title}`}
                    onClick={() => onDeleteSession(item.id)}
                  >
                    <Trash2 size={15} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>
        <section className="conversationDetail" aria-label="会话详情">
          {activeSession ? (
            <>
              <div className="conversationDetailHeader">
                <div>
                  <div className="conversationTitle">{activeSession.title}</div>
                  <div className="configStatus">
                    {activeSession.date} · {activeSession.turns.length} 轮对话
                  </div>
                </div>
                <button
                  className="primaryButton"
                  type="button"
                  onClick={() => onExportMarkdown(activeSession)}
                >
                  <Download size={15} />
                  导出 Markdown
                </button>
              </div>
              {exportStatus ? (
                <div className="configStatus exportStatus">
                  {exportStatus.kind === "success" ? (
                    <>
                      <span className="exportStatusPrefix">已导出：</span>
                      <button
                        className="exportStatusLink"
                        type="button"
                        title={`在文件管理器中显示 ${exportStatus.filename}`}
                        onClick={() =>
                          onRevealExportedFile?.(exportStatus.path)
                        }
                      >
                        {exportStatus.filename}
                      </button>
                    </>
                  ) : (
                    exportStatus.message
                  )}
                </div>
              ) : null}
              <div className="historyTimeline">
                {activeSession.turns.map((turn, index) => (
                  <article
                    className="historyTurn"
                    key={`${turn.transcript}-${index}`}
                  >
                    <div className="historyTurnUser">
                      <span className="historyTurnLabel">转写</span>
                      <div className="chatBubble userBubble">
                        <p>{turn.transcript}</p>
                      </div>
                    </div>
                    {turn.suggestion ? (
                      <div className="historyTurnAssistant">
                        <span className="historyTurnLabel">建议回复</span>
                        <MarkdownContent className="historyMdReply">
                          {turn.suggestion}
                        </MarkdownContent>
                      </div>
                    ) : null}
                  </article>
                ))}
              </div>
            </>
          ) : (
            <div className="emptyConversation">
              保存会话后，可在这里查看完整长会话内容。
            </div>
          )}
        </section>
      </div>
    </section>
  );
}
