import { create } from "zustand";

const STORAGE_KEY = "promptdock_custom_categories";

function loadCustomCategories(): string[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return [
      ...new Set(
        parsed.filter(
          (item): item is string => typeof item === "string" && item.trim().length > 0
        )
      ),
    ];
  } catch {
    return [];
  }
}

function saveCustomCategories(categories: string[]): void {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(categories));
}

interface CategoryState {
  customCategories: string[];
  addCustomCategory: (name: string) => boolean;
  removeCustomCategory: (name: string) => void;
}

export const useCategoryStore = create<CategoryState>((set, get) => ({
  customCategories: loadCustomCategories(),

  addCustomCategory: (name) => {
    const category = name.trim();
    if (!category || get().customCategories.includes(category)) return false;
    const customCategories = [...get().customCategories, category];
    saveCustomCategories(customCategories);
    set({ customCategories });
    return true;
  },

  removeCustomCategory: (name) => {
    const customCategories = get().customCategories.filter((category) => category !== name);
    saveCustomCategories(customCategories);
    set({ customCategories });
  },
}));
