type TauriInternals = {
  __TAURI_INTERNALS__?: {
    invoke?: unknown;
  };
};

export function hasTauriInvoke(): boolean {
  if (typeof window === "undefined") return false;

  const internals = (window as Window & TauriInternals).__TAURI_INTERNALS__;
  return typeof internals?.invoke === "function";
}
