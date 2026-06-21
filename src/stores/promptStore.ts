import { create } from "zustand";
import { Channel, invoke } from "@tauri-apps/api/core";
import { hasTauriInvoke } from "@/lib/tauriRuntime";
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

    try {
      const prompt = await invoke<Prompt>("get_prompt", { id });
      const tags = await invoke<{ id: string; name: string }[]>("get_prompt_tags", {
        promptId: id,
      });
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
    set({ isSaving: true });
    try {
      if (currentId) {
        await invoke("update_prompt", {
          id: currentId,
          title,
          content,
          optimizedContent,
        });
      } else {
        const prompt = await invoke<Prompt>("create_prompt", {
          title,
          content,
          optimizedContent,
        });
        set({ currentId: prompt.id });
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

    const { selectedCategory, favoriteOnly } = get();
    try {
      const prompts = await invoke<Prompt[]>("list_prompts", {
        category: selectedCategory,
        favoriteOnly,
      });
      set({ prompts });
    } catch (e) {
      const msg = getErrorMessage(e);
      console.error("Failed to load prompts:", msg);
      set({ error: msg });
    }
  },

  search: async (query: string) => {
    set({ isSearching: true, searchQuery: query });
    try {
      if (!query.trim()) {
        set({ searchResults: [] });
        return;
      }
      if (!hasTauriInvoke()) {
        set({ searchResults: [] });
        return;
      }
      const results = await invoke<SearchResult[]>("search_prompts", { query });
      set({ searchResults: results });
    } finally {
      set({ isSearching: false });
    }
  },

  setSearchQuery: (query: string) => set({ searchQuery: query }),
  setSelectedCategory: (category: string | null) => {
    set({ selectedCategory: category });
    get().loadPrompts();
  },
  setFavoriteOnly: (fav: boolean) => {
    set({ favoriteOnly: fav });
    get().loadPrompts();
  },

  analyzePolishProcess: async (content: string, mode: PolishMode) => {
    return await invoke<PolishProcessStep[]>("analyze_polish_process", {
      content,
      mode,
    });
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
      const message = getErrorMessage(e);
      if (
        message.includes("polish_prompt_stream") ||
        message.includes("unknown command") ||
        message.includes("not found")
      ) {
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
    const { currentId, optimizedContent } = get();
    if (!currentId) return;
    if (!hasTauriInvoke()) return;

    await invoke("update_prompt_meta", {
      id: currentId,
      category: result.category,
      summary: result.summary,
      optimizedContent,
      recommendedTool: result.recommended_tool,
    });
    await invoke("set_prompt_tags", {
      promptId: currentId,
      tagNames: result.tags,
    });
    set({
      category: result.category,
      summary: result.summary,
      tags: result.tags,
    });
  },
}));

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "An unexpected error occurred";
}
