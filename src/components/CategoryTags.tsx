interface CategoryTagsProps {
  category: string | null;
  tags: string[];
}

export const CATEGORIES = [
  "内容创作", "代码开发", "学习研究", "销售营销",
  "数据分析", "图像生成", "工作汇报", "产品设计",
  "视频脚本", "其他",
] as const;

export function CategoryTags({ category, tags }: CategoryTagsProps) {
  return (
    <div className="flex flex-wrap items-center gap-1">
      {category && (
        <span className="inline-flex items-center rounded bg-primary/10 text-primary px-2 py-0.5 text-[10px] font-medium">
          {category}
        </span>
      )}
      {tags.map((tag) => (
        <span
          key={tag}
          className="inline-flex items-center rounded border bg-background px-2 py-0.5 text-[10px] text-muted-foreground"
        >
          {tag}
        </span>
      ))}
    </div>
  );
}
