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
import { DiscardConfirmModal } from "@/components/DiscardConfirmModal";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { ErrorBanner } from "@/components/ErrorBanner";
import { useAutoSave } from "@/hooks/useAutoSave";
import { isOverlayOpen } from "@/lib/overlay";
import { getErrorMessage } from "@/lib/utils";

export type MainView = "editor" | "explore" | "list" | "detail" | "settings";
export type ListMode = "all" | "favorites" | "category" | "recent" | "search";

/**
 * 独立组件承载自动保存：hook 订阅了 title/content，若直接在 App 里调用，
 * 每次按键都会重渲染整个应用树。这里只让它自己重渲染。
 */
function AutoSave() {
  useAutoSave();
  return null;
}

function App() {
  const [mainView, setMainView] = useState<MainView>("editor");
  const [listMode, setListMode] = useState<ListMode>("all");
  const [detailId, setDetailId] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [showDiscardModal, setShowDiscardModal] = useState(false);

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
    const store = usePromptStore.getState();
    // 有未保存的修改时先确认，否则输入不足 3s（自动保存未触发）的内容会直接丢失。
    if (store.isDirty()) {
      setShowDiscardModal(true);
      return;
    }
    store.newPrompt();
    setMainView("editor");
  }, []);

  const handleDiscardConfirm = useCallback(() => {
    setShowDiscardModal(false);
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

      // 弹窗打开时全局快捷键一律不生效：⌘K 会叠在确认框上，
      // ⌘S/⌘D 则可能操作用户看不见的编辑器内容。
      if (isOverlayOpen()) return;

      const key = event.key.toLowerCase();
      if (key === "k") {
        event.preventDefault();
        setSearchOpen(true);
        return;
      }

      // ⌘S / ⌘D 只对编辑器生效：在设置页或搜索弹窗里按 ⌘D，
      // 会收藏一条用户根本看不见的 Prompt。
      if (mainView !== "editor" || searchOpen) return;

      const store = usePromptStore.getState();
      if (key === "s") {
        event.preventDefault();
        if (store.content.trim()) {
          // 失败时 store 会写 saveError，由 ErrorBanner 持久展示；
          // 这里只需吞掉 rejection，避免 unhandled promise。
          void store.savePrompt().catch(() => {});
        }
        return;
      }

      if (key === "d" && store.currentId) {
        event.preventDefault();
        void store.toggleFavorite().catch((e) => {
          // 快捷键路径没有 toast 容器，失败写进错误横幅，不能静默。
          usePromptStore.setState({ error: getErrorMessage(e) });
        });
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [mainView, searchOpen]);

  const showRightPanel =
    mainView === "editor" || mainView === "explore" || mainView === "list";

  return (
    <TooltipProvider>
      <ToastProvider>
        <ErrorBoundary>
        <div className="flex h-screen bg-[var(--background)]">
          <AutoSave />
          <Sidebar
            mainView={mainView}
            listMode={listMode}
            onNavigate={handleNavigate}
            onNewPrompt={handleNewPrompt}
          />
          <main className="flex-1 flex flex-col overflow-hidden min-w-0">
            <ErrorBanner />
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
          <DiscardConfirmModal
            open={showDiscardModal}
            onConfirm={handleDiscardConfirm}
            onCancel={() => setShowDiscardModal(false)}
          />
        </div>
        </ErrorBoundary>
      </ToastProvider>
    </TooltipProvider>
  );
}

export default App;
