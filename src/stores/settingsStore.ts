import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { hasTauriInvoke } from "@/lib/tauriRuntime";
import type { ApiConfig } from "@/types";

interface SettingsState {
  configs: ApiConfig[];
  activeConfig: ApiConfig | null;
  isLoaded: boolean;

  loadConfigs: () => Promise<void>;
  saveConfig: (
    name: string,
    provider: string,
    baseUrl: string,
    apiKey: string,
    model: string,
  ) => Promise<void>;
  updateConfig: (
    id: string,
    name: string,
    provider: string,
    baseUrl: string,
    apiKey: string,
    model: string,
  ) => Promise<void>;
  deleteConfig: (id: string) => Promise<void>;
  activateConfig: (id: string) => Promise<void>;
  getActiveConfig: () => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  configs: [],
  activeConfig: null,
  isLoaded: false,

  loadConfigs: async () => {
    if (!hasTauriInvoke()) {
      set({ configs: [], activeConfig: null, isLoaded: true });
      return;
    }

    try {
      const configs = await invoke<ApiConfig[]>("list_api_configs");
      const active = configs.find((c) => c.is_active) || null;
      set({ configs, activeConfig: active, isLoaded: true });
    } catch (e) {
      console.error("Failed to load API configs:", getErrorMessage(e));
      set({ isLoaded: true });
    }
  },

  saveConfig: async (name, provider, baseUrl, apiKey, model) => {
    if (!hasTauriInvoke()) return;

    await invoke<ApiConfig>("save_api_config", {
      name,
      provider,
      baseUrl,
      apiKey,
      model,
    });
    await get().loadConfigs();
  },

  updateConfig: async (id, name, provider, baseUrl, apiKey, model) => {
    if (!hasTauriInvoke()) return;

    await invoke("update_api_config", {
      id,
      name,
      provider,
      baseUrl,
      apiKey,
      model,
    });
    await get().loadConfigs();
  },

  deleteConfig: async (id) => {
    if (!hasTauriInvoke()) return;

    await invoke("delete_api_config", { id });
    await get().loadConfigs();
  },

  activateConfig: async (id) => {
    if (!hasTauriInvoke()) return;

    await invoke("activate_api_config", { id });
    await get().loadConfigs();
  },

  getActiveConfig: async () => {
    if (!hasTauriInvoke()) {
      set({ activeConfig: null });
      return;
    }

    const config = await invoke<ApiConfig | null>("get_active_config");
    set({ activeConfig: config });
  },
}));

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "An unexpected error occurred";
}
