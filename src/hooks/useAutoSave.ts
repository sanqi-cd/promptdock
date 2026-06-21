import { useEffect, useRef } from "react";
import { usePromptStore } from "@/stores/promptStore";

export function useAutoSave() {
  const { currentId, title, content, savePrompt } = usePromptStore();
  const lastSaveRef = useRef<string>("");

  useEffect(() => {
    if (!title && !content) return;

    const saveKey = `${currentId}:${title}:${content}`;
    if (saveKey === lastSaveRef.current) return;

    const timer = setTimeout(() => {
      lastSaveRef.current = saveKey;
      savePrompt();
    }, 3000);

    return () => clearTimeout(timer);
  }, [title, content, currentId, savePrompt]);
}
