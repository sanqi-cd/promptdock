import { useState, useEffect, useCallback } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ToastProvider } from "@/components/ui/toast";
import { useSettingsStore } from "@/stores/settingsStore";
import { usePromptStore } from "@/stores/promptStore";
import { useThemeStore } from "@/stores/themeStore";
import { Sidebar } from "@/components/Sidebar";
import { PromptEditor } from "@/components/PromptEditor";
import { RightPanel } from "@/components/RightPanel";
import { SettingsPage } from "@/components/SettingsPage";
import { ExploreView } from "@/components/ExploreView";
import { PromptListView } from "@/components/PromptListView";
import { PromptDetailView } from "@/components/PromptDetailView";
import { GlobalSearchModal } from "@/components/GlobalSearchModal";
import { ErrorBoundary } from "@/components/ErrorBoundary";

export type MainView = "editor" | "explore" | "list" | "detail" | "settings";
export type ListMode = "all" | "favorites" | "category" | "recent" | "search";

function App() {
  const [mainView, setMainView] = useState<MainView>("editor");
  const [listMode, setListMode] = useState<ListMode>("all");
  const [detailId, setDetailId] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);

  const loadConfigs = useSettingsStore((s) => s.loadConfigs);
  const loadPrompts = usePromptStore((s) => s.loadPrompts);
  const theme = useThemeStore((s) => s.theme);

  useEffect(() => {
    loadConfigs();
    // Load prompts up front: the sidebar counts, the explore view and the right
    // panel all derive from `prompts`, and only the list view used to fetch them
    // — so a cold start showed "no prompts yet" on data that existed.
    loadPrompts();
  }, [loadConfigs, loadPrompts]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const handleNavigate = useCallback(
    (target: MainView, mode?: ListMode) => {
      setMainView(target);
      if (mode) setListMode(mode);

      const store = usePromptStore.getState();
      if (target === "list") {
        // 切换列表模式时丢弃上一轮搜索，否则搜索结果会劫持收藏 / 最近使用 / 分类
        // 视图 —— 标题变了，内容却还是搜索结果。
        store.clearSearch();
        if (mode === "favorites") {
          store.setFavoriteOnly(true);
          store.setSelectedCategory(null);
        } else if (mode === "recent") {
          store.setFavoriteOnly(false);
          store.setSelectedCategory(null);
        } else if (mode === "all") {
          store.setFavoriteOnly(false);
          store.setSelectedCategory(null);
        } else if (mode === "category") {
          store.setFavoriteOnly(false);
        }
        store.loadPrompts();
      }
    },
    []
  );

  const handleNewPrompt = useCallback(() => {
    usePromptStore.getState().newPrompt();
    setMainView("editor");
  }, []);

  const handleOpenDetail = useCallback((id: string) => {
    setDetailId(id);
    setMainView("detail");
  }, []);

  const handleSelectPrompt = useCallback((id: string) => {
    usePromptStore.getState().loadPrompt(id);
    setMainView("editor");
    setSearchOpen(false);
  }, []);

  const handleSelectCategory = useCallback((cat: string) => {
    // 这条路径不经过 handleNavigate，同样要清掉上一轮搜索。
    const store = usePromptStore.getState();
    store.clearSearch();
    store.setSelectedCategory(cat);
    setListMode("category");
    setMainView("list");
  }, []);

  const handleDetailEdit = useCallback(() => {
    if (detailId) {
      usePromptStore.getState().loadPrompt(detailId);
      setMainView("editor");
    }
  }, [detailId]);

  const handleDetailBack = useCallback(() => {
    setMainView("list");
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const isModified = event.metaKey || event.ctrlKey;
      if (!isModified) return;

      const key = event.key.toLowerCase();
      if (key === "k") {
        event.preventDefault();
        setSearchOpen(true);
        return;
      }

      const store = usePromptStore.getState();
      if (key === "s") {
        event.preventDefault();
        if (store.content.trim()) {
          store.savePrompt();
        }
        return;
      }

      if (key === "d" && store.currentId) {
        event.preventDefault();
        store.toggleFavorite();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const showRightPanel =
    mainView === "editor" || mainView === "explore" || mainView === "list";

  return (
    <TooltipProvider>
      <ToastProvider>
        <ErrorBoundary>
        <div className="flex h-screen bg-[var(--background)]">
          <Sidebar
            mainView={mainView}
            listMode={listMode}
            onNavigate={handleNavigate}
            onNewPrompt={handleNewPrompt}
          />
          <main className="flex-1 flex flex-col overflow-hidden min-w-0">
            {mainView === "editor" && <PromptEditor />}
            {mainView === "explore" && (
              <ExploreView
                onSelectPrompt={handleSelectPrompt}
                onSelectCategory={handleSelectCategory}
              />
            )}
            {mainView === "list" && (
              <PromptListView
                mode={listMode}
                onSelectPrompt={handleSelectPrompt}
                onOpenDetail={handleOpenDetail}
              />
            )}
            {mainView === "detail" && detailId && (
              <PromptDetailView
                promptId={detailId}
                onBack={handleDetailBack}
                onEdit={handleDetailEdit}
              />
            )}
            {mainView === "settings" && <SettingsPage />}
          </main>
          {showRightPanel && (
            <RightPanel context={mainView === "editor" ? "editor" : "list"} />
          )}
          <GlobalSearchModal
            open={searchOpen}
            onClose={() => setSearchOpen(false)}
            onSelect={handleSelectPrompt}
          />
        </div>
        </ErrorBoundary>
      </ToastProvider>
    </TooltipProvider>
  );
}

export default App;
