import { useState, useRef, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { usePromptStore } from "@/stores/promptStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useAutoSave } from "@/hooks/useAutoSave";
import { useToast } from "@/components/ui/toast";
import { getErrorMessage } from "@/lib/utils";
import { PolishPanel } from "@/components/PolishPanel";
import { CategoryTags } from "@/components/CategoryTags";
import { DeleteConfirmModal } from "@/components/DeleteConfirmModal";

export function PromptEditor() {
  const currentId = usePromptStore((s) => s.currentId);
  const title = usePromptStore((s) => s.title);
  const content = usePromptStore((s) => s.content);
  const isSaving = usePromptStore((s) => s.isSaving);
  const category = usePromptStore((s) => s.category);
  const summary = usePromptStore((s) => s.summary);
  const tags = usePromptStore((s) => s.tags);
  const setTitle = usePromptStore((s) => s.setTitle);
  const setContent = usePromptStore((s) => s.setContent);
  const savePrompt = usePromptStore((s) => s.savePrompt);
  const deletePrompt = usePromptStore((s) => s.deletePrompt);

  const activeConfig = useSettingsStore((s) => s.activeConfig);
  const isLoaded = useSettingsStore((s) => s.isLoaded);
  const { addToast } = useToast();
  const [showTitleInput, setShowTitleInput] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useAutoSave();

  const insertFormatting = useCallback(
    (before: string, after: string, placeholder: string) => {
      const ta = textareaRef.current;
      if (!ta) return;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const selected = content.slice(start, end);
      const text = selected || placeholder;
      const newContent =
        content.slice(0, start) + before + text + after + content.slice(end);
      setContent(newContent);
      requestAnimationFrame(() => {
        ta.focus();
        if (selected) {
          ta.setSelectionRange(
            start + before.length,
            start + before.length + selected.length
          );
        } else {
          ta.setSelectionRange(
            start + before.length,
            start + before.length + placeholder.length
          );
        }
      });
    },
    [content, setContent]
  );

  const handleImageInsert = useCallback(() => {
    insertFormatting(`![`, `](图片URL)`, "图片描述");
  }, [insertFormatting]);

  const handleSave = async () => {
    if (!content.trim()) return;
    try {
      await savePrompt();
      addToast("已保存", "success");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(content);
      addToast("已复制到剪贴板", "success");
    } catch {
      addToast("复制失败，请重试", "error");
    }
  };

  const handleDelete = () => {
    if (!currentId) return;
    setShowDeleteModal(true);
  };

  const handleDeleteConfirm = async () => {
    if (!currentId) return;
    try {
      await deletePrompt(currentId);
      addToast("已删除", "info");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    } finally {
      setShowDeleteModal(false);
    }
  };

  const wordCount = content.trim() ? content.trim().split(/\s+/).length : 0;
  const charCount = content.length;

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="p-5 space-y-4">

        {/* Editor Card */}
        <div className="rounded-[14px] bg-gradient-to-br from-[#111827] to-[#0F1729] border border-white/[0.06] shadow-xl shadow-black/20">
          {/* Header */}
          <div className="flex items-center gap-2 px-5 pt-4 pb-2">
            <div
              className="flex items-center gap-1.5 cursor-pointer"
              onClick={() => setShowTitleInput(!showTitleInput)}
            >
              <span className="text-[15px] font-semibold text-slate-100">
                {title || "新建 Prompt"}
              </span>
              <svg xmlns="http://www.w3.org/2000/svg" width="13" height="13"
                   viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   strokeWidth="2" strokeLinecap="round"
                   className="text-slate-500">
                <path d="M17 3a2.85 2.85 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
              </svg>
            </div>
            <div className="flex-1" />
            <span className="text-[11px] text-slate-600">
              自动保存
            </span>
          </div>

          {showTitleInput && (
            <div className="px-5 pb-2">
              <input
                type="text"
                value={title}
                onChange={(e) => setTitle(e.target.value.slice(0, 50))}
                placeholder="输入标题"
                className="w-full bg-white/[0.04] border border-white/[0.06] rounded-md px-3 py-1.5 text-[13px] text-slate-200 outline-none focus:border-purple-500/50 transition-colors"
                autoFocus
              />
            </div>
          )}

          {(category || summary) && (
            <div className="px-5 pb-1">
              <CategoryTags category={category} tags={tags} />
              {summary && (
                <span className="text-[11px] text-slate-500 ml-2">{summary}</span>
              )}
            </div>
          )}

          {/* Textarea */}
          <div className="px-5 py-3">
            <Textarea
              ref={textareaRef}
              value={content}
              onChange={(e) => setContent(e.target.value)}
              placeholder="输入你的 Prompt...&#10;&#10;支持 Markdown 格式"
              className="min-h-[220px] resize-none border-0 shadow-none focus-visible:ring-0 text-[14px] leading-relaxed p-0 bg-transparent placeholder:text-slate-700 text-slate-200"
            />
          </div>

          {/* Format toolbar */}
          <div className="flex items-center gap-0.5 px-5 py-2 border-t border-white/[0.05]">
            <button
              onClick={() => insertFormatting("**", "**", "粗体文字")}
              className="w-7 h-7 flex items-center justify-center rounded-md text-slate-600 hover:text-slate-300 hover:bg-white/[0.04] transition-colors"
              title="粗体"
            >
              <FormatIcon name="bold" />
            </button>
            <button
              onClick={() => insertFormatting("*", "*", "斜体文字")}
              className="w-7 h-7 flex items-center justify-center rounded-md text-slate-600 hover:text-slate-300 hover:bg-white/[0.04] transition-colors"
              title="斜体"
            >
              <FormatIcon name="italic" />
            </button>
            <button
              onClick={() => insertFormatting("~~", "~~", "删除线文字")}
              className="w-7 h-7 flex items-center justify-center rounded-md text-slate-600 hover:text-slate-300 hover:bg-white/[0.04] transition-colors"
              title="删除线"
            >
              <FormatIcon name="strikethrough" />
            </button>
            <button
              onClick={() => insertFormatting("`", "`", "代码")}
              className="w-7 h-7 flex items-center justify-center rounded-md text-slate-600 hover:text-slate-300 hover:bg-white/[0.04] transition-colors"
              title="行内代码"
            >
              <FormatIcon name="code" />
            </button>
            <button
              onClick={() => insertFormatting("\n- ", "", "列表项")}
              className="w-7 h-7 flex items-center justify-center rounded-md text-slate-600 hover:text-slate-300 hover:bg-white/[0.04] transition-colors"
              title="无序列表"
            >
              <FormatIcon name="list" />
            </button>
            <button
              onClick={() => insertFormatting("[", "](url)", "链接文字")}
              className="w-7 h-7 flex items-center justify-center rounded-md text-slate-600 hover:text-slate-300 hover:bg-white/[0.04] transition-colors"
              title="链接"
            >
              <FormatIcon name="link" />
            </button>
            <button
              onClick={handleImageInsert}
              className="w-7 h-7 flex items-center justify-center rounded-md text-slate-600 hover:text-slate-300 hover:bg-white/[0.04] transition-colors"
              title="插入图片链接"
            >
              <FormatIcon name="image" />
            </button>
            <div className="flex-1" />
            <span className="text-[11px] text-slate-600 mr-2 tabular-nums">
              {charCount} 字 / {wordCount} 词
            </span>
          </div>

          {/* Polish section */}
          {isLoaded && activeConfig && (
            <>
              <div className="border-t border-white/[0.05]" />
              <div className="px-5 py-4">
                <div className="flex items-center gap-2 mb-3">
                  <span className="text-[14px] font-semibold text-slate-100">
                    AI 润色
                  </span>
                  <div className="flex-1" />
                  <span className="text-[11px] text-slate-500">
                    使用模型：{activeConfig.provider} / {activeConfig.model}
                  </span>
                  <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11"
                       viewBox="0 0 24 24" fill="none" stroke="currentColor"
                       strokeWidth="2" className="text-slate-500">
                    <polyline points="6 9 12 15 18 9" />
                  </svg>
                </div>
                <PolishPanel />
              </div>
            </>
          )}

          {/* Action bar */}
          <div className="flex items-center gap-1.5 px-5 py-2 border-t border-white/[0.05] bg-white/[0.02] rounded-b-[14px]">
            <Button
              onClick={handleSave}
              disabled={isSaving || !content.trim()}
              size="xs"
              className="h-7 text-[12px]"
            >
              {isSaving ? "保存中..." : "保存"}
            </Button>
            <Button
              onClick={handleCopy}
              disabled={!content.trim()}
              size="xs"
              variant="outline"
              className="h-7 text-[12px]"
            >
              复制
            </Button>
            <div className="flex-1" />
            {currentId && (
              <Button
                onClick={handleDelete}
                size="xs"
                variant="ghost"
                className="h-7 text-[12px] text-red-400 hover:text-red-300"
              >
                删除
              </Button>
            )}
          </div>
        </div>

      </div>

      <DeleteConfirmModal
        open={showDeleteModal}
        title={title || "未命名 Prompt"}
        onConfirm={handleDeleteConfirm}
        onCancel={() => setShowDeleteModal(false)}
      />
    </div>
  );
}

function FormatIcon({ name }: { name: string }) {
  switch (name) {
    case "bold":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" />
          <path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" />
        </svg>
      );
    case "italic":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <line x1="19" y1="4" x2="10" y2="4" />
          <line x1="14" y1="20" x2="5" y2="20" />
          <line x1="15" y1="4" x2="9" y2="20" />
        </svg>
      );
    case "strikethrough":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M17.3 4.9c-2.3-.6-4.4-1-6.2-.9-2.7 0-5.3.7-5.3 3.6 0 1.9 1.3 2.5 3.4 3.2" />
          <path d="M6.7 19.1c2.3.6 4.4 1 6.2.9 2.7 0 5.3-.7 5.3-3.6 0-1.4-.8-2.1-2.1-2.7" />
          <line x1="4" y1="12" x2="20" y2="12" />
        </svg>
      );
    case "code":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="16 18 22 12 16 6" />
          <polyline points="8 6 2 12 8 18" />
        </svg>
      );
    case "list":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <line x1="8" y1="6" x2="21" y2="6" />
          <line x1="8" y1="12" x2="21" y2="12" />
          <line x1="8" y1="18" x2="21" y2="18" />
          <line x1="3" y1="6" x2="3.01" y2="6" />
          <line x1="3" y1="12" x2="3.01" y2="12" />
          <line x1="3" y1="18" x2="3.01" y2="18" />
        </svg>
      );
    case "link":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
          <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
        </svg>
      );
    case "image":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
          <circle cx="8.5" cy="8.5" r="1.5" />
          <polyline points="21 15 16 10 5 21" />
        </svg>
      );
    default:
      return null;
  }
}
