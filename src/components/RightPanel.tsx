import { usePromptStore } from "@/stores/promptStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useToast } from "@/components/ui/toast";
import { getErrorMessage } from "@/lib/utils";

interface RightPanelProps {
  context: "editor" | "list";
}

export function RightPanel({ context }: RightPanelProps) {
  const currentId = usePromptStore((s) => s.currentId);
  const content = usePromptStore((s) => s.content);
  const isFavorite = usePromptStore((s) => s.isFavorite);
  const category = usePromptStore((s) => s.category);
  const summary = usePromptStore((s) => s.summary);
  const tags = usePromptStore((s) => s.tags);
  const isClassifying = usePromptStore((s) => s.isClassifying);
  const prompts = usePromptStore((s) => s.prompts);
  const savePrompt = usePromptStore((s) => s.savePrompt);
  const toggleFavorite = usePromptStore((s) => s.toggleFavorite);
  const autoClassify = usePromptStore((s) => s.autoClassify);
  const applyClassifyResult = usePromptStore((s) => s.applyClassifyResult);
  const newPrompt = usePromptStore((s) => s.newPrompt);

  const activeConfig = useSettingsStore((s) => s.activeConfig);
  const { addToast } = useToast();

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(content);
      addToast("已复制到剪贴板", "success");
    } catch {
      addToast("复制失败，请重试", "error");
    }
  };

  const handleSave = async () => {
    if (!content.trim()) return;
    try {
      await savePrompt();
      addToast("已保存", "success");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    }
  };

  const handleFavorite = async () => {
    try {
      await toggleFavorite();
      addToast(isFavorite ? "已取消收藏" : "已收藏", "success");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    }
  };

  const handleClear = () => {
    newPrompt();
    addToast("已清空", "info");
  };

  const handleAutoClassify = async () => {
    if (!content.trim()) {
      addToast("请先输入 Prompt 内容", "error");
      return;
    }
    if (!activeConfig) {
      addToast("请先在设置中配置并激活一个模型 API", "error");
      return;
    }
    try {
      if (!usePromptStore.getState().currentId) {
        await savePrompt();
      }
      const result = await autoClassify();
      await applyClassifyResult(result);
      addToast("自动分类完成", "success");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    }
  };

  const favCount = prompts.filter((p) => p.is_favorite).length;
  const catCount = new Set(prompts.map((p) => p.category).filter(Boolean)).size;

  return (
    <aside className="w-[300px] border-l border-[var(--sidebar-border)] bg-[var(--sidebar)] flex flex-col shrink-0 overflow-y-auto">
      <div className="px-4 py-4 space-y-3">
        {context === "editor" ? (
          <>
            {/* Prompt Info Card */}
            <div className="rounded-xl border border-white/[0.06] bg-white/[0.02]">
              <div className="flex items-center gap-2 px-4 pt-3.5 pb-2">
                <span className="text-[13px] font-semibold text-slate-100">
                  Prompt 信息
                </span>
              </div>
              <div className="px-4 pb-3.5 space-y-0">
                {activeConfig ? (
                  <>
                    {!currentId && !isClassifying && content.trim() && (
                      <div className="flex items-center gap-2 py-2">
                        <span className="text-[11px] text-purple-400">
                          保存后可自动分类
                        </span>
                      </div>
                    )}
                    {isClassifying && (
                      <div className="flex items-center gap-2 py-2">
                        <span className="text-[11px] text-purple-400">
                          自动分类中...
                        </span>
                      </div>
                    )}
                    <InfoRow label="分类" value={category || "—"} dot="#A78BFA" />
                    <InfoRow label="标签" value={tags.length > 0 ? tags.join("、") : "—"} />
                    <InfoRow label="预计用途" value={summary ? summary.slice(0, 20) : "—"} />
                    <InfoRow label="推荐工具" value={activeConfig.model} />
                    <button
                      onClick={handleAutoClassify}
                      disabled={isClassifying || !content.trim()}
                      className="mt-2 w-full text-[11px] text-purple-400 hover:text-purple-300 py-1.5 rounded-md transition-colors text-left disabled:opacity-40 disabled:cursor-not-allowed"
                    >
                      {isClassifying ? "分类中..." : "自动分类 →"}
                    </button>
                  </>
                ) : (
                  <>
                    <InfoRow label="分类" value={category || "—"} dot="#A78BFA" />
                    <InfoRow label="标签" value={tags.length > 0 ? tags.join("、") : "—"} />
                    <div className="flex items-center gap-2 py-2">
                      <span className="text-[11px] text-slate-500">
                        配置 API 后可自动分类
                      </span>
                    </div>
                  </>
                )}
              </div>
            </div>

            {/* Quick Actions Card */}
            <div className="rounded-xl border border-white/[0.06] bg-white/[0.02]">
              <div className="px-4 pt-3.5 pb-2">
                <span className="text-[13px] font-semibold text-slate-100">
                  快捷操作
                </span>
              </div>
              <div className="px-2.5 pb-3 space-y-0.5">
                <ActionRow icon="copy" label="复制 Prompt" onClick={handleCopy} disabled={!content.trim()} />
                <ActionRow icon="save" label="保存 Prompt" shortcut="⌘S" onClick={handleSave} disabled={!content.trim()} />
                <ActionRow icon="star" label={isFavorite ? "取消收藏" : "收藏 Prompt"} shortcut="⌘D" onClick={handleFavorite} disabled={!currentId} />
                <ActionRow icon="trash" label="新建空白" onClick={handleClear} />
              </div>
            </div>
          </>
        ) : (
          /* Stats Card for list/explore context */
          <div className="rounded-xl border border-white/[0.06] bg-white/[0.02]">
            <div className="px-4 pt-3.5 pb-2">
              <span className="text-[13px] font-semibold text-slate-100">统计</span>
            </div>
            <div className="px-4 pb-3.5 space-y-0">
              <InfoRow label="总数" value={String(prompts.length)} />
              <InfoRow label="收藏" value={String(favCount)} />
              <InfoRow label="分类" value={String(catCount)} />
              <InfoRow label="API" value={activeConfig ? activeConfig.model : "未配置"} />
            </div>
          </div>
        )}

        {/* Tips Card */}
        <div className="rounded-xl border border-white/[0.06] bg-white/[0.02]">
          <div className="flex items-center gap-2 px-4 pt-3.5 pb-2">
            <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15"
                 viewBox="0 0 24 24" fill="none" stroke="currentColor"
                 strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"
                 className="text-amber-500">
              <path d="M9 18h6" /><path d="M10 22h4" />
              <path d="M15.09 14c.18-.98.65-1.74 1.41-2.5A4.65 4.65 0 0 0 18 8 6 6 0 0 0 6 8c0 1 .23 2.23 1.5 3.5A4.61 4.61 0 0 1 8.91 14" />
            </svg>
            <span className="text-[13px] font-semibold text-slate-100">提示</span>
          </div>
          <div className="px-4 pb-3.5 space-y-2">
            <p className="text-[11px] text-slate-500 leading-relaxed">
              保存 Prompt 后可使用收藏和自动分类
            </p>
            <p className="text-[11px] text-slate-500 leading-relaxed">
              AI 润色支持优化、结构化、精简和专家增强
            </p>
            <p className="text-[11px] text-slate-500 leading-relaxed">
              使用 <span className="text-slate-400 font-medium">⌘K</span> 打开全局搜索，快速查找 Prompt
            </p>
          </div>
        </div>
      </div>
    </aside>
  );
}

function InfoRow({ label, value, dot }: { label: string; value: string; dot?: string }) {
  return (
    <>
      <div className="h-px bg-white/[0.04]" />
      <div className="flex items-center gap-2 py-2">
        <span className="text-[11px] text-slate-500 w-[56px] shrink-0">{label}</span>
        {dot && (
          <div
            className="w-1.5 h-1.5 rounded-full shrink-0"
            style={{ backgroundColor: dot }}
          />
        )}
        <span className="text-[11px] text-slate-300 truncate">{value || "—"}</span>
      </div>
    </>
  );
}

function ActionRow({
  icon,
  label,
  shortcut,
  onClick,
  disabled,
}: {
  icon: string;
  label: string;
  shortcut?: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="flex items-center gap-2.5 w-full px-2.5 py-2 rounded-lg text-left hover:bg-white/[0.04] transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
    >
      <ActionIcon name={icon} />
      <span className="text-[12px] text-slate-300 font-medium">{label}</span>
      <div className="flex-1" />
      {shortcut && (
        <kbd className="text-[10px] text-slate-600 bg-black/30 px-1.5 py-0.5 rounded font-mono">
          {shortcut}
        </kbd>
      )}
    </button>
  );
}

function ActionIcon({ name }: { name: string }) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15"
         viewBox="0 0 24 24" fill="none" stroke="currentColor"
         strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"
         className="text-slate-500 shrink-0">
      {name === "copy" && <><rect x="9" y="9" width="13" height="13" rx="2" ry="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></>}
      {name === "save" && <><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" /><polyline points="17 21 17 13 7 13 7 21" /><polyline points="7 3 7 8 15 8" /></>}
      {name === "star" && <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />}
      {name === "trash" && <><polyline points="3 6 5 6 21 6" /><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" /></>}
    </svg>
  );
}
