import { useState, useEffect, useCallback, useRef } from "react";
import { usePromptStore } from "@/stores/promptStore";
import { cn } from "@/lib/utils";

interface GlobalSearchModalProps {
  open: boolean;
  onClose: () => void;
  onSelect: (id: string) => void;
}

export function GlobalSearchModal({ open, onClose, onSelect }: GlobalSearchModalProps) {
  const searchResults = usePromptStore((s) => s.searchResults);
  const isSearching = usePromptStore((s) => s.isSearching);
  const search = usePromptStore((s) => s.search);
  const clearSearch = usePromptStore((s) => s.clearSearch);
  const [inputValue, setInputValue] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const searchTimerRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    if (!open) {
      setInputValue("");
      // 关闭时不能只清 searchQuery：旧结果和仍在飞的请求都会留下来，
      // 下次打开会看到与输入框不匹配的「幽灵结果」。
      clearSearch();
      setSelectedIndex(0);
    }
  }, [open, clearSearch]);

  useEffect(() => {
    return () => window.clearTimeout(searchTimerRef.current);
  }, []);

  useEffect(() => {
    setSelectedIndex(0);
  }, [searchResults]);

  useEffect(() => {
    if (selectedIndex >= 0 && listRef.current) {
      const items = listRef.current.querySelectorAll('[role="option"]');
      if (items[selectedIndex]) {
        (items[selectedIndex] as HTMLElement).scrollIntoView({ block: "nearest" });
      }
    }
  }, [selectedIndex]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((i) => Math.min(i + 1, searchResults.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          // 输入为空时列表只是占位，不能选中上一轮的残留结果。
          if (inputValue.trim() && searchResults[selectedIndex]) {
            onSelect(searchResults[selectedIndex].id);
            onClose();
          }
          break;
        case "Home":
          e.preventDefault();
          setSelectedIndex(0);
          break;
        case "End":
          e.preventDefault();
          setSelectedIndex(searchResults.length - 1);
          break;
        case "Escape":
          onClose();
          break;
      }
    },
    [searchResults, selectedIndex, inputValue, onSelect, onClose]
  );

  const handleInput = useCallback(
    (value: string) => {
      setInputValue(value);
      window.clearTimeout(searchTimerRef.current);

      if (!value.trim()) {
        // 立即作废在途请求并清空结果，避免清空输入后仍显示旧结果。
        clearSearch();
        return;
      }

      // 每个按键都发起一次查询会在中文 LIKE 全表扫描时放大开销。
      searchTimerRef.current = window.setTimeout(() => search(value), 200);
    },
    [search, clearSearch]
  );

  if (!open) return null;

  return (
    <div
      data-modal-open="true"
      className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]"
    >
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onClose} />

      {/* Dialog */}
      <div className="relative w-full max-w-lg bg-[#111827] border border-white/[0.08] rounded-2xl shadow-2xl shadow-black/40 overflow-hidden">
        {/* Search input */}
        <div className="flex items-center gap-3 px-4 h-12 border-b border-white/[0.06]">
          <svg
            xmlns="http://www.w3.org/2000/svg" width="16" height="16"
            viewBox="0 0 24 24" fill="none" stroke="currentColor"
            strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"
            className="text-slate-500 shrink-0"
          >
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.3-4.3" />
          </svg>
          <input
            type="text"
            value={inputValue}
            onChange={(e) => handleInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="搜索 Prompt..."
            className="flex-1 bg-transparent text-[14px] text-slate-200 placeholder:text-slate-600 outline-none"
            autoFocus
            role="combobox"
            aria-expanded={searchResults.length > 0}
            aria-controls="search-results-list"
          />
          {isSearching && (
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14"
                 viewBox="0 0 24 24" fill="none" stroke="currentColor"
                 strokeWidth="2" className="text-slate-500 animate-spin">
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
          )}
          <kbd className="text-[10px] text-slate-600 bg-white/[0.04] px-1.5 py-0.5 rounded font-mono">
            ESC
          </kbd>
        </div>

        {/* Results */}
        <div ref={listRef} id="search-results-list" role="listbox" className="max-h-[320px] overflow-y-auto">
          {inputValue.trim() && searchResults.length === 0 && !isSearching && (
            <div className="flex flex-col items-center gap-2 py-12">
              <p className="text-[13px] text-slate-500">未找到匹配的 Prompt</p>
            </div>
          )}
          {!inputValue.trim() && (
            <div className="flex flex-col items-center gap-2 py-12">
              <p className="text-[13px] text-slate-600">输入关键词开始搜索</p>
            </div>
          )}
          {inputValue.trim() && searchResults.map((r, i) => (
            <button
              key={r.id}
              role="option"
              aria-selected={i === selectedIndex}
              onClick={() => {
                onSelect(r.id);
                onClose();
              }}
              onMouseEnter={() => setSelectedIndex(i)}
              className={cn(
                "flex items-start gap-3 w-full px-4 py-3 text-left hover:bg-white/[0.04] transition-colors border-b border-white/[0.03] last:border-b-0",
                i === selectedIndex && "bg-white/[0.06]"
              )}
            >
              <div className="flex-1 min-w-0">
                <span className="text-[13px] font-medium text-slate-200 truncate block">
                  {r.title || "未命名"}
                </span>
                {r.summary && (
                  <span className="text-[12px] text-slate-500 line-clamp-1 mt-0.5 block">
                    {r.summary}
                  </span>
                )}
              </div>
              {r.category && (
                <span className="text-[11px] text-purple-400 bg-purple-500/10 px-1.5 py-0.5 rounded shrink-0 mt-0.5">
                  {r.category}
                </span>
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
