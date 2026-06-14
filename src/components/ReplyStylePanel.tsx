import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  Bookmark,
  ChevronLeft,
  ChevronRight,
  MessageSquareText,
  Save,
  Trash2,
  X,
} from "lucide-react";
import {
  getReplyStyleSettings,
  saveReplyStyleSettings,
} from "../services/tauriApi";
import {
  deleteReplyStylePreset,
  listReplyStylePresets,
  saveReplyStylePreset,
  type ReplyStylePreset,
} from "../state/replyStylePresets";
import { setupDialogWindowFit } from "../services/windowFit";
import {
  REPLY_STYLE_PRESETS_PER_PAGE,
  REPLY_STYLE_TEXTAREA_MAX_HEIGHT_PX,
  REPLY_STYLE_TEXTAREA_MIN_HEIGHT_PX,
} from "../domain/replyStyleLayout";

const REPLY_STYLE_EXAMPLES = [
  {
    label: "详细解释",
    text: "请回答得更详细、有层次。先直接给出结论，再分 2-3 点解释原因。如果涉及技术方案，要说明取舍、风险和适用场景。保持口语化，像我在会议里自然说出来。",
  },
  {
    label: "面试回答",
    text: "请按面试回答的方式组织：先给明确结论，再结合项目经验解释原因，最后补一句权衡或总结。回答要专业、有逻辑，但不要太书面化。",
  },
  {
    label: "技术答辩",
    text: "请用技术答辩风格回答。优先解释设计动机、核心机制、边界情况和风险控制。如果问题很宽泛，先给整体判断，再展开关键点。",
  },
] as const;

function syncTextareaHeight(textarea: HTMLTextAreaElement) {
  textarea.style.height = "auto";
  const nextHeight = Math.min(
    REPLY_STYLE_TEXTAREA_MAX_HEIGHT_PX,
    Math.max(REPLY_STYLE_TEXTAREA_MIN_HEIGHT_PX, textarea.scrollHeight),
  );
  textarea.style.height = `${nextHeight}px`;
}

type ReplyStylePanelProps = {
  onClose: () => void;
  closeTitle?: string;
  className?: string;
  fitWindow?: boolean;
};

export function ReplyStylePanel({
  onClose,
  closeTitle = "关闭回复风格设置",
  className = "modalPanel replyStylePanel",
  fitWindow = false,
}: ReplyStylePanelProps) {
  const panelRef = useRef<HTMLElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [replyStylePrompt, setReplyStylePrompt] = useState("");
  const [status, setStatus] = useState("");
  const [saving, setSaving] = useState(false);
  const [presets, setPresets] = useState<ReplyStylePreset[]>(() =>
    listReplyStylePresets(),
  );
  const [presetName, setPresetName] = useState("");
  const [editingPresetId, setEditingPresetId] = useState<string | null>(null);
  const [savingPreset, setSavingPreset] = useState(false);
  const [presetPage, setPresetPage] = useState(0);

  const presetPageCount = Math.max(
    1,
    Math.ceil(presets.length / REPLY_STYLE_PRESETS_PER_PAGE),
  );
  const pagedPresets = useMemo(() => {
    const start = presetPage * REPLY_STYLE_PRESETS_PER_PAGE;
    return presets.slice(start, start + REPLY_STYLE_PRESETS_PER_PAGE);
  }, [presets, presetPage]);

  useEffect(() => {
    setPresetPage((page) => Math.min(page, presetPageCount - 1));
  }, [presetPageCount]);

  useLayoutEffect(() => {
    if (textareaRef.current) {
      syncTextareaHeight(textareaRef.current);
    }
  }, [replyStylePrompt]);

  useEffect(() => {
    if (!fitWindow) {
      return undefined;
    }
    return setupDialogWindowFit(panelRef.current);
  }, [fitWindow]);

  useEffect(() => {
    void getReplyStyleSettings()
      .then((settings) => {
        setReplyStylePrompt(settings.userPrompt ?? "");
      })
      .catch(() => {
        setStatus("加载回复风格设置失败");
      });
  }, []);

  async function saveReplyStyle() {
    setSaving(true);
    setStatus("");
    try {
      const saved = await saveReplyStyleSettings({
        userPrompt: replyStylePrompt,
      });
      setReplyStylePrompt(saved.userPrompt);
      setStatus("回复风格已保存");
    } catch (error) {
      setStatus(
        error instanceof Error ? error.message : "保存回复风格失败",
      );
    } finally {
      setSaving(false);
    }
  }

  function applyExamplePrompt(text: string) {
    setReplyStylePrompt(text);
    setEditingPresetId(null);
    setPresetName("");
    setStatus("");
  }

  function applyPreset(preset: ReplyStylePreset) {
    setReplyStylePrompt(preset.userPrompt);
    setEditingPresetId(preset.id);
    setPresetName(preset.name);
    setStatus("");
  }

  function savePreset() {
    setSavingPreset(true);
    setStatus("");
    try {
      const nextPresets = saveReplyStylePreset(
        presetName,
        replyStylePrompt,
        editingPresetId,
      );
      setPresets(nextPresets);
      const savedPreset = nextPresets.find(
        (preset) =>
          preset.id === editingPresetId ||
          preset.name === presetName.trim(),
      );
      if (savedPreset) {
        setEditingPresetId(savedPreset.id);
        setPresetName(savedPreset.name);
        const savedIndex = nextPresets.findIndex(
          (preset) => preset.id === savedPreset.id,
        );
        if (savedIndex >= 0) {
          setPresetPage(Math.floor(savedIndex / REPLY_STYLE_PRESETS_PER_PAGE));
        }
      }
      setStatus(editingPresetId ? "预设已更新" : "预设已保存");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "保存预设失败");
    } finally {
      setSavingPreset(false);
    }
  }

  function removePreset(presetId: string) {
    setStatus("");
    try {
      const deletedIndex = presets.findIndex((preset) => preset.id === presetId);
      const nextPresets = deleteReplyStylePreset(presetId);
      setPresets(nextPresets);
      if (deletedIndex >= 0) {
        const nextPageCount = Math.max(
          1,
          Math.ceil(nextPresets.length / REPLY_STYLE_PRESETS_PER_PAGE),
        );
        setPresetPage((page) =>
          Math.min(page, Math.max(0, nextPageCount - 1)),
        );
      }
      if (editingPresetId === presetId) {
        setEditingPresetId(null);
        setPresetName("");
      }
      setStatus("预设已删除");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "删除预设失败");
    }
  }

  return (
    <section
      ref={panelRef}
      aria-labelledby="reply-style-title"
      className={className}
      role="dialog"
    >
      <div className="modalHeader">
        <div>
          <h2 id="reply-style-title">回复风格</h2>
          <div className="configStatus">
            自定义 LLM 回复的语气、长度和结构
          </div>
        </div>
        <button type="button" onClick={onClose} title={closeTitle}>
          <X size={16} />
        </button>
      </div>

      <div className="replyStylePanelBody">
        <div className="replyStyleIntro">
          <div className="replyStyleIntroIcon" aria-hidden="true">
            <MessageSquareText size={16} />
          </div>
          <p>
            这段提示词只控制回复风格，不会覆盖系统安全规则。留空则使用默认风格。
          </p>
        </div>

        <label className="replyStyleField">
          <span>回复风格提示词</span>
          <textarea
            ref={textareaRef}
            className="replyStyleTextarea"
            value={replyStylePrompt}
            onChange={(event) => {
              setReplyStylePrompt(event.target.value);
              setStatus("");
              syncTextareaHeight(event.target);
            }}
            placeholder="例如：请回答得更详细、有层次。先给结论，再分 2-3 点说明原因。保持像真人在会议里说话。"
            maxLength={2000}
            rows={4}
          />
        </label>

        <div className="replyStyleHint">
          每次 LLM 请求会携带当前保存的设置；已开始的生成不受影响。
        </div>

        <div className="replyStyleExamples">
          <span className="replyStyleExamplesLabel">快捷示例</span>
          <div className="replyStyleExampleButtons">
            {REPLY_STYLE_EXAMPLES.map((example) => (
              <button
                key={example.label}
                type="button"
                className="replyStyleExampleButton"
                onClick={() => applyExamplePrompt(example.text)}
              >
                {example.label}
              </button>
            ))}
          </div>
        </div>

        <section
          className="replyStylePresetsSection"
          aria-label="已保存的回复风格预设"
        >
          <div className="replyStylePresetsHeader">
            <div className="replyStylePresetsHeaderIcon" aria-hidden="true">
              <Bookmark size={15} />
            </div>
            <div>
              <span className="replyStyleExamplesLabel">我的预设</span>
              <div className="replyStylePresetsMeta">
                {presets.length} 个已保存
              </div>
            </div>
          </div>

          <div className="replyStylePresetSaveRow">
            <input
              className="replyStylePresetNameInput"
              value={presetName}
              onChange={(event) => {
                const nextName = event.target.value;
                setPresetName(nextName);
                if (editingPresetId) {
                  const selectedPreset = presets.find(
                    (preset) => preset.id === editingPresetId,
                  );
                  if (selectedPreset && nextName.trim() !== selectedPreset.name) {
                    setEditingPresetId(null);
                  }
                }
                setStatus("");
              }}
              placeholder="输入预设名称，例如：产品评审"
              maxLength={40}
            />
            <button
              type="button"
              className="secondaryButton replyStylePresetSaveButton"
              disabled={savingPreset || !presetName.trim() || !replyStylePrompt.trim()}
              onClick={() => savePreset()}
            >
              {editingPresetId ? "更新预设" : "保存预设"}
            </button>
          </div>

          {presets.length === 0 ? (
            <div className="replyStylePresetsEmpty">
              保存常用提示词后，可在此一键选用。
            </div>
          ) : (
            <>
              <ul className="replyStylePresetsList">
                {pagedPresets.map((preset) => (
                  <li className="replyStylePresetItem" key={preset.id}>
                    <button
                      type="button"
                      className={
                        editingPresetId === preset.id
                          ? "replyStylePresetButton selected"
                          : "replyStylePresetButton"
                      }
                      onClick={() => applyPreset(preset)}
                      title={preset.userPrompt}
                    >
                      {preset.name}
                    </button>
                    <button
                      type="button"
                      className="replyStylePresetDelete"
                      title={`删除 ${preset.name}`}
                      onClick={() => removePreset(preset.id)}
                    >
                      <Trash2 size={14} />
                    </button>
                  </li>
                ))}
              </ul>
              {presets.length > REPLY_STYLE_PRESETS_PER_PAGE ? (
                <div className="replyStylePresetsPagination">
                  <button
                    type="button"
                    className="replyStylePresetsPageButton"
                    disabled={presetPage <= 0}
                    onClick={() =>
                      setPresetPage((page) => Math.max(0, page - 1))
                    }
                    title="上一页"
                  >
                    <ChevronLeft size={15} aria-hidden="true" />
                    上一页
                  </button>
                  <span className="replyStylePresetsPageStatus">
                    第 {presetPage + 1} / {presetPageCount} 页
                  </span>
                  <button
                    type="button"
                    className="replyStylePresetsPageButton"
                    disabled={presetPage >= presetPageCount - 1}
                    onClick={() =>
                      setPresetPage((page) =>
                        Math.min(presetPageCount - 1, page + 1),
                      )
                    }
                    title="下一页"
                  >
                    下一页
                    <ChevronRight size={15} aria-hidden="true" />
                  </button>
                </div>
              ) : null}
            </>
          )}
        </section>
      </div>

      <div className="modalFooter replyStylePanelFooter">
        {status ? <div className="configStatus">{status}</div> : <div />}
        <div className="replyStyleActionButtons">
          <button
            type="button"
            className="secondaryButton"
            onClick={() => {
              applyExamplePrompt("");
            }}
          >
            恢复默认
          </button>
          <button
            type="button"
            className="saveButton"
            disabled={saving}
            onClick={() => void saveReplyStyle()}
          >
            <Save size={15} aria-hidden="true" />
            保存风格
          </button>
        </div>
      </div>
    </section>
  );
}
