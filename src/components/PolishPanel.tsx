import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { usePromptStore } from "@/stores/promptStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useToast } from "@/components/ui/toast";
import { cn, getErrorMessage } from "@/lib/utils";
import type { PolishMode, PolishProcessStep } from "@/types";

const POLISH_MODES: { value: PolishMode; label: string; desc: string; icon: string }[] = [
  { value: "general", label: "优化", desc: "日常表达优化", icon: "sparkles" },
  { value: "structured", label: "结构化", desc: "整理成模块", icon: "list" },
  { value: "concise", label: "精简", desc: "压缩成短模板", icon: "compress" },
];

const ADVANCED_POLISH_MODE: { value: PolishMode; label: string; desc: string; icon: string } = {
  value: "professional",
  label: "专家增强",
  desc: "升级为正式任务说明，补充角色、步骤、约束和质量标准",
  icon: "shield-check",
};

const PROCESS_ANALYSIS_DELAY_MS = 800;

export function PolishPanel() {
  const [mode, setMode] = useState<PolishMode>("general");
  const [result, setResult] = useState<string | null>(null);
  const [processSteps, setProcessSteps] = useState<PolishProcessStep[]>([]);
  const [processError, setProcessError] = useState<string | null>(null);
  const [isAnalyzingProcess, setIsAnalyzingProcess] = useState(false);
  const [activeProgressStep, setActiveProgressStep] = useState(0);
  const polishRunIdRef = useRef(0);

  const content = usePromptStore((s) => s.content);
  const isPolishing = usePromptStore((s) => s.isPolishing);
  const analyzePolishProcess = usePromptStore((s) => s.analyzePolishProcess);
  const polishPromptStream = usePromptStore((s) => s.polishPromptStream);
  const activeConfig = useSettingsStore((s) => s.activeConfig);
  const { addToast } = useToast();
  const isProcessing = isAnalyzingProcess || isPolishing;

  useEffect(() => {
    if (!isProcessing || processSteps.length === 0) {
      setActiveProgressStep(0);
      return;
    }

    setActiveProgressStep(0);
    const interval = window.setInterval(() => {
      setActiveProgressStep((step) =>
        Math.min(step + 1, processSteps.length - 1)
      );
    }, 900);

    return () => window.clearInterval(interval);
  }, [isProcessing, processSteps.length]);

  const handlePolish = async () => {
    if (!content.trim()) {
      addToast("请先输入 Prompt 内容", "error");
      return;
    }
    if (!activeConfig) {
      addToast("请先在设置中配置并激活一个模型 API", "error");
      return;
    }
    const sourceContent = content.trim();
    const sourceMode = mode;
    setResult(null);
    setProcessSteps([]);
    setProcessError(null);

    let processTimer: number | undefined;
    try {
      const runId = polishRunIdRef.current + 1;
      polishRunIdRef.current = runId;
      const isCurrentRun = () => polishRunIdRef.current === runId;
      let streamedContent = "";
      processTimer = window.setTimeout(() => {
        if (!isCurrentRun() || streamedContent.length > 0) {
          return;
        }

        setIsAnalyzingProcess(true);
        void analyzePolishProcess(sourceContent, sourceMode)
          .then((steps) => {
            if (isCurrentRun()) {
              setProcessSteps(steps);
            }
          })
          .catch(() => {
            if (isCurrentRun()) {
              setProcessError("处理要点暂不可用，正在继续润色");
            }
          })
          .finally(() => {
            if (isCurrentRun()) {
              setIsAnalyzingProcess(false);
            }
          });
      }, PROCESS_ANALYSIS_DELAY_MS);

      const optimized = await polishPromptStream(sourceMode, sourceContent, (delta) => {
        if (!isCurrentRun()) {
          return;
        }
        streamedContent += delta;
        setResult(streamedContent);
      });
      window.clearTimeout(processTimer);
      if (!isCurrentRun()) {
        return;
      }
      setResult(optimized);
      setIsAnalyzingProcess(false);
      polishRunIdRef.current = runId + 1;
      usePromptStore.getState().setOptimizedContent(optimized);
      await usePromptStore.getState().savePrompt();
      addToast("润色完成", "success");
    } catch (e) {
      if (processTimer) {
        window.clearTimeout(processTimer);
      }
      setIsAnalyzingProcess(false);
      polishRunIdRef.current += 1;
      addToast(getErrorMessage(e), "error");
    }
  };

  const handleApply = () => {
    if (result) {
      usePromptStore.getState().setContent(result);
      setResult(null);
      addToast("已替换原文", "success");
    }
  };

  const handleCopy = () => {
    if (result) {
      navigator.clipboard.writeText(result);
      addToast("已复制到剪贴板", "success");
    }
  };

  return (
    <div className="space-y-3">
      {/* Main polish modes */}
      <div className="grid grid-cols-3 gap-2.5">
        {POLISH_MODES.map((m) => {
          const active = mode === m.value;
          return (
            <button
              key={m.value}
              onClick={() => setMode(m.value)}
              disabled={isProcessing}
              className={cn(
                "flex flex-col items-start gap-1.5 rounded-[10px] border p-3 text-left transition-all",
                active
                  ? "border-purple-500/40 bg-purple-500/[0.06] ring-1 ring-purple-500/20"
                  : "border-white/[0.06] bg-white/[0.02] hover:border-white/[0.10] hover:bg-white/[0.03]",
                isProcessing && "cursor-not-allowed opacity-70"
              )}
            >
              <ModeIcon name={m.icon} active={active} />
              <span className={cn("text-[12px] font-semibold", active ? "text-purple-200" : "text-slate-200")}>
                {m.label}
              </span>
              <span className="text-[11px] text-slate-500 leading-tight">{m.desc}</span>
            </button>
          );
        })}
      </div>

      {/* Advanced polish mode */}
      {(() => {
        const active = mode === ADVANCED_POLISH_MODE.value;
        return (
          <button
            onClick={() => setMode(ADVANCED_POLISH_MODE.value)}
            disabled={isProcessing}
            className={cn(
              "flex w-full items-center gap-3 rounded-[10px] border px-3 py-2.5 text-left transition-all",
              active
                ? "border-purple-500/40 bg-purple-500/[0.06] ring-1 ring-purple-500/20"
                : "border-white/[0.06] bg-white/[0.02] hover:border-white/[0.10] hover:bg-white/[0.03]",
              isProcessing && "cursor-not-allowed opacity-70"
            )}
          >
            <ModeIcon name={ADVANCED_POLISH_MODE.icon} active={active} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className={cn("text-[12px] font-semibold", active ? "text-purple-200" : "text-slate-200")}>
                  {ADVANCED_POLISH_MODE.label}
                </span>
                <span className="rounded-full bg-purple-500/10 px-1.5 py-0.5 text-[10px] font-medium text-purple-400">
                  进阶
                </span>
              </div>
              <span className="mt-0.5 block truncate text-[11px] text-slate-500">
                {ADVANCED_POLISH_MODE.desc}
              </span>
            </div>
          </button>
        );
      })()}

      {/* Submit button */}
      <Button
        onClick={handlePolish}
        disabled={isProcessing || !content.trim()}
        className="w-full h-11 rounded-[10px] bg-gradient-to-br from-purple-600 to-blue-500 hover:from-purple-500 hover:to-blue-400 text-white text-[13px] font-semibold shadow-lg shadow-purple-500/25 transition-all active:scale-[0.98] border-0"
      >
        <span className="mr-1.5">✨</span>
        {isProcessing ? "AI 润色中..." : "开始润色（⌘↵）"}
      </Button>

      {isProcessing && !result && (isAnalyzingProcess || processSteps.length > 0 || processError) && (
        <PolishProgress
          activeStep={activeProgressStep}
          isAnalyzing={isAnalyzingProcess}
          processError={processError}
          steps={processSteps}
        />
      )}

      {/* Result */}
      {result && (
        <div className="space-y-2 border border-white/[0.06] rounded-[10px] p-3 bg-black/20">
          <div className="text-[11px] font-medium text-slate-500">
            润色结果
          </div>
          <Textarea
            value={result}
            readOnly
            className="min-h-[80px] text-[13px] bg-white/[0.02] border-white/[0.06] text-slate-200"
          />
          <div className="flex gap-1.5">
            <Button size="xs" onClick={handleApply} className="h-7 text-[11px]">
              替换原文
            </Button>
            <Button size="xs" variant="outline" onClick={handleCopy} className="h-7 text-[11px]">
              复制结果
            </Button>
            <Button size="xs" variant="ghost" onClick={() => setResult(null)} className="h-7 text-[11px] text-slate-400">
              关闭
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function PolishProgress({
  activeStep,
  isAnalyzing,
  processError,
  steps,
}: {
  activeStep: number;
  isAnalyzing: boolean;
  processError: string | null;
  steps: PolishProcessStep[];
}) {
  return (
    <div className="rounded-[10px] border border-white/[0.06] bg-white/[0.02] p-3">
      <div className="mb-2.5 flex items-center gap-2">
        <span className="relative flex h-2 w-2">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-purple-400 opacity-60" />
          <span className="relative inline-flex h-2 w-2 rounded-full bg-purple-400" />
        </span>
        <span className="text-[11px] font-semibold text-slate-200">
          优化过程
        </span>
        <span className="text-[11px] text-slate-500">
          AI 正在生成本次处理要点
        </span>
      </div>

      <div className="grid gap-1.5">
        {isAnalyzing && steps.length === 0 && (
          <div className="flex items-start gap-2 rounded-lg px-2 py-1.5 bg-purple-500/[0.08]">
            <span className="mt-0.5 h-4 w-4 shrink-0 animate-pulse rounded-full border border-purple-400/70 bg-purple-500/10" />
            <div className="min-w-0">
              <div className="text-[11px] font-medium text-purple-200">
                分析输入
              </div>
              <div className="mt-0.5 text-[10px] leading-relaxed text-slate-500">
                等待模型返回本次专属处理要点
              </div>
            </div>
          </div>
        )}

        {!isAnalyzing && processError && steps.length === 0 && (
          <div className="flex items-start gap-2 rounded-lg px-2 py-1.5">
            <span className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-white/[0.08] text-[9px] text-slate-600">
              !
            </span>
            <div className="min-w-0">
              <div className="text-[11px] font-medium text-slate-400">
                继续润色
              </div>
              <div className="mt-0.5 text-[10px] leading-relaxed text-slate-500">
                {processError}
              </div>
            </div>
          </div>
        )}

        {steps.map((step, index) => {
          const isDone = index < activeStep;
          const isActive = index === activeStep;

          return (
            <div
              key={step.title}
              className={cn(
                "flex items-start gap-2 rounded-lg px-2 py-1.5 transition-colors",
                isActive && "bg-purple-500/[0.08]"
              )}
            >
              <span
                className={cn(
                  "mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border text-[9px]",
                  isDone
                    ? "border-purple-400/50 bg-purple-500/15 text-purple-300"
                    : isActive
                      ? "border-purple-400/70 bg-purple-500/10 text-purple-300"
                      : "border-white/[0.08] text-slate-600"
                )}
                aria-hidden="true"
              >
                {isDone ? (
                  <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
                    <polyline points="20 6 9 17 4 12" />
                  </svg>
                ) : (
                  index + 1
                )}
              </span>
              <div className="min-w-0">
                <div
                  className={cn(
                    "text-[11px] font-medium",
                    isActive || isDone ? "text-purple-200" : "text-slate-400"
                  )}
                >
                  {step.title}
                </div>
                <div className="mt-0.5 text-[10px] leading-relaxed text-slate-500">
                  {step.desc}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ModeIcon({ name, active }: { name: string; active: boolean }) {
  const color = active ? "#A78BFA" : "#6B7280";
  switch (name) {
    case "sparkles":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z" />
        </svg>
      );
    case "list":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <line x1="8" y1="6" x2="21" y2="6" />
          <line x1="8" y1="12" x2="21" y2="12" />
          <line x1="8" y1="18" x2="21" y2="18" />
          <line x1="3" y1="6" x2="3.01" y2="6" />
          <line x1="3" y1="12" x2="3.01" y2="12" />
          <line x1="3" y1="18" x2="3.01" y2="18" />
        </svg>
      );
    case "shield-check":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
          <path d="m9 12 2 2 4-4" />
        </svg>
      );
    case "compress":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="4 14 10 14 10 20" />
          <polyline points="20 10 14 10 14 4" />
          <line x1="14" y1="10" x2="21" y2="3" />
          <line x1="3" y1="21" x2="10" y2="14" />
        </svg>
      );
    default:
      return null;
  }
}
