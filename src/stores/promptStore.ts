import { create } from "zustand";
import { Channel, invoke } from "@tauri-apps/api/core";
import { hasTauriInvoke } from "@/lib/tauriRuntime";
import { createLatestOnlyGuard } from "@/lib/latestOnly";
import type {
  Prompt,
  SearchResult,
  ClassifyResult,
  PolishMode,
  PolishProcessStep,
} from "@/types";

interface PromptState {
  currentId: string | null;
  title: string;
  content: string;
  optimizedContent: string | null;
  category: string | null;
  summary: string | null;
  tags: string[];
  isFavorite: boolean;

  prompts: Prompt[];
  searchResults: SearchResult[];
  searchQuery: string;
  selectedCategory: string | null;
  favoriteOnly: boolean;

  isSaving: boolean;
  isPolishing: boolean;
  isClassifying: boolean;
  isSearching: boolean;
  error: string | null;

  newPrompt: () => void;
  loadPrompt: (id: string) => Promise<void>;
  savePrompt: () => Promise<void>;
  deletePrompt: (id: string) => Promise<void>;
  recordPromptUse: (id: string) => Promise<void>;
  toggleFavorite: () => Promise<void>;
  setTitle: (title: string) => void;
  setContent: (content: string) => void;
  setOptimizedContent: (content: string | null) => void;

  loadPrompts: () => Promise<void>;
  search: (query: string) => Promise<void>;
  setSearchQuery: (query: string) => void;
  clearSearch: () => void;
  setSelectedCategory: (category: string | null) => void;
  setFavoriteOnly: (fav: boolean) => void;

  analyzePolishProcess: (
    content: string,
    mode: PolishMode
  ) => Promise<PolishProcessStep[]>;
  polishPrompt: (mode: PolishMode, sourceContent?: string) => Promise<string>;
  polishPromptStream: (
    mode: PolishMode,
    sourceContent: string,
    onDelta: (delta: string) => void
  ) => Promise<string>;
  autoClassify: () => Promise<ClassifyResult>;
  applyClassifyResult: (result: ClassifyResult) => Promise<void>;
}

interface PolishStreamEvent {
  type: "delta" | "done";
  delta?: string;
  content?: string;
}

// 三类相互独立的异步读取，各自需要「最新者胜」。用同一个守卫会让列表刷新
// 把在途的搜索或详情加载误判为过期。
const beginListLoad = createLatestOnlyGuard();
const beginPromptLoad = createLatestOnlyGuard();
const beginSearch = createLatestOnlyGuard();

/**
 * 「处理要点」的结果缓存。
 *
 * 生成要点是一次独立的 LLM 调用，而它只在润色超过 800ms 时才真正发出、
 * 且一旦发出就无法取消。按 (内容, 模式) 缓存之后，「润色 → 不满意 → 再润色」
 * 这条最常见的路径只会花一次钱。
 */
const polishProcessCache = new Map<string, PolishProcessStep[]>();
const POLISH_PROCESS_CACHE_LIMIT = 50;

function polishProcessCacheKey(content: string, mode: PolishMode): string {
  // 用 JSON 编码而不是拼分隔符：正文里什么字符都可能出现，任何分隔符都有边界歧义。
  return JSON.stringify([mode, content]);
}

function readPolishedProcessCache(key: string): PolishProcessStep[] | undefined {
  const cached = polishProcessCache.get(key);
  if (!cached) return undefined;

  // 命中后挪到队尾，让淘汰按最近使用而不是最早写入。
  polishProcessCache.delete(key);
  polishProcessCache.set(key, cached);
  return cached;
}

function writePolishProcessCache(key: string, steps: PolishProcessStep[]): void {
  polishProcessCache.set(key, steps);
  while (polishProcessCache.size > POLISH_PROCESS_CACHE_LIMIT) {
    const oldest = polishProcessCache.keys().next().value;
    if (oldest === undefined) break;
    polishProcessCache.delete(oldest);
  }
}

export const usePromptStore = create<PromptState>((set, get) => ({
  currentId: null,
  title: "",
  content: "",
  optimizedContent: null,
  category: null,
  summary: null,
  tags: [],
  isFavorite: false,
  prompts: [],
  searchResults: [],
  searchQuery: "",
  selectedCategory: null,
  favoriteOnly: false,
  isSaving: false,
  isPolishing: false,
  isClassifying: false,
  isSearching: false,
  error: null,

  newPrompt: () => {
    set({
      currentId: null,
      title: "",
      content: "",
      optimizedContent: null,
      category: null,
      summary: null,
      tags: [],
      isFavorite: false,
    });
  },

  loadPrompt: async (id: string) => {
    if (!hasTauriInvoke()) return;

    // 快速连续打开两条 Prompt 时，慢的旧响应会把 currentId 写回旧 id，
    // 详情页因此永远等不到匹配的 id 而卡在 spinner。
    const isLatest = beginPromptLoad();

    try {
      const prompt = await invoke<Prompt>("get_prompt", { id });
      const tags = await invoke<{ id: string; name: string }[]>("get_prompt_tags", {
        promptId: id,
      });
      if (!isLatest()) return;

      set({
        currentId: prompt.id,
        title: prompt.title,
        content: prompt.content,
        optimizedContent: prompt.optimized_content,
        category: prompt.category,
        summary: prompt.summary,
        tags: tags.map((t) => t.name),
        isFavorite: prompt.is_favorite,
      });
      await get().recordPromptUse(id);
    } catch (e) {
      if (!isLatest()) return;
      const msg = getErrorMessage(e);
      console.error("Failed to load prompt:", msg);
      set({ error: msg });
    }
  },

  savePrompt: async () => {
    if (!hasTauriInvoke()) {
      set({ error: "PromptDock desktop runtime is unavailable" });
      return;
    }

    const { currentId, title, content, optimizedContent } = get();
    // 没有标题的记录在列表、搜索和详情里一律显示「未命名」，几乎找不回来。
    // 自动保存和「自动分类」都会隐式落库，所以标题在这里统一兜底。
    const effectiveTitle = title.trim() || deriveTitle(content);

    set({ isSaving: true });
    try {
      if (currentId) {
        await invoke("update_prompt", {
          id: currentId,
          title: effectiveTitle,
          content,
          optimizedContent,
        });
        set({ title: effectiveTitle });
      } else {
        const prompt = await invoke<Prompt>("create_prompt", {
          title: effectiveTitle,
          content,
          optimizedContent,
        });
        set({ currentId: prompt.id, title: effectiveTitle });
      }
    } finally {
      set({ isSaving: false });
    }
  },

  deletePrompt: async (id: string) => {
    if (!hasTauriInvoke()) return;

    try {
      await invoke("delete_prompt", { id });
      const { currentId } = get();
      if (currentId === id) {
        get().newPrompt();
      }
    } catch (e) {
      set({ error: getErrorMessage(e) });
      throw e;
    }
  },

  recordPromptUse: async (id: string) => {
    if (!hasTauriInvoke()) return;

    try {
      await invoke("record_prompt_use", { id });
      await get().loadPrompts();
    } catch (e) {
      set({ error: getErrorMessage(e) });
    }
  },

  toggleFavorite: async () => {
    const { currentId } = get();
    if (!currentId) return;
    if (!hasTauriInvoke()) return;

    try {
      const fav = await invoke<boolean>("toggle_favorite", { id: currentId });
      set({ isFavorite: fav });
    } catch (e) {
      set({ error: getErrorMessage(e) });
      throw e;
    }
  },

  setTitle: (title: string) => set({ title }),
  setContent: (content: string) => set({ content }),
  setOptimizedContent: (content: string | null) =>
    set({ optimizedContent: content }),

  loadPrompts: async () => {
    if (!hasTauriInvoke()) {
      set({ prompts: [] });
      return;
    }

    // 一次侧边栏点击可能并发多个 list_prompts，响应乱序时旧的会覆盖新的。
    const isLatest = beginListLoad();

    const { selectedCategory, favoriteOnly } = get();
    try {
      const prompts = await invoke<Prompt[]>("list_prompts", {
        category: selectedCategory,
        favoriteOnly,
      });
      if (!isLatest()) return;
      set({ prompts });
    } catch (e) {
      if (!isLatest()) return;
      const msg = getErrorMessage(e);
      console.error("Failed to load prompts:", msg);
      set({ error: msg });
    }
  },

  search: async (query: string) => {
    const isLatest = beginSearch();
    set({ isSearching: true, searchQuery: query });

    if (!query.trim() || !hasTauriInvoke()) {
      set({ searchResults: [], isSearching: false });
      return;
    }

    try {
      const results = await invoke<SearchResult[]>("search_prompts", { query });
      if (!isLatest()) return;
      set({ searchResults: results });
    } catch (e) {
      if (!isLatest()) return;
      set({ error: getErrorMessage(e), searchResults: [] });
    } finally {
      // 只有最新那次请求才有权关掉 loading，否则后发请求还在飞时图标就消失了。
      if (isLatest()) set({ isSearching: false });
    }
  },

  setSearchQuery: (query: string) => set({ searchQuery: query }),

  clearSearch: () => {
    // 先作废在途的搜索响应，否则清空之后旧结果还会被写回来。
    beginSearch();
    set({ searchQuery: "", searchResults: [], isSearching: false });
  },
  setSelectedCategory: (category: string | null) => {
    set({ selectedCategory: category });
    get().loadPrompts();
  },
  setFavoriteOnly: (fav: boolean) => {
    set({ favoriteOnly: fav });
    get().loadPrompts();
  },

  analyzePolishProcess: async (content: string, mode: PolishMode) => {
    const key = polishProcessCacheKey(content, mode);
    const cached = readPolishedProcessCache(key);
    if (cached) return cached;

    const steps = await invoke<PolishProcessStep[]>("analyze_polish_process", {
      content,
      mode,
    });
    writePolishProcessCache(key, steps);
    return steps;
  },

  polishPrompt: async (mode: PolishMode, sourceContent?: string) => {
    set({ isPolishing: true });
    try {
      const { content } = get();
      return await invoke<string>("polish_prompt", {
        content: sourceContent ?? content,
        mode,
      });
    } finally {
      set({ isPolishing: false });
    }
  },

  polishPromptStream: async (
    mode: PolishMode,
    sourceContent: string,
    onDelta: (delta: string) => void
  ) => {
    set({ isPolishing: true });
    try {
      const channel = new Channel<PolishStreamEvent>((event) => {
        if (event.type === "delta" && event.delta) {
          onDelta(event.delta);
        }
      });

      const result = await invoke<string | null>("polish_prompt_stream", {
        content: sourceContent,
        mode,
        onChunk: channel,
      });

      if (typeof result === "string" && result.length > 0) {
        return result;
      }

      return await invoke<string>("polish_prompt", {
        content: sourceContent,
        mode,
      });
    } catch (e) {
      if (isMissingStreamCommand(getErrorMessage(e))) {
        return await invoke<string>("polish_prompt", {
          content: sourceContent,
          mode,
        });
      }
      throw e;
    } finally {
      set({ isPolishing: false });
    }
  },

  autoClassify: async () => {
    set({ isClassifying: true });
    try {
      const { content } = get();
      return await invoke<ClassifyResult>("auto_classify", { content });
    } finally {
      set({ isClassifying: false });
    }
  },

  applyClassifyResult: async (result: ClassifyResult) => {
    const { currentId } = get();
    if (!currentId) return;
    if (!hasTauriInvoke()) return;

    // 单个命令、单个事务写入。以前拆成 update_prompt_meta + set_prompt_tags
    // 两步，第二步会清空全部标签，把第一步刚写入的推荐工具标签删掉。
    const tags = await invoke<{ id: string; name: string }[]>("apply_classification", {
      id: currentId,
      category: result.category,
      summary: result.summary,
      recommendedTool: result.recommended_tool,
      tagNames: result.tags,
    });
    set({
      category: result.category,
      summary: result.summary,
      tags: tags.map((t) => t.name),
    });
  },
}));

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "An unexpected error occurred";
}

/** 兜底标题的字符数上限。 */
const DERIVED_TITLE_MAX_CHARS = 30;

/**
 * 从正文推导一个标题，用于用户没填标题的情况。
 *
 * 取第一行非空文本并按**码点**截断（`slice` 会切断代理对，把 emoji 变成乱码）。
 */
export function deriveTitle(content: string): string {
  const firstLine = content
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0);

  return Array.from(firstLine ?? "")
    .slice(0, DERIVED_TITLE_MAX_CHARS)
    .join("");
}

/**
 * 判断错误是否表示「运行时的二进制里没有 polish_prompt_stream 这个命令」
 * （前端比后端新）。只有这种情况才回退到非流式接口。
 *
 * 以前用 `message.includes("not found")` 这种宽松匹配，会把模型返回的
 * 「model not found」也当成命令缺失，于是同一次请求被发两遍、计费两次。
 */
function isMissingStreamCommand(message: string): boolean {
  return /command\s+polish_prompt_stream\s+not\s+found/i.test(message);
}
