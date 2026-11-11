export {};

declare global {
  interface Window {
    __TAURI_INTERNALS__: {
      invoke: (cmd: string, args?: Record<string, unknown>) => unknown;
      transformCallback?: (callback: (...args: unknown[]) => unknown) => number;
      unregisterCallback?: (id: number) => void;
      runCallback?: (id: number, data: unknown) => unknown;
      callbacks?: Map<number, (...args: unknown[]) => unknown>;
    };
    __polishCalls: Record<string, unknown>[] | string[] | { cmd: string }[];
    __timingEvents: Array<{ cmd: string; at: number }>;
    __createCalls: Record<string, unknown>[];
    __saveCalls: Record<string, unknown>[];
    __cancelCalls: Record<string, unknown>[];
    __favoriteCalls: number;
  }
}
