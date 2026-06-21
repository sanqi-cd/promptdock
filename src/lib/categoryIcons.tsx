import type { LucideIcon } from "lucide-react";
import {
  Activity,
  BookOpen,
  Briefcase,
  ChartNoAxesColumn,
  CircleHelp,
  Code2,
  Compass,
  Cpu,
  Database,
  FileText,
  FolderKanban,
  Globe2,
  GraduationCap,
  HeartHandshake,
  Image,
  Landmark,
  LayoutTemplate,
  Megaphone,
  MessageSquareText,
  Palette,
  PenLine,
  Plane,
  Presentation,
  Scale,
  ShoppingCart,
  Sparkles,
  Tags,
  Target,
  Video,
  WalletCards,
  Wrench,
} from "lucide-react";

export interface CategoryIconMeta {
  id: string;
  Icon: LucideIcon;
}

const EXACT_CATEGORY_ICONS: Record<string, CategoryIconMeta> = {
  内容创作: { id: "pen-line", Icon: PenLine },
  代码开发: { id: "code", Icon: Code2 },
  学习研究: { id: "book-open", Icon: BookOpen },
  销售营销: { id: "megaphone", Icon: Megaphone },
  数据分析: { id: "chart", Icon: ChartNoAxesColumn },
  图像生成: { id: "image", Icon: Image },
  工作汇报: { id: "presentation", Icon: Presentation },
  产品设计: { id: "layout-template", Icon: LayoutTemplate },
  视频脚本: { id: "video", Icon: Video },
  其他: { id: "circle-help", Icon: CircleHelp },
};

const KEYWORD_CATEGORY_ICONS: Array<{
  id: string;
  Icon: LucideIcon;
  keywords: string[];
}> = [
  { id: "briefcase", Icon: Briefcase, keywords: ["工作", "办公", "职场", "任务", "项目", "管理", "会议"] },
  { id: "pen-line", Icon: PenLine, keywords: ["写作", "文案", "文章", "内容", "创作", "公众号", "博客"] },
  { id: "code", Icon: Code2, keywords: ["代码", "开发", "编程", "程序", "前端", "后端", "工程"] },
  { id: "book-open", Icon: BookOpen, keywords: ["学习", "研究", "读书", "课程", "知识", "论文", "资料"] },
  { id: "chart", Icon: ChartNoAxesColumn, keywords: ["数据", "分析", "统计", "报表", "指标", "增长"] },
  { id: "image", Icon: Image, keywords: ["图片", "图像", "绘画", "海报", "视觉", "摄影"] },
  { id: "megaphone", Icon: Megaphone, keywords: ["营销", "销售", "增长", "推广", "广告", "品牌"] },
  { id: "video", Icon: Video, keywords: ["视频", "脚本", "短视频", "剪辑", "分镜"] },
  { id: "palette", Icon: Palette, keywords: ["设计", "产品", "界面", "UI", "UX", "原型"] },
  { id: "wallet", Icon: WalletCards, keywords: ["财务", "预算", "账单", "收入", "成本", "钱"] },
  { id: "scale", Icon: Scale, keywords: ["法律", "合同", "合规", "条款", "协议"] },
  { id: "plane", Icon: Plane, keywords: ["旅行", "出差", "行程", "路线", "旅游"] },
  { id: "heart-handshake", Icon: HeartHandshake, keywords: ["客户", "服务", "关系", "合作", "社群"] },
  { id: "database", Icon: Database, keywords: ["数据库", "知识库", "素材库", "归档", "存储"] },
  { id: "globe", Icon: Globe2, keywords: ["国际", "海外", "语言", "翻译", "全球"] },
  { id: "shopping-cart", Icon: ShoppingCart, keywords: ["电商", "商品", "店铺", "订单", "零售"] },
  { id: "message", Icon: MessageSquareText, keywords: ["沟通", "对话", "客服", "消息", "聊天"] },
  { id: "cpu", Icon: Cpu, keywords: ["AI", "人工智能", "模型", "智能", "自动化"] },
];

const FALLBACK_ICON_POOL: CategoryIconMeta[] = [
  { id: "target", Icon: Target },
  { id: "tags", Icon: Tags },
  { id: "sparkles", Icon: Sparkles },
  { id: "wrench", Icon: Wrench },
  { id: "file-text", Icon: FileText },
  { id: "folder-kanban", Icon: FolderKanban },
  { id: "activity", Icon: Activity },
  { id: "graduation-cap", Icon: GraduationCap },
  { id: "landmark", Icon: Landmark },
  { id: "compass", Icon: Compass },
];

export function getCategoryIconMeta(category: string): CategoryIconMeta {
  const normalized = normalizeCategory(category);
  if (EXACT_CATEGORY_ICONS[normalized]) {
    return EXACT_CATEGORY_ICONS[normalized];
  }

  const keywordMatch = KEYWORD_CATEGORY_ICONS.find((item) =>
    item.keywords.some((keyword) => normalized.includes(keyword.toLowerCase()))
  );
  if (keywordMatch) {
    return { id: keywordMatch.id, Icon: keywordMatch.Icon };
  }

  return FALLBACK_ICON_POOL[hashCategory(normalized) % FALLBACK_ICON_POOL.length];
}

export function CategoryIcon({
  category,
  className,
  size = 15,
  strokeWidth = 1.6,
}: {
  category: string;
  className?: string;
  size?: number;
  strokeWidth?: number;
}) {
  const { id, Icon } = getCategoryIconMeta(category);
  return (
    <Icon
      aria-hidden="true"
      className={className}
      data-category-icon={id}
      size={size}
      strokeWidth={strokeWidth}
    />
  );
}

function normalizeCategory(category: string): string {
  return category.trim().toLowerCase();
}

function hashCategory(category: string): number {
  let hash = 0;
  for (let i = 0; i < category.length; i += 1) {
    hash = (hash * 31 + category.charCodeAt(i)) >>> 0;
  }
  return hash;
}
