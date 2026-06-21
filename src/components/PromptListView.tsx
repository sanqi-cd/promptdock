import { useEffect } from "react";
import { usePromptStore } from "@/stores/promptStore";
import type { ListMode } from "@/App";

interface PromptListViewProps {
  mode: ListMode;
  onSelectPrompt: (id: string) => void;
  onOpenDetail: (id: string) => void;
}

const MODE_LABELS: Record<ListMode, string> = {
  all: "全部 Prompt",
  favorites: "收藏",
  category: "分类筛选",
  recent: "最近使用",
  search: "搜索结果",
};

export function PromptListView({ mode, onSelectPrompt, onOpenDetail }: PromptListViewProps) {
  const prompts = usePromptStore((s) => s.prompts);
  const searchResults = usePromptStore((s) => s.searchResults);
  const searchQuery = usePromptStore((s) => s.searchQuery);
  const selectedCategory = usePromptStore((s) => s.selectedCategory);
  const favoriteOnly = usePromptStore((s) => s.favoriteOnly);
  const isSearching = usePromptStore((s) => s.isSearching);
  const loadPrompts = usePromptStore((s) => s.loadPrompts);
  const search = usePromptStore((s) => s.search);

  useEffect(() => {
    loadPrompts();
  }, [loadPrompts, mode]);

  const displayItems =
    searchQuery.trim()
      ? searchResults.map((r) => ({
          id: r.id,
          title: r.title,
          content: r.summary || "",
          category: r.category,
          is_favorite: false,
          updated_at: r.updated_at,
        }))
      : mode === "recent"
        ? [...prompts].sort((a, b) =>
            (b.last_used_at || b.updated_at).localeCompare(a.last_used_at || a.updated_at)
          )
        : prompts;

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="p-5 space-y-4">
        {/* Header */}
        <div className="flex items-center gap-3">
          <div>
            <h1 className="text-[18px] font-semibold text-slate-100">
              {searchQuery.trim() ? MODE_LABELS.search : MODE_LABELS[mode]}
            </h1>
            {mode === "category" && selectedCategory && (
              <p className="text-[13px] text-purple-400 mt-0.5">
                分类：{selectedCategory}
              </p>
            )}
          </div>
          <div className="flex-1" />
          <span className="text-[12px] text-slate-600">{displayItems.length} 个 Prompt</span>
        </div>

        {/* Search bar */}
        <div className="flex items-center gap-2.5 bg-white/[0.04] border border-white/[0.08] rounded-[10px] px-3.5 h-9">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="text-slate-500 shrink-0"
          >
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.3-4.3" />
          </svg>
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => {
              search(e.target.value);
            }}
            placeholder="搜索 Prompt 标题、内容、标签..."
            className="flex-1 bg-transparent text-[13px] text-slate-300 placeholder:text-slate-600 outline-none"
          />
          {isSearching && (
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              className="text-slate-500 animate-spin"
            >
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
          )}
        </div>

        {/* Active filters */}
        {(favoriteOnly || selectedCategory) && mode !== "search" && (
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-slate-500">筛选：</span>
            {favoriteOnly && (
              <span className="inline-flex items-center gap-1 text-[11px] bg-amber-500/10 text-amber-400 px-2 py-0.5 rounded-full">
                收藏
              </span>
            )}
            {selectedCategory && (
              <span className="inline-flex items-center gap-1 text-[11px] bg-purple-500/10 text-purple-400 px-2 py-0.5 rounded-full">
                {selectedCategory}
              </span>
            )}
          </div>
        )}

        {/* Card list */}
        {displayItems.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 gap-3">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="40"
              height="40"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeLinecap="round"
              className="text-slate-700"
            >
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
            </svg>
            <p className="text-[13px] text-slate-600">
              {searchQuery.trim()
                ? "未找到匹配的 Prompt"
                : "暂无 Prompt"}
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {displayItems.map((item) => (
              <div
                key={item.id}
                className="rounded-xl border border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.04] hover:border-white/[0.1] transition-colors"
              >
                <button
                  onClick={() => onSelectPrompt(item.id)}
                  className="w-full text-left px-4 pt-3.5 pb-3"
                >
                  <div className="flex items-start gap-3">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-[14px] font-medium text-slate-200 truncate">
                          {item.title || "未命名"}
                        </span>
                        {item.is_favorite && (
                          <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="12"
                            height="12"
                            viewBox="0 0 24 24"
                            fill="#F59E0B"
                            stroke="#F59E0B"
                            strokeWidth="1.5"
                            className="shrink-0"
                          >
                            <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                          </svg>
                        )}
                        {item.category && (
                          <span className="text-[11px] text-purple-400 bg-purple-500/10 px-1.5 py-0.5 rounded shrink-0">
                            {item.category}
                          </span>
                        )}
                      </div>
                      <p className="text-[12px] text-slate-500 line-clamp-2 mt-1">
                        {item.content.slice(0, 150)}
                      </p>
                    </div>
                    <span className="text-[11px] text-slate-600 shrink-0 mt-0.5">
                      {new Date(item.updated_at).toLocaleDateString("zh-CN")}
                    </span>
                  </div>
                </button>
                <div className="flex items-center gap-1 px-4 pb-3 border-t border-white/[0.03] pt-2">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onOpenDetail(item.id);
                    }}
                    className="text-[11px] text-slate-500 hover:text-slate-300 px-2 py-1 rounded transition-colors"
                  >
                    详情
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
