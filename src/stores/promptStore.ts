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

  /** 最近一次成功落库（或加载）的标题/正文快照，用于「新建前是否丢内容」的脏检查。 */
  savedSnapshot: { title: string; content: string };

  /**
   * 每次切换编辑目标（新建 / 加载完成）时递增。
   *
   * 保存队列用它判断「这次保存的结果是否还属于当前编辑对象」：A 的保存请求
   * 在用户切到 B 之后才返回时，不能把 A 的标题和快照写回 B 的状态。
   * 润色面板也用它判断是否该清空上一条 Prompt 的结果。
   */
  editSession: number;

  prompts: Prompt[];
  searchResults: SearchResult[];
  searchQuery: string;
  selectedCategory: string | null;
  favoriteOnly: boolean;

  isSaving: boolean;
  isPolishing: boolean;
  /** 在途流式润色的请求 id，`cancelPolishStream` 按它通知后端。 */
  polishRequestId: string | null;
  isClassifying: boolean;
  isSearching: boolean;
  /**
   * 后台读取失败的提示。只在**用户没有其他反馈渠道**的操作上设置：
   * 列表 / 详情 / 搜索失败时界面会显示空白或陈旧内容，必须让用户知道原因。
   * 删除、收藏这类用户主动触发、且调用方已经有 toast 的操作**不要**写这里，
   * 否则 toast 消失后横幅还挂在那里。
   */
  error: string | null;
  /** 保存失败（含自动保存重试期间）的持久提示，成功后自动清除。 */
  saveError: string | null;

  newPrompt: () => void;
  isDirty: () => boolean;
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
  clearError: () => void;
  clearSaveError: () => void;
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
  cancelPolishStream: () => Promise<void>;
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
const beginPolishStream = createLatestOnlyGuard();

/**
 * 保存串行化队列。自动保存（3s 防抖）和 ⌘S 可能同时发起：两个请求都读到
 * `currentId === null` 就会各建一条重复记录。排队执行后，后一个保存读到的是
 * 前一个写回的 currentId，自然走 update 而不是重复 create。
 */
let saveQueue: Promise<void> = Promise.resolve();

/**
 * 保存请求序号。`isSaving` 只允许最新一次保存在结束时复位，否则慢的旧保存
 * 会在新保存进行中把 loading 关掉。
 */
let saveRequestSeq = 0;

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
  polishRequestId: null,
  isClassifying: false,
  isSearching: false,
  error: null,
  saveError: null,
  savedSnapshot: { title: "", content: "" },
  editSession: 0,

  newPrompt: () => {
    // 先作废在途的 loadPrompt，否则它的迟到响应会在清空之后把编辑器写回旧 Prompt。
    beginPromptLoad();
    // 编辑目标变了，在途的流式润色一并取消。
    void get().cancelPolishStream();
    set({
      currentId: null,
      title: "",
      content: "",
      optimizedContent: null,
      category: null,
      summary: null,
      tags: [],
      isFavorite: false,
      savedSnapshot: { title: "", content: "" },
      // 用户已明确丢弃上一条内容，之前的保存失败提示不再有意义。
      saveError: null,
      // 让在途保存的结果失效，并通知润色面板清空上一条 Prompt 的结果。
      editSession: get().editSession + 1,
    });
  },

  isDirty: () => {
    const { title, content, savedSnapshot } = get();
    return title !== savedSnapshot.title || content !== savedSnapshot.content;
  },

  loadPrompt: async (id: string) => {
    if (!hasTauriInvoke()) return;

    // 切换编辑目标前先把还没到 3 秒防抖的修改落库。loadPrompt 会替换
    // title/content，effect cleanup 随即清掉自动保存 timer，不 flush 就永久丢失。
    // 保存失败时保持当前编辑器内容不变，避免用户以为已经切换成功。
    if (get().isDirty()) {
      try {
        await get().savePrompt();
      } catch {
        // savePrompt 已把具体原因写进 saveError。留在当前 Prompt，
        // 不要让用户以为已经切换成功。
        return;
      }
    }

    // 切换编辑目标时取消在途的流式润色，结果不该写进新打开的 Prompt。
    void get().cancelPolishStream();

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
        savedSnapshot: { title: prompt.title, content: prompt.content },
        error: null,
        saveError: null,
        // 加载完成才算真正切换：失败时不 bump，编辑器仍属于上一条 Prompt。
        editSession: get().editSession + 1,
      });
      await get().recordPromptUse(id);
    } catch (e) {
      if (!isLatest()) return;
      const msg = getErrorMessage(e);
      console.error("Failed to load prompt:", msg);
      set({ error: msg });
    }
  },

  savePrompt: () => {
    if (!hasTauriInvoke()) {
      set({ error: "PromptDock desktop runtime is unavailable" });
      return Promise.resolve();
    }

    // 记录这次保存属于哪个编辑会话。排队/网络等待期间用户可能新建或切到
    // 另一条 Prompt，此时把旧结果写回状态会污染新 Prompt（标题、快照错位，
    // 下一次自动保存再覆盖错记录）。
    const session = get().editSession;
    const seq = ++saveRequestSeq;

    // 串行执行（原因见 saveQueue 声明处）。注意读 get() 的时机在执行时而不是
    // 入队时：排队期间状态可能已被前一个保存或用户输入改变。
    const run = saveQueue.then(async () => {
      // 入队后编辑目标已切换：这次保存的数据不再属于当前会话，跳过。
      if (session !== get().editSession) return;

      const { currentId, title, content, optimizedContent } = get();
      // 新建且完全空白时不落库：保存队列的迟到执行不应生成空记录。
      if (!currentId && !title.trim() && !content.trim()) return;

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
          // 写入已经落在旧记录上是对的，但状态回写必须属于同一会话。
          if (session !== get().editSession) return;
          set({
            title: effectiveTitle,
            savedSnapshot: { title: effectiveTitle, content },
            saveError: null,
          });
        } else {
          const prompt = await invoke<Prompt>("create_prompt", {
            title: effectiveTitle,
            content,
            optimizedContent,
          });
          // 创建期间用户已新建/切换：记录保留（内容没有丢），但不要把它
          // 重新挂回当前编辑器。
          if (session !== get().editSession) return;
          set({
            currentId: prompt.id,
            title: effectiveTitle,
            savedSnapshot: { title: effectiveTitle, content },
            saveError: null,
          });
        }
      } catch (e) {
        if (session === get().editSession) {
          set({ saveError: `保存失败：${getErrorMessage(e)}` });
        }
        throw e;
      } finally {
        // 只有最新一次保存有权复位，否则会在新保存进行中把 loading 关掉。
        if (seq === saveRequestSeq) set({ isSaving: false });
      }
    });
    // 链上挂吞错分支，一次保存失败不会卡死后续所有保存；
    // 错误本身仍通过 run 抛给调用方（toast / error）。
    saveQueue = run.catch(() => {});
    return run;
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
      // 调用方已经有 toast 了，这里只留日志，避免 toast 消失后横幅还挂着。
      console.error("Failed to delete prompt:", getErrorMessage(e));
      throw e;
    }
  },

  recordPromptUse: async (id: string) => {
    if (!hasTauriInvoke()) return;

    try {
      await invoke("record_prompt_use", { id });
      await get().loadPrompts();
    } catch (e) {
      // 「最近使用」只是个统计，失败不该打断用户，也不值得弹横幅。
      console.error("Failed to record prompt use:", getErrorMessage(e));
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
      console.error("Failed to toggle favorite:", getErrorMessage(e));
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
      // 加载成功就清掉上一次的失败提示，用户不需要手动关。
      set({ prompts, error: null });
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
      set({ searchResults: results, error: null });
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
    set({ searchQuery: "", searchResults: [], isSearching: false, error: null });
  },

  clearError: () => set({ error: null }),
  clearSaveError: () => set({ saveError: null }),
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
    // 流式竞态守卫：取消或发起新流之后，旧流的迟到 delta 和最终结果一律丢弃。
    const isCurrent = beginPolishStream();
    const requestId = crypto.randomUUID();
    set({ isPolishing: true, polishRequestId: requestId });
    try {
      const channel = new Channel<PolishStreamEvent>((event) => {
        if (event.type === "delta" && event.delta && isCurrent()) {
          onDelta(event.delta);
        }
      });

      const result = await invoke<string | null>("polish_prompt_stream", {
        content: sourceContent,
        mode,
        requestId,
        onChunk: channel,
      });

      // 已取消：结果丢弃（调用方的 runId 守卫同样会拦住，这里双保险）。
      if (!isCurrent()) return "";

      if (typeof result === "string" && result.length > 0) {
        return result;
      }

      return await invoke<string>("polish_prompt", {
        content: sourceContent,
        mode,
      });
    } catch (e) {
      // 后端取消是一种正常收尾，不是错误：返回空串让调用方静默结束。
      // 即使前端守卫尚未作废（例如别处按 requestId 取消），也不能再回退到
      // 非流式接口重发一次请求。
      if (getErrorMessage(e) === POLISH_CANCELLED_MESSAGE) {
        return "";
      }
      if (isMissingStreamCommand(getErrorMessage(e))) {
        return await invoke<string>("polish_prompt", {
          content: sourceContent,
          mode,
        });
      }
      throw e;
    } finally {
      // 只有没被取消/替换的那次流才有权复位状态，否则会把新流误判为已结束。
      if (isCurrent()) {
        set({ isPolishing: false, polishRequestId: null });
      }
    }
  },

  cancelPolishStream: async () => {
    const { polishRequestId } = get();
    if (!polishRequestId) return;

    // 先作废守卫：之后到达的 delta 和最终结果都被 polishPromptStream 丢弃。
    beginPolishStream();
    set({ polishRequestId: null, isPolishing: false });

    if (!hasTauriInvoke()) return;
    try {
      // 后端按 requestId 停流，幂等；取消不存在的 id 不报错。
      await invoke("cancel_polish_stream", { requestId: polishRequestId });
    } catch (e) {
      // 取消是尽力而为，失败只留日志。
      console.error("Failed to cancel polish stream:", getErrorMessage(e));
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

/** 与 `src-tauri/src/ai.rs` 的 `POLISH_CANCELLED` 保持一致。 */
const POLISH_CANCELLED_MESSAGE = "POLISH_CANCELLED";
