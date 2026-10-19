import { useEffect, useState } from "react";
import { usePromptStore } from "@/stores/promptStore";
import { useToast } from "@/components/ui/toast";
import { getErrorMessage } from "@/lib/utils";
import { DeleteConfirmModal } from "@/components/DeleteConfirmModal";

interface PromptDetailViewProps {
  promptId: string;
  onBack: () => void;
  onEdit: () => void;
}

export function PromptDetailView({ promptId, onBack, onEdit }: PromptDetailViewProps) {
  const currentId = usePromptStore((s) => s.currentId);
  const title = usePromptStore((s) => s.title);
  const content = usePromptStore((s) => s.content);
  const optimizedContent = usePromptStore((s) => s.optimizedContent);
  const category = usePromptStore((s) => s.category);
  const summary = usePromptStore((s) => s.summary);
  const tags = usePromptStore((s) => s.tags);
  const isFavorite = usePromptStore((s) => s.isFavorite);
  const loadError = usePromptStore((s) => s.error);
  const loadPrompt = usePromptStore((s) => s.loadPrompt);
  const toggleFavorite = usePromptStore((s) => s.toggleFavorite);
  const deletePrompt = usePromptStore((s) => s.deletePrompt);
  const newPrompt = usePromptStore((s) => s.newPrompt);
  const { addToast } = useToast();
  const [showDeleteModal, setShowDeleteModal] = useState(false);

  useEffect(() => {
    loadPrompt(promptId);
  }, [promptId, loadPrompt]);

  const handleFavorite = async () => {
    try {
      await toggleFavorite();
      addToast(isFavorite ? "已取消收藏" : "已收藏", "success");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    }
  };

  const handleDelete = () => {
    setShowDeleteModal(true);
  };

  const handleDeleteConfirm = async () => {
    try {
      await deletePrompt(promptId);
      addToast("已删除", "info");
      newPrompt();
      onBack();
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    } finally {
      setShowDeleteModal(false);
    }
  };

  const handleCopy = async () => {
    try {
      const text = optimizedContent || content;
      await navigator.clipboard.writeText(text);
      addToast("已复制到剪贴板", "success");
    } catch {
      addToast("复制失败，请重试", "error");
    }
  };

  if (currentId !== promptId) {
    // 读取失败时 currentId 永远不会变成目标 id。继续转圈会让用户以为应用卡死，
    // 给出原因和「返回列表」比无限 spinner 有用。
    if (loadError) {
      return (
        <div className="flex-1 flex flex-col items-center justify-center gap-3 px-6">
          <p className="text-[13px] text-slate-300">加载 Prompt 失败</p>
          <p className="text-[12px] text-red-300/90 max-w-md text-center break-words">
            {loadError}
          </p>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => loadPrompt(promptId)}
              className="h-8 px-4 rounded-lg text-[12px] text-slate-200 bg-white/[0.06] border border-white/[0.08] hover:bg-white/[0.08]"
            >
              重试
            </button>
            <button
              type="button"
              onClick={onBack}
              className="h-8 px-4 rounded-lg text-[12px] text-slate-400 hover:text-slate-200"
            >
              返回列表
            </button>
          </div>
        </div>
      );
    }

    return (
      <div className="flex-1 flex items-center justify-center">
        <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"
             viewBox="0 0 24 24" fill="none" stroke="currentColor"
             strokeWidth="2" className="text-slate-600 animate-spin">
          <path d="M21 12a9 9 0 1 1-6.219-8.56" />
        </svg>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="p-6 space-y-4">
        {/* Breadcrumb + Title */}
        <div className="flex items-center gap-3">
          <button
            onClick={onBack}
            className="w-8 h-8 rounded-lg flex items-center justify-center hover:bg-white/[0.04] text-slate-400 hover:text-slate-200 transition-colors shrink-0"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18"
                 viewBox="0 0 24 24" fill="none" stroke="currentColor"
                 strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </button>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 text-[12px] text-slate-500 mb-0.5">
              <span>全部 Prompt</span>
              <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"
                   viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   strokeWidth="2" className="text-slate-600">
                <polyline points="9 18 15 12 9 6" />
              </svg>
              <span className="text-slate-400 truncate">{title || "未命名"}</span>
            </div>
            <h1 className="text-[17px] font-semibold text-slate-100">{title || "未命名"}</h1>
          </div>
          <div className="flex items-center gap-1.5">
            <button
              onClick={onEdit}
              className="h-8 px-3 rounded-lg text-[12px] font-medium bg-white/[0.04] border border-white/[0.08] text-slate-300 hover:bg-white/[0.06] transition-colors"
            >
              编辑
            </button>
            <button
              onClick={handleDelete}
              className="h-8 w-8 rounded-lg flex items-center justify-center text-red-400 hover:bg-red-500/10 transition-colors"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15"
                   viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              </svg>
            </button>
          </div>
        </div>

        {/* Meta info */}
        <div className="flex items-center gap-2 flex-wrap">
          {category && (
            <span className="text-[11px] text-purple-400 bg-purple-500/10 px-2 py-0.5 rounded-full">
              {category}
            </span>
          )}
          {tags.map((t) => (
            <span key={t} className="text-[11px] text-slate-400 bg-white/[0.04] px-2 py-0.5 rounded-full">
              {t}
            </span>
          ))}
          <div className="flex-1" />
          <button
            onClick={handleFavorite}
            className="flex items-center gap-1.5 text-[12px] text-slate-400 hover:text-amber-400 transition-colors"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14"
                 viewBox="0 0 24 24" fill={isFavorite ? "#F59E0B" : "none"}
                 stroke={isFavorite ? "#F59E0B" : "currentColor"}
                 strokeWidth="1.6">
              <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
            </svg>
            {isFavorite ? "已收藏" : "收藏"}
          </button>
        </div>

        {/* Content Card */}
        <div className="rounded-xl border border-white/[0.06] bg-gradient-to-br from-[#111827] to-[#0F1729]">
          <div className="px-5 pt-4 pb-2">
            <span className="text-[13px] font-semibold text-slate-100">Prompt 内容</span>
          </div>
          <div className="px-5 pb-4">
            <pre className="text-[14px] text-slate-200 leading-relaxed whitespace-pre-wrap font-sans">
              {content}
            </pre>
          </div>
          <div className="flex items-center gap-1.5 px-5 py-2 border-t border-white/[0.05] bg-white/[0.02] rounded-b-xl">
            <button
              onClick={handleCopy}
              className="h-7 px-3 rounded-md text-[12px] text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] transition-colors"
            >
              复制
            </button>
          </div>
        </div>

        {/* AI Result Card */}
        {optimizedContent && (
          <div className="rounded-xl border border-white/[0.06] bg-gradient-to-br from-[#111827] to-[#0F1729]">
            <div className="flex items-center gap-2 px-5 pt-4 pb-2">
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14"
                   viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   strokeWidth="2" strokeLinecap="round"
                   className="text-purple-400">
                <path d="M12 2a10 10 0 1 0 10 10H12V2z" />
                <path d="M12 2a10 10 0 0 0-10 10h10V2z" />
              </svg>
              <span className="text-[13px] font-semibold text-slate-100">AI 润色结果</span>
            </div>
            <div className="px-5 pb-4">
              <pre className="text-[14px] text-slate-200 leading-relaxed whitespace-pre-wrap font-sans">
                {optimizedContent}
              </pre>
            </div>
            <div className="flex items-center gap-1.5 px-5 py-2 border-t border-white/[0.05] bg-white/[0.02] rounded-b-xl">
              <button
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(optimizedContent);
                    addToast("已复制优化版本", "success");
                  } catch {
                    addToast("复制失败，请重试", "error");
                  }
                }}
                className="h-7 px-3 rounded-md text-[12px] text-purple-400 hover:text-purple-300 hover:bg-purple-500/10 transition-colors"
              >
                复制优化版
              </button>
            </div>
          </div>
        )}

        {/* Meta Card */}
        {summary && (
          <div className="rounded-xl border border-white/[0.06] bg-white/[0.02]">
            <div className="px-4 pt-3.5 pb-2">
              <span className="text-[13px] font-semibold text-slate-100">元信息</span>
            </div>
            <div className="px-4 pb-3.5 space-y-2">
              <p className="text-[12px] text-slate-400 leading-relaxed">{summary}</p>
            </div>
          </div>
        )}
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
