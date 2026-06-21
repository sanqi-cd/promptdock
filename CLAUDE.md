# PromptDock

Mac 本地 AI 提示词管理工具，Tauri v2 + React + SQLite。

## 技术栈

| 模块 | 技术 |
|------|------|
| 桌面框架 | Tauri v2 |
| 前端 | React 19 + TypeScript + Vite |
| UI | Tailwind CSS v4 + shadcn/ui |
| 数据库 | SQLite (rusqlite, bundled) |
| 搜索 | SQLite FTS5 |
| 状态管理 | Zustand |
| HTTP | reqwest (AI API 调用) |
| 密钥存储 | macOS Keychain (security-framework) |

## 项目结构

```
src/                     React 前端
  components/
    ui/                  shadcn/ui 基础组件
    PromptEditor.tsx     编辑器主组件
    PromptListView.tsx   列表 + 搜索
    PolishPanel.tsx      AI 润色面板
    SettingsPage.tsx     API 配置
    CategoryTags.tsx     分类标签展示
  hooks/
    useAutoSave.ts       3s debounce 自动保存
    useDebounce.ts       通用 debounce
  stores/
    promptStore.ts       Prompt 状态 (Zustand)
    settingsStore.ts     API 设置状态
  lib/utils.ts           cn() 工具函数
  types.ts               TypeScript 类型

src-tauri/               Rust 后端
  src/
    lib.rs               Tauri 入口、Command 注册
    db.rs                数据库初始化、FTS5
    models.rs            数据结构
    commands.rs          所有 CRUD + 搜索 Commands
    ai.rs                AI 润色、分类、连接测试
    keychain.rs          macOS Keychain 存取
```

## 开发命令

```bash
# 开发模式
pnpm tauri dev

# 仅前端
pnpm dev

# 类型检查
pnpm tsc --noEmit

# Rust 构建
cargo build

# Rust 测试
cargo test

# 生产构建
pnpm tauri build
```

## 数据流

```
React UI → Zustand Store → invoke() → Tauri Command → rusqlite → SQLite
                                     ↘ reqwest → AI API → macOS Keychain
```

## Tauri Commands

| Command | 功能 |
|---------|------|
| create_prompt | 新建 Prompt |
| update_prompt | 更新 Prompt |
| delete_prompt | 删除 Prompt |
| get_prompt | 获取详情 |
| list_prompts | 列表（支持分类/收藏过滤） |
| toggle_favorite | 切换收藏 |
| search_prompts | FTS5 + LIKE 搜索 |
| get_prompt_tags / set_prompt_tags | 标签管理 |
| update_prompt_meta | 更新元信息（分类、摘要） |
| polish_prompt | AI 润色 |
| auto_classify | AI 自动分类 |
| test_connection | 测试 API 连接 |
| list_api_configs / get_active_config | API 配置查询 |
| save_api_config / update_api_config / delete_api_config / activate_api_config | API 配置管理 |
