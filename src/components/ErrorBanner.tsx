import { usePromptStore } from "@/stores/promptStore";

interface ErrorRowProps {
  message: string;
  onClose: () => void;
}

function ErrorRow({ message, onClose }: ErrorRowProps) {
  return (
    <div
      role="alert"
      className="flex items-start gap-2.5 mx-5 mt-4 px-3.5 py-2.5 rounded-[10px] border border-red-500/25 bg-red-500/[0.08]"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="15"
        height="15"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        className="mt-0.5 shrink-0 text-red-400"
        aria-hidden="true"
      >
        <circle cx="12" cy="12" r="9" />
        <path d="M12 8v4.5M12 16h.01" />
      </svg>

      <span className="flex-1 min-w-0 text-[12px] leading-relaxed text-red-200 break-words">
        {message}
      </span>

      <button
        type="button"
        onClick={onClose}
        aria-label="关闭错误提示"
        className="shrink-0 text-red-400/70 hover:text-red-200 transition-colors"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          aria-hidden="true"
        >
          <path d="M18 6 6 18M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
}

/**
 * 后台读写失败的持久提示条。
 *
 * - `error`：列表 / 详情 / 搜索读取失败。界面会显示空白或陈旧内容，
 *   没有提示时用户只会看到「没有数据」。
 * - `saveError`：保存（含自动保存重试）失败。数据还在内存里，但没落库，
 *   必须让用户知道并保留到下次保存成功。
 *
 * 两个槽位分开渲染：一次成功的列表刷新不该把一条仍未解决的详情错误清掉，
 * 反过来也一样。用户主动触发的操作（删除、收藏…）仍由调用方 toast 反馈。
 */
export function ErrorBanner() {
  const error = usePromptStore((s) => s.error);
  const saveError = usePromptStore((s) => s.saveError);
  const clearError = usePromptStore((s) => s.clearError);
  const clearSaveError = usePromptStore((s) => s.clearSaveError);

  if (!error && !saveError) return null;

  return (
    <>
      {saveError && <ErrorRow message={saveError} onClose={clearSaveError} />}
      {error && <ErrorRow message={error} onClose={clearError} />}
    </>
  );
}
