import {
  Bookmark,
  MessageSquare,
  Sparkles,
  X,
} from "lucide-react";
import { summarizeSessionTitle } from "../state/sessionHistory";
import { buildSessionTurns, type SessionState } from "../state/sessionStore";

function hasSessionContent(session: SessionState): boolean {
  return (
    session.transcript.length > 0 ||
    session.suggestions.length > 0 ||
    session.currentSuggestion.trim().length > 0 ||
    session.systemMessages.length > 0
  );
}

function truncatePreview(text: string, max = 80): string {
  const trimmed = text.replace(/\s+/g, " ").trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max)}…`;
}

type SaveSessionPanelProps = {
  session: SessionState;
  onSave: () => void;
  onDiscard: () => void;
  onClose: () => void;
  closeTitle?: string;
  className?: string;
  ariaModal?: boolean | "true" | "false";
};

export function SaveSessionPanel({
  session,
  onSave,
  onDiscard,
  onClose,
  closeTitle = "关闭保存提示",
  className = "modalPanel saveSessionPanel",
  ariaModal,
}: SaveSessionPanelProps) {
  const hasContent = hasSessionContent(session);
  const turns = buildSessionTurns(session);
  const suggestionCount = turns.filter((turn) => turn.suggestion?.trim()).length;
  const previewTitle = hasContent
    ? summarizeSessionTitle(session)
    : "未命名会话";
  const previewSnippet = session.transcript[0] ?? session.systemMessages[0] ?? "";

  return (
    <section
      aria-labelledby="save-session-title"
      aria-modal={ariaModal}
      className={className}
      role="dialog"
    >
      <div className="modalHeader">
        <div>
          <h2 id="save-session-title">保存会话</h2>
          <div className="configStatus">
            {hasContent
              ? "保存后可在会话历史中查看完整记录"
              : "本次会话还没有可保存的内容"}
          </div>
        </div>
        <button type="button" onClick={onClose} title={closeTitle}>
          <X size={16} />
        </button>
      </div>

      <div className="saveSessionBody">
        <div className="saveSessionHero">
          <div
            className={
              hasContent
                ? "saveSessionIcon"
                : "saveSessionIcon saveSessionIconMuted"
            }
            aria-hidden="true"
          >
            <Bookmark size={22} />
          </div>
          <p className="saveSessionQuestion">本次会话记录是否保存</p>
        </div>

        {hasContent ? (
          <div className="saveSessionPreview">
            <div className="saveSessionPreviewLabel">自动标题预览</div>
            <div className="saveSessionPreviewTitle">{previewTitle}</div>
            <div className="saveSessionStats" aria-label="会话统计">
              <div className="saveSessionStat">
                <MessageSquare size={14} aria-hidden="true" />
                <span>{turns.length} 轮转写</span>
              </div>
              {suggestionCount > 0 ? (
                <div className="saveSessionStat">
                  <Sparkles size={14} aria-hidden="true" />
                  <span>{suggestionCount} 条建议</span>
                </div>
              ) : null}
            </div>
            {previewSnippet ? (
              <blockquote className="saveSessionSnippet">
                {truncatePreview(previewSnippet)}
              </blockquote>
            ) : null}
          </div>
        ) : (
          <div className="saveSessionEmpty">
            <p className="saveSessionEmptyTitle">暂无转写内容</p>
            <p className="saveSessionEmptyHint">
              开始会话并产生转写后，可在此保存完整记录。
            </p>
          </div>
        )}
      </div>

      <div className="modalFooter saveSessionFooter">
        <button
          className="secondaryButton"
          type="button"
          onClick={onDiscard}
        >
          暂不保存
        </button>
        <button
          className="primaryButton saveSessionPrimaryButton"
          type="button"
          onClick={onSave}
          disabled={!hasContent}
        >
          <Bookmark size={15} aria-hidden="true" />
          保存会话
        </button>
      </div>
    </section>
  );
}
