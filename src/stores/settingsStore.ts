import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { hasTauriInvoke } from "@/lib/tauriRuntime";
import type { ApiConfig } from "@/types";

interface SettingsState {
  configs: ApiConfig[];
  activeConfig: ApiConfig | null;
  isLoaded: boolean;
  /** 读取配置失败的原因。失败时 configs 为空，界面不能把它当成「没有配置」。 */
  error: string | null;

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
  error: null,

  loadConfigs: async () => {
    if (!hasTauriInvoke()) {
      set({ configs: [], activeConfig: null, isLoaded: true, error: null });
      return;
    }

    try {
      const configs = await invoke<ApiConfig[]>("list_api_configs");
      const active = configs.find((c) => c.is_active) || null;
      set({ configs, activeConfig: active, isLoaded: true, error: null });
    } catch (e) {
      const msg = getErrorMessage(e);
      console.error("Failed to load API configs:", msg);
      // 保留错误原因：否则设置页会显示「暂无 API 配置」、编辑器静默隐藏润色区，
      // 用户会把读取失败误判成真的没有配置。
      set({ isLoaded: true, error: msg });
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
