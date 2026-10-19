interface DiscardConfirmModalProps {
  open: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * 「新建空白」前的未保存内容确认。样式与 DeleteConfirmModal 同款，
 * 只是语义是「丢弃修改」而不是删除记录，所以用警示色而不是红色。
 */
export function DiscardConfirmModal({
  open,
  onConfirm,
  onCancel,
}: DiscardConfirmModalProps) {
  if (!open) return null;

  return (
    <div
      data-modal-open="true"
      className="fixed inset-0 z-50 flex items-center justify-center"
    >
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onCancel} />

      {/* Dialog */}
      <div className="relative w-full max-w-sm bg-[#111827] border border-white/[0.08] rounded-2xl shadow-2xl shadow-black/40 p-6">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-9 h-9 rounded-full bg-amber-500/10 flex items-center justify-center shrink-0">
            <svg
              xmlns="http://www.w3.org/2000/svg" width="18" height="18"
              viewBox="0 0 24 24" fill="none" stroke="currentColor"
              strokeWidth="1.8" strokeLinecap="round"
              className="text-amber-400"
            >
              <circle cx="12" cy="12" r="10" />
              <line x1="12" y1="8" x2="12" y2="12" />
              <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
          </div>
          <div>
            <h3 className="text-[14px] font-semibold text-slate-100">丢弃未保存的修改</h3>
            <p className="text-[12px] text-slate-500 mt-0.5">
              当前内容还未保存，新建空白后这些修改将丢失。
            </p>
          </div>
        </div>
        <div className="flex gap-2 justify-end">
          <button
            onClick={onCancel}
            className="h-8 px-4 rounded-lg text-[12px] font-medium text-slate-300 bg-white/[0.04] border border-white/[0.06] hover:bg-white/[0.06] transition-colors"
          >
            继续编辑
          </button>
          <button
            onClick={onConfirm}
            className="h-8 px-4 rounded-lg text-[12px] font-medium text-white bg-amber-500/80 hover:bg-amber-500 transition-colors"
          >
            丢弃并新建
          </button>
        </div>
      </div>
    </div>
  );
}
