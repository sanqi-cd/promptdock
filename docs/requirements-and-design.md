# PromptDock V1 需求与设计文档

**版本**: V1.0 | **更新**: 2026-06-11 | **原型**: Pencil 17 帧

---

## 一、产品概述

PromptDock 是一个 Mac 本地 AI 提示词管理工具（Tauri v2 + React + SQLite），采用暗色主题三栏布局，帮助用户编写、优化、分类、搜索和复用 Prompt。

## 二、设计系统

| 维度 | 选择 |
|------|------|
| 主题 | Dark — `#0D1117` 渐变背景 |
| 主色 | `#7C3AED → #3B82F6` 紫蓝渐变 |
| 表面 | `#111827AA` 玻璃卡片 + `#FFFFFF0D` 描边 |
| 字体 | Inter, 11-22px |
| 圆角 | 8-14px |
| 图标 | Lucide |

## 三、信息架构

```
PromptDock
├── 侧边栏 (270px, 始终可见)
│   ├── Logo + 新建按钮
│   ├── 导航 (5项): 工作台 / 探索 / 收藏 / 最近使用 / 全部 Prompt
│   ├── 分类列表 (7类 + 更多)
│   └── 设置入口 (底部固定)
│
├── 主内容区 (flex-1, 动态切换)
│   ├── 工作台 → PromptEditor (编辑器 + AI润色区)
│   ├── 探索 → ExploreView (分类卡片网格 + 最近使用)
│   ├── 收藏/全部Prompt/分类筛选 → PromptListView (卡片列表)
│   ├── Prompt详情 → PromptDetailView
│   └── 设置 → SettingsView (API配置 + 应用设置)
│
└── 右侧面板 (300px, 编辑器/列表模式下可见)
    ├── Prompt 信息卡片
    ├── 快捷操作
    └── 提示 / 统计
```

## 四、页面与导航状态矩阵

| 页面 | 导航激活 | 主内容 | 右侧面板 | 原型 |
|------|---------|--------|---------|------|
| 工作台 | 工作台 | 编辑器 + AI润色 | 信息+操作+提示 | zLYeL |
| 探索 | 探索 | 分类卡片 + 最近 | 统计+提示 | Wh4CM |
| 全部 Prompt | 全部 Prompt | 卡片列表 | 统计+提示 | D9Dry |
| 收藏 | 收藏 | 筛选列表 | 统计+提示 | I7Cy1 |
| 分类筛选 | (无) | 筛选列表 | 统计+提示 | EPyTL |
| Prompt详情 | 全部 Prompt | 详情内容 | (隐藏) | TVShN |
| API设置 | (无,底部高亮) | API配置 | (隐藏) | gglXi |
| AI润色对比 | — | 左右对比 | (隐藏) | O80ZUA |

## 五、核心交互流程

### 5.1 Prompt 编辑流程
```
新建 / 点击列表项 → 编辑器加载 → 输入内容 → 自动保存(3s debounce)
→ [AI润色] → 调用模型API → 润色结果(对比/替换/复制)
→ [自动分类] → 调用模型API → 设置分类/标签/摘要
→ 保存 → 列表刷新
```

### 5.2 探索浏览流程
```
点击"探索" → 分类卡片网格 → 点击分类 → 跳转分类筛选
→ 最近使用 → 点击 → 加载到编辑器
```

### 5.3 搜索流程
```
⌘K → 全局搜索弹窗 → 实时搜索 → 选择结果 → 加载到编辑器
侧边栏搜索 → 筛选列表
```

### 5.4 API 配置流程
```
侧边栏底部"设置" → API配置页 → 查看/编辑/删除配置
→ 添加新配置 → 填写Provider/URL/Key/Model → 测试连接 → 保存
```

## 六、组件树

```
App
├── Sidebar
│   ├── LogoRow
│   ├── NewPromptBtn
│   ├── NavGroup (5 items)
│   ├── CategoryHeader
│   ├── CategoryList (7 + "更多")
│   ├── SettingsSpacer
│   └── SettingsLink
│
├── MainArea (动态路由)
│   ├── PromptEditor (工作台)
│   │   ├── Toolbar (搜索框 + 模型选择)
│   │   ├── EditorCard (标题 + 内容 + 格式栏)
│   │   ├── PolishSection (AI润色)
│   │   └── ActionBar (保存/复制/删除)
│   │
│   ├── ExploreView (探索)
│   │   ├── ExploreHeader
│   │   ├── CategoryGrid (2×3 卡片)
│   │   └── RecentSection
│   │
│   ├── PromptListView (全部/收藏/分类)
│   │   ├── SearchBar + FilterChip
│   │   └── CardList
│   │
│   ├── PromptDetailView (详情)
│   │   ├── Breadcrumb + TitleRow
│   │   ├── ContentCard
│   │   ├── AICard (AI润色结果)
│   │   └── MetaCard
│   │
│   └── SettingsView (设置)
│       ├── APIConfigCards
│       └── AppSettingsSections
│
├── RightPanel (条件渲染)
│   ├── StatsCard / InfoCard
│   ├── ActionsCard
│   └── TipsCard
│
├── GlobalSearchModal (⌘K)
├── DeleteConfirmModal
├── ToastContainer
└── APITestStatusOverlay
```

## 七、数据流

```
React UI → Zustand Store → invoke() → Tauri Command → rusqlite → SQLite
                                     ↘ reqwest → AI API → macOS Keychain
```

Zustand Stores (不变):
- `promptStore` — Prompt CRUD, 搜索, 分类, 润色
- `settingsStore` — API 配置 CRUD, 激活切换

## 八、文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `src/App.tsx` | **重写** | 三栏布局，主视图路由 |
| `src/components/Sidebar.tsx` | **重写** | 5项导航，移除用户区，设置入口 |
| `src/components/PromptEditor.tsx` | 修改 | 适配新布局 |
| `src/components/RightPanel.tsx` | 修改 | 上下文感知内容 |
| `src/components/SettingsPage.tsx` | 修改 | 嵌入主区域，移除 onBack |
| `src/components/ExploreView.tsx` | **新建** | 探索页面 |
| `src/components/PromptListView.tsx` | **新建** | 统一列表视图 |
| `src/components/PromptDetailView.tsx` | **新建** | Prompt 详情 |
| `src/components/GlobalSearchModal.tsx` | **新建** | ⌘K 搜索弹窗 |
| `src/components/DeleteConfirmModal.tsx` | **新建** | 删除确认弹窗 |
| `src/index.css` | 不变 | 暗色主题已匹配 |
| `src/stores/*` | 不变 | 状态逻辑不变 |
| `src-tauri/*` | 不变 | Rust 后端不动 |

## 九、不做的功能 (V1)

云同步、多端同步、Prompt社区、分享、工作流编排、自动粘贴、插件系统、AI Agent、OCR、浏览器插件
