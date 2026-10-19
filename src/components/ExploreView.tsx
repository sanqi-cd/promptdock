import { usePromptStore } from "@/stores/promptStore";
import { CATEGORIES } from "@/components/CategoryTags";
import { CategoryIcon } from "@/lib/categoryIcons";
import { formatDate } from "@/lib/formatDate";

interface ExploreViewProps {
  onSelectPrompt: (id: string) => void;
  onSelectCategory: (category: string) => void;
}

const CARD_GRADIENTS = [
  "from-purple-600/20 to-blue-500/10",
  "from-emerald-500/20 to-teal-500/10",
  "from-orange-500/20 to-yellow-500/10",
  "from-rose-500/20 to-pink-500/10",
  "from-cyan-500/20 to-blue-500/10",
  "from-violet-500/20 to-purple-500/10",
  "from-amber-500/20 to-orange-500/10",
  "from-sky-500/20 to-cyan-500/10",
  "from-pink-500/20 to-rose-500/10",
  "from-indigo-500/20 to-violet-500/10",
];

export function ExploreView({ onSelectPrompt, onSelectCategory }: ExploreViewProps) {
  const prompts = usePromptStore((s) => s.prompts);

  const categoryCounts: Record<string, number> = {};
  for (const p of prompts) {
    if (p.category) {
      categoryCounts[p.category] = (categoryCounts[p.category] || 0) + 1;
    }
  }

  const recent = [...prompts]
    .sort((a, b) =>
      (b.last_used_at || b.updated_at).localeCompare(a.last_used_at || a.updated_at)
    )
    .slice(0, 6);

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="p-6 space-y-6">
        {/* Header */}
        <div>
          <h1 className="text-[18px] font-semibold text-slate-100">探索</h1>
          <p className="text-[13px] text-slate-500 mt-1">
            按分类浏览 Prompt，或从最近使用中快速恢复
          </p>
        </div>

        {/* Category Grid */}
        <section>
          <h2 className="text-[13px] font-semibold text-slate-300 mb-3">分类浏览</h2>
          <div className="grid grid-cols-2 gap-3">
            {CATEGORIES.map((cat, i) => (
              <button
                key={cat}
                onClick={() => onSelectCategory(cat)}
                className="flex flex-col items-start gap-3 p-4 rounded-xl border border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.04] hover:border-white/[0.1] transition-colors text-left group"
              >
                <div
                  className={`w-9 h-9 rounded-lg bg-gradient-to-br ${CARD_GRADIENTS[i % CARD_GRADIENTS.length]} flex items-center justify-center shrink-0`}
                >
                  <CategoryIcon
                    category={cat}
                    className="text-slate-300 group-hover:text-white transition-colors"
                    size={18}
                    strokeWidth={1.8}
                  />
                </div>
                <div>
                  <span className="text-[13px] font-medium text-slate-200 group-hover:text-white transition-colors">
                    {cat}
                  </span>
                  <span className="text-[11px] text-slate-600 ml-2">
                    {categoryCounts[cat] || 0}
                  </span>
                </div>
              </button>
            ))}
          </div>
        </section>

        {/* Recent section */}
        {recent.length > 0 && (
          <section>
            <h2 className="text-[13px] font-semibold text-slate-300 mb-3">最近使用</h2>
            <div className="grid grid-cols-2 gap-3">
              {recent.map((p) => (
                <button
                  key={p.id}
                  onClick={() => onSelectPrompt(p.id)}
                  className="flex flex-col gap-1.5 p-4 rounded-xl border border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.04] hover:border-white/[0.1] transition-colors text-left"
                >
                  <span className="text-[13px] font-medium text-slate-200 truncate">
                    {p.title || "未命名"}
                  </span>
                  <div className="flex items-center gap-2">
                    {p.category && (
                      <span className="text-[11px] text-purple-400 bg-purple-500/10 px-1.5 py-0.5 rounded">
                        {p.category}
                      </span>
                    )}
                    <span className="text-[11px] text-slate-600">
                      {formatDate(p.updated_at)}
                    </span>
                  </div>
                  <p className="text-[12px] text-slate-500 line-clamp-2 mt-0.5">
                    {p.content.slice(0, 100)}
                  </p>
                </button>
              ))}
            </div>
          </section>
        )}

        {/* Empty state */}
        {prompts.length === 0 && (
          <section>
            <div className="flex flex-col items-center justify-center py-20 gap-3">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="40"
                height="40"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="text-slate-700"
              >
                <circle cx="12" cy="12" r="10" />
                <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                <line x1="2" y1="12" x2="22" y2="12" />
              </svg>
              <p className="text-[13px] text-slate-600">暂无 Prompt</p>
              <p className="text-[12px] text-slate-700">
                创建第一个 Prompt 开始探索
              </p>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
