import { useEffect, useRef, useState } from "react";
import { usePromptStore } from "@/stores/promptStore";

const AUTOSAVE_DELAY_MS = 3000;
const AUTOSAVE_RETRY_MAX_MS = 30_000;

/**
 * Debounced autosave.
 *
 * The hook is mounted at the app root, not inside the editor: navigating to
 * the list/settings unmounts the editor, and an editor-scoped timer would be
 * cleared on the way out, losing the last keystrokes.
 *
 * It also only writes when the editor is actually dirty. Loading a prompt
 * changes `title`/`content` but not `savedSnapshot`, so without this check
 * merely opening a record rewrote its `updated_at` (and reordered the list)
 * three seconds later.
 */
export function useAutoSave() {
  const currentId = usePromptStore((s) => s.currentId);
  const title = usePromptStore((s) => s.title);
  const content = usePromptStore((s) => s.content);
  const savePrompt = usePromptStore((s) => s.savePrompt);
  const lastSaveRef = useRef<string>("");
  const retryDelayRef = useRef<number>(AUTOSAVE_DELAY_MS);
  // Bumping this re-runs the effect after a failed save, scheduling a retry
  // with exponential backoff. The values themselves may not have changed.
  const [retryNonce, setRetryNonce] = useState(0);

  useEffect(() => {
    if (!usePromptStore.getState().isDirty()) {
      return;
    }

    const saveKey = `${currentId}:${title}:${content}`;
    if (saveKey === lastSaveRef.current) return;

    const timer = window.setTimeout(async () => {
      try {
        await usePromptStore.getState().savePrompt();
        lastSaveRef.current = saveKey;
        retryDelayRef.current = AUTOSAVE_DELAY_MS;
      } catch {
        // Store already recorded a persistent `saveError`; schedule a retry
        // with backoff so a transient lock (e.g. WAL contention) recovers.
        retryDelayRef.current = Math.min(
          retryDelayRef.current * 2,
          AUTOSAVE_RETRY_MAX_MS
        );
        setRetryNonce((nonce) => nonce + 1);
      }
    }, retryDelayRef.current);

    return () => window.clearTimeout(timer);
  }, [title, content, currentId, savePrompt, retryNonce]);
}
