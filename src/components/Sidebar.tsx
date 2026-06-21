import { useState, useEffect } from "react";
import { usePromptStore } from "@/stores/promptStore";
import { useThemeStore } from "@/stores/themeStore";
import { CATEGORIES } from "@/components/CategoryTags";
import { CategoryIcon } from "@/lib/categoryIcons";
import { cn } from "@/lib/utils";
import type { MainView, ListMode } from "@/App";
import promptdockLogo from "@/assets/brand/promptdock-app-icon-256.png";

const STORAGE_KEY = "promptdock_custom_categories";

function loadCustomCategories(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed) && parsed.every((item): item is string => typeof item === "string")) {
      return parsed;
    }
    return [];
  } catch {
    return [];
  }
}

function saveCustomCategories(cats: string[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(cats));
}

interface SidebarProps {
  mainView: MainView;
  listMode: ListMode;
  onNavigate: (target: MainView, mode?: ListMode) => void;
  onNewPrompt: () => void;
}

interface NavItem {
  id: string;
  icon: React.ReactNode;
  label: string;
}

const NAV_ITEMS: NavItem[] = [
  {
    id: "editor",
    icon: (
      <path d="M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z" />
    ),
    label: "工作台",
  },
  {
    id: "explore",
    icon: (
      <>
        <circle cx="12" cy="12" r="10" />
        <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
        <line x1="2" y1="12" x2="22" y2="12" />
      </>
    ),
    label: "探索",
  },
  {
    id: "favorites",
    icon: (
      <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
    ),
    label: "收藏",
  },
  {
    id: "recent",
    icon: (
      <>
        <circle cx="12" cy="12" r="10" />
        <polyline points="12 6 12 12 16 14" />
      </>
    ),
    label: "最近使用",
  },
  {
    id: "all",
    icon: (
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    ),
    label: "全部 Prompt",
  },
];

function navItemActive(id: string, mainView: MainView, listMode: ListMode): boolean {
  switch (id) {
    case "editor":
      return mainView === "editor";
    case "explore":
      return mainView === "explore";
    case "favorites":
      return mainView === "list" && listMode === "favorites";
    case "recent":
      return mainView === "list" && listMode === "recent";
    case "all":
      return (
        mainView === "list" &&
        (listMode === "all" || listMode === "category" || listMode === "search")
      ) || mainView === "detail";
    default:
      return false;
  }
}

function navItemTarget(id: string): { target: MainView; mode?: ListMode } {
  switch (id) {
    case "editor":
      return { target: "editor" };
    case "explore":
      return { target: "explore" };
    case "favorites":
      return { target: "list", mode: "favorites" };
    case "recent":
      return { target: "list", mode: "recent" };
    case "all":
      return { target: "list", mode: "all" };
    default:
      return { target: "editor" };
  }
}

export function Sidebar({ mainView, listMode, onNavigate, onNewPrompt }: SidebarProps) {
  const prompts = usePromptStore((s) => s.prompts);
  const selectedCategory = usePromptStore((s) => s.selectedCategory);
  const theme = useThemeStore((s) => s.theme);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);
  const [customCategories, setCustomCategories] = useState<string[]>(loadCustomCategories);
  const [showAddInput, setShowAddInput] = useState(false);
  const [newCatName, setNewCatName] = useState("");

  useEffect(() => {
    saveCustomCategories(customCategories);
  }, [customCategories]);

  const allCategories = [...CATEGORIES, ...customCategories.filter((c) => !(CATEGORIES as readonly string[]).includes(c))];

  const categoryCounts: Record<string, number> = {};
  for (const p of prompts) {
    if (p.category) {
      categoryCounts[p.category] = (categoryCounts[p.category] || 0) + 1;
    }
  }

  const handleNav = (id: string) => {
    const { target, mode } = navItemTarget(id);
    onNavigate(target, mode);
  };

  const handleCategory = (cat: string) => {
    const store = usePromptStore.getState();
    if (selectedCategory === cat) {
      store.setSelectedCategory(null);
    } else {
      store.setSelectedCategory(cat);
    }
    store.setFavoriteOnly(false);
    store.loadPrompts();
    onNavigate("list", "category");
  };

  const handleAddCategory = () => {
    const name = newCatName.trim();
    if (!name) return;
    if ((CATEGORIES as readonly string[]).includes(name) || customCategories.includes(name)) {
      setNewCatName("");
      setShowAddInput(false);
      return;
    }
    setCustomCategories((prev) => [...prev, name]);
    setNewCatName("");
    setShowAddInput(false);
  };

  const handleRemoveCategory = (cat: string) => {
    setCustomCategories((prev) => prev.filter((c) => c !== cat));
    if (selectedCategory === cat) {
      const store = usePromptStore.getState();
      store.setSelectedCategory(null);
      store.loadPrompts();
    }
  };

  return (
    <aside className="w-[270px] border-r border-[var(--sidebar-border)] bg-[var(--sidebar)] flex flex-col shrink-0 text-[var(--sidebar-foreground)]">
      {/* Logo */}
      <div className="flex items-center gap-2.5 px-4 h-12 shrink-0">
        <img
          src={promptdockLogo}
          alt="PromptDock"
          className="w-7 h-7 rounded-lg object-cover shrink-0 shadow-lg shadow-purple-500/20"
        />
        <span className="text-lg font-bold text-[var(--sidebar-foreground)] tracking-tight">PromptDock</span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={toggleTheme}
          aria-label={theme === "dark" ? "切换到暖纸主题" : "切换到深色主题"}
          title={theme === "dark" ? "切换到暖纸主题" : "切换到深色主题"}
          className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 transition-colors hover:bg-white/[0.05] hover:text-slate-200"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            {theme === "dark" ? (
              <>
                <path d="M7 3h8l4 4v14H7z" />
                <path d="M15 3v5h5" />
                <path d="M10 12h6" />
                <path d="M10 16h5" />
              </>
            ) : (
              <path d="M21 12.8A8.6 8.6 0 1 1 11.2 3a6.8 6.8 0 0 0 9.8 9.8z" />
            )}
          </svg>
        </button>
      </div>

      {/* New Prompt button */}
      <div className="px-4 pb-3">
        <button
          onClick={onNewPrompt}
          className="flex items-center justify-center gap-2 w-full h-11 rounded-[10px] bg-gradient-to-br from-purple-600 to-blue-500 text-white text-[13px] font-semibold shadow-lg shadow-purple-500/25 hover:shadow-xl hover:shadow-purple-500/30 transition-shadow active:scale-[0.98]"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.5"
            strokeLinecap="round"
          >
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
          新建 Prompt
        </button>
      </div>

      {/* Navigation */}
      <nav className="px-2 space-y-0.5">
        {NAV_ITEMS.map((item) => {
          const active = navItemActive(item.id, mainView, listMode);
          return (
            <button
              key={item.id}
              onClick={() => handleNav(item.id)}
              className={cn(
                "flex items-center gap-2.5 w-full px-3 py-2 rounded-lg text-[13px] font-medium  text-left",
                active
                  ? "bg-purple-500/10 text-purple-300"
                  : "text-slate-400 hover:bg-white/[0.04] hover:text-slate-200"
              )}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="17"
                height="17"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="shrink-0"
              >
                {item.icon}
              </svg>
              <span>{item.label}</span>
            </button>
          );
        })}
      </nav>

      {/* Categories */}
      <div className="mt-4 px-3">
        <div className="flex items-center justify-between mb-1.5">
          <span className="text-[11px] font-semibold text-slate-500 uppercase tracking-wider">
            分类
          </span>
          <button
            onClick={() => { setShowAddInput(!showAddInput); setNewCatName(""); }}
            aria-label="添加分类"
            title="添加分类"
            className="text-slate-500 hover:text-slate-300 "
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            >
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
          </button>
        </div>

        {showAddInput && (
          <div className="flex items-center gap-1.5 mb-1.5">
            <input
              type="text"
              value={newCatName}
              onChange={(e) => setNewCatName(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleAddCategory(); if (e.key === "Escape") { setShowAddInput(false); setNewCatName(""); }}}
              placeholder="新分类名称"
              className="flex-1 bg-white/[0.04] border border-white/[0.08] rounded-md px-2 py-1 text-[12px] text-slate-200 placeholder:text-slate-600 outline-none focus:border-purple-500/50"
              autoFocus
            />
            <button
              onClick={handleAddCategory}
              disabled={!newCatName.trim()}
              aria-label="确认添加分类"
              className="text-[11px] text-purple-400 hover:text-purple-300 disabled:text-slate-600 px-1 shrink-0"
            >
              添加
            </button>
          </div>
        )}

        <div className="space-y-0.5">
          {allCategories.map((cat) => {
            const active =
              mainView === "list" && listMode === "category" && selectedCategory === cat;
            const isCustom = customCategories.includes(cat);
            return (
              <div key={cat} className="flex items-center group/cat">
                <button
                  onClick={() => handleCategory(cat)}
                  data-category-name={cat}
                  className={cn(
                    "flex items-center gap-2.5 flex-1 px-2.5 py-1.5 rounded-md text-[12px]  text-left min-w-0",
                    active
                      ? "bg-purple-500/10 text-purple-300"
                      : "text-slate-400 hover:bg-white/[0.04] hover:text-slate-200"
                  )}
                >
                  <CategoryIcon category={cat} className="shrink-0" />
                  <span className="flex-1 truncate">{cat}</span>
                  {categoryCounts[cat] > 0 && (
                    <span className="text-[11px] text-slate-500 bg-white/[0.05] px-1.5 py-0.5 rounded-full tabular-nums min-w-[20px] text-center">
                      {categoryCounts[cat]}
                    </span>
                  )}
                </button>
                {isCustom && (
                  <button
                    onClick={() => handleRemoveCategory(cat)}
                    className="w-5 h-5 rounded flex items-center justify-center text-slate-700 hover:text-red-400 opacity-0 group-hover/cat:opacity-100 transition-all shrink-0 ml-0.5"
                    title="删除此分类"
                  >
                    <svg
                      xmlns="http://www.w3.org/2000/svg"
                      width="10"
                      height="10"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2.5"
                      strokeLinecap="round"
                    >
                      <line x1="18" y1="6" x2="6" y2="18" />
                      <line x1="6" y1="6" x2="18" y2="18" />
                    </svg>
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Settings link at bottom */}
      <div className="border-t border-white/[0.05] px-4 py-3">
        <button
          onClick={() => onNavigate("settings")}
          className={cn(
            "flex items-center gap-2.5 w-full px-3 py-2 rounded-lg text-[13px] font-medium  text-left",
            mainView === "settings"
              ? "bg-purple-500/10 text-purple-300"
              : "text-slate-400 hover:bg-white/[0.04] hover:text-slate-200"
          )}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="17"
            height="17"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="shrink-0"
          >
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
          <span>设置</span>
        </button>
      </div>
    </aside>
  );
}
