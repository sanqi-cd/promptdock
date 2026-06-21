import { create } from "zustand";

export type AppTheme = "dark" | "paper";

const THEME_STORAGE_KEY = "promptdock_theme";

function readStoredTheme(): AppTheme {
  if (typeof window === "undefined") return "dark";

  const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
  return stored === "paper" || stored === "dark" ? stored : "dark";
}

interface ThemeState {
  theme: AppTheme;
  setTheme: (theme: AppTheme) => void;
  toggleTheme: () => void;
}

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: readStoredTheme(),

  setTheme: (theme) => {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
    set({ theme });
  },

  toggleTheme: () => {
    const nextTheme = get().theme === "dark" ? "paper" : "dark";
    window.localStorage.setItem(THEME_STORAGE_KEY, nextTheme);
    set({ theme: nextTheme });
  },
}));
