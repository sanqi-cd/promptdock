interface DeleteConfirmModalProps {
  open: boolean;
  title: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function DeleteConfirmModal({
  open,
  title,
  onConfirm,
  onCancel,
}: DeleteConfirmModalProps) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onCancel} />

      {/* Dialog */}
      <div className="relative w-full max-w-sm bg-[#111827] border border-white/[0.08] rounded-2xl shadow-2xl shadow-black/40 p-6">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-9 h-9 rounded-full bg-red-500/10 flex items-center justify-center shrink-0">
            <svg
              xmlns="http://www.w3.org/2000/svg" width="18" height="18"
              viewBox="0 0 24 24" fill="none" stroke="currentColor"
              strokeWidth="1.8" strokeLinecap="round"
              className="text-red-400"
            >
              <circle cx="12" cy="12" r="10" />
              <line x1="12" y1="8" x2="12" y2="12" />
              <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
          </div>
          <div>
            <h3 className="text-[14px] font-semibold text-slate-100">确认删除</h3>
            <p className="text-[12px] text-slate-500 mt-0.5">
              确定要删除 "{title}" 吗？此操作不可恢复。
            </p>
          </div>
        </div>
        <div className="flex gap-2 justify-end">
          <button
            onClick={onCancel}
            className="h-8 px-4 rounded-lg text-[12px] font-medium text-slate-300 bg-white/[0.04] border border-white/[0.06] hover:bg-white/[0.06] transition-colors"
          >
            取消
          </button>
          <button
            onClick={onConfirm}
            className="h-8 px-4 rounded-lg text-[12px] font-medium text-white bg-red-500/80 hover:bg-red-500 transition-colors"
          >
            删除
          </button>
        </div>
      </div>
    </div>
  );
}
