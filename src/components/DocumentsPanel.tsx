import { useRef, useState, type DragEvent } from "react";
import { FileText, Layers, Trash2, Type, Upload, X } from "lucide-react";
import type { DocumentSummary } from "../services/tauriApi";

function formatCharCount(count: number): string {
  if (count < 1000) return `${count} 字`;
  if (count < 10000) return `${(count / 1000).toFixed(1)}k 字`;
  return `${Math.round(count / 1000)}k 字`;
}

type DocumentsPanelProps = {
  documents: DocumentSummary[];
  onUpload: (file: File) => void;
  onRemove: (name: string) => void;
  onClose: () => void;
  closeTitle?: string;
  className?: string;
};

export function DocumentsPanel({
  documents,
  onUpload,
  onRemove,
  onClose,
  closeTitle = "关闭文档面板",
  className = "modalPanel documentsPanel",
}: DocumentsPanelProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [dragOver, setDragOver] = useState(false);

  const totalChunks = documents.reduce((sum, doc) => sum + doc.chunkCount, 0);
  const totalChars = documents.reduce((sum, doc) => sum + doc.charCount, 0);

  const handleFileInput = (files: FileList | null) => {
    const file = files?.[0];
    if (file) onUpload(file);
  };

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragOver(false);
    const file = event.dataTransfer.files[0];
    if (file?.name.toLowerCase().endsWith(".md")) {
      onUpload(file);
    }
  };

  return (
    <section
      aria-labelledby="documents-title"
      className={className}
      role="dialog"
    >
      <div className="modalHeader">
        <div>
          <h2 id="documents-title">文档知识库</h2>
          <div className="configStatus">
            {documents.length === 0
              ? "上传 Markdown 文档，为 AI 回复提供参考上下文"
              : `已加载 ${documents.length} 篇文档 · ${totalChunks} 块 · ${formatCharCount(totalChars)}`}
          </div>
        </div>
        <button type="button" onClick={onClose} title={closeTitle}>
          <X size={16} />
        </button>
      </div>

      <div className="documentsPanelBody">
        {documents.length > 0 ? (
          <div className="documentsStats" aria-label="知识库统计">
            <div className="documentsStat">
              <FileText size={14} aria-hidden="true" />
              <span>{documents.length} 篇</span>
            </div>
            <div className="documentsStat">
              <Layers size={14} aria-hidden="true" />
              <span>{totalChunks} 块</span>
            </div>
            <div className="documentsStat">
              <Type size={14} aria-hidden="true" />
              <span>{formatCharCount(totalChars)}</span>
            </div>
          </div>
        ) : null}

        <div
          className={dragOver ? "documentUploadZone dragOver" : "documentUploadZone"}
          onClick={() => fileInputRef.current?.click()}
          onDragEnter={(event) => {
            event.preventDefault();
            setDragOver(true);
          }}
          onDragLeave={(event) => {
            if (event.currentTarget.contains(event.relatedTarget as Node)) return;
            setDragOver(false);
          }}
          onDragOver={(event) => event.preventDefault()}
          onDrop={handleDrop}
          role="button"
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              fileInputRef.current?.click();
            }
          }}
        >
          <input
            ref={fileInputRef}
            type="file"
            accept=".md"
            hidden
            onChange={(event) => {
              handleFileInput(event.target.files);
              event.target.value = "";
            }}
          />
          <div className="documentUploadIcon" aria-hidden="true">
            <Upload size={18} />
          </div>
          <div className="documentUploadCopy">
            <strong>上传 Markdown 文档</strong>
            <span>拖拽 .md 文件到此处，或点击选择</span>
          </div>
        </div>

        <div className="documentList" aria-label="已加载的文档">
          {documents.length === 0 ? (
            <div className="emptyDocuments">
              <div className="emptyDocumentsIcon" aria-hidden="true">
                <FileText size={28} strokeWidth={1.4} />
              </div>
              <strong>知识库为空</strong>
              <p>
                上传 Markdown 文件后，AI 在生成回复时会自动检索相关片段作为参考。
              </p>
            </div>
          ) : (
            <ul className="documentListItems">
              {documents.map((doc) => (
                <li className="documentItemRow" key={doc.name}>
                  <div className="documentItem">
                    <div className="documentIcon" aria-hidden="true">
                      <FileText size={16} />
                    </div>
                    <div className="documentInfo">
                      <span className="documentName" title={doc.name}>
                        {doc.name}
                      </span>
                      <div className="documentMeta">
                        <span>
                          <Layers size={11} aria-hidden="true" />
                          {doc.chunkCount} 块
                        </span>
                        <span>
                          <Type size={11} aria-hidden="true" />
                          {formatCharCount(doc.charCount)}
                        </span>
                      </div>
                    </div>
                  </div>
                  <button
                    className="documentRemove"
                    type="button"
                    aria-label="移除"
                    title={`移除 ${doc.name}`}
                    onClick={() => onRemove(doc.name)}
                  >
                    <Trash2 size={15} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </section>
  );
}
