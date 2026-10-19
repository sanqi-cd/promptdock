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
| API Key 存储 | SQLite `api_configs.api_key` 明文列（已知取舍，见下方「安全现状」） |

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
  lib/
    utils.ts             cn() 工具函数
    latestOnly.ts        异步「最新者胜」守卫（防响应乱序覆盖）
    tauriRuntime.ts      是否运行在 Tauri 运行时
  types.ts               TypeScript 类型

src-tauri/               Rust 后端
  src/
    lib.rs               Tauri 入口、Command 注册
    db.rs                数据库初始化、FTS5
    models.rs            数据结构
    commands.rs          Prompt CRUD、标签、自动分类 Commands
    search.rs            FTS5 + LIKE 双路径搜索（分词规则见文件头注释）
    ai.rs                AI 润色、分类、连接测试、SSE 流式解析
    api_configs.rs       API 配置 CRUD + 激活切换 + 输入校验
    polish_prompts.rs    各能力的系统提示词（纯文本内容）
```

异步读取一律用 `src/lib/latestOnly.ts` 的守卫包裹：`loadPrompts` / `loadPrompt` /
`search` / `polishPromptStream` 各自持有独立的守卫，避免用同一个把在途请求误判为过期。
`newPrompt` 也会 bump `beginPromptLoad` 作废在途加载，否则慢响应会把「新建」的编辑器
写回旧 Prompt。

`savePrompt` 通过模块级 `saveQueue` 串行执行：自动保存（3s 防抖）与 ⌘S 并发时两个
请求都会读到 `currentId === null`，串行化后第二个保存走 update 而不是重复 create。
`savedSnapshot` 记录最近一次落库（或加载）的标题/正文，`isDirty()` 据此判断「新建」
前是否需要弹丢弃确认。

自动保存的 timer 挂在 `App` 的 `<AutoSave />` 上，不在编辑器里：切到列表/设置会卸载
编辑器，组件级 timer 会被 cleanup 清掉，最后几秒的编辑就没人保存了。保存前必须检查
`isDirty()`，否则「仅打开」一条 Prompt 也会在 3 秒后改写 `updated_at`。

`editSession` 是编辑目标的代数，`newPrompt` 和**成功加载**时递增。保存队列在入队时
记录代数，执行和写回前都比对：A 的保存晚于切到 B 返回时，写入 A 记录是对的，但绝不能
把 A 的 title/savedSnapshot 写回 B 的状态，否则下一次自动保存会覆盖错记录。润色面板
也用它清空上一条 Prompt 的结果。

`loadPrompt` 在切换前会 `await savePrompt()` flush 未到防抖的编辑；保存失败则停在
当前 Prompt 并显示 `saveError`。`saveError` 与读取错误 `error` 是两个槽位，自动保存
失败后会按指数退避重试（3s → 30s），成功后自动清除。

弹窗打开时（`data-modal-open="true"`，见 `src/lib/overlay.ts`）全局快捷键一律禁用：
否则搜索框里的 ⌘↵ 会发起一次真实计费的润色调用。

时间戳一律走 `src/lib/formatDate.ts`，不要直接 `new Date(后端字段)`：后端存的是
`"2026-06-21 19:36:00"`（非 ISO、无时区），解析行为属于实现定义；而且 `conn_now()`
在查询失败时会写入字面量 `"unknown"`，直接 `new Date()` 会渲染出 "Invalid Date"。

标签长度按**字符**计（`TAG_MAX_CHARS`），不是字节 —— 按字节会把 6 个字以上的中文
标签静默丢弃。自动分类结果必须走 `apply_classification` 单命令写入：曾被拆成
「写标签 + 整体替换标签」两步，后者会把前者刚写的推荐工具标签删掉。

## 安全现状（重要，勿按错误假设改代码）

**API Key 目前是明文存储在本地 SQLite 里**，没有使用 macOS Keychain。
仓库中不存在 `keychain.rs`，`Cargo.toml` 也没有 `security-framework` / `keyring` 依赖。

- 落库位置：`~/Library/Application Support/com.promptdock.desktop/promptdock.db`
- 列为 `api_configs.api_key`；WAL 模式下明文同样会残留在 `.db-wal` / `.db-shm`，删除配置不等于擦除
- 数据库目录 0700、db/wal/shm 文件 0600（`db.rs::restrict_db_permissions`）
- Key 只会发送到用户自己配置的 `base_url`；`validate_base_url` 用 URL 解析而非前缀匹配，
  拒绝 userinfo（`https://api.openai.com@evil.example`）、query、fragment
- 修改已有配置时，若 `base_url` 的 scheme/host/port 变化且 Key 留空，更新会被拒绝，
  必须重新输入 Key（`same_origin`）；避免旧 Key 被静默转发到新主机
- HTTP 客户端禁用重定向（`redirect::Policy::none()`）：reqwest 跨主机重定向只剥离
  `Authorization`，不会剥离 Anthropic 的自定义 `x-api-key`
- 流式请求使用不带总超时的独立客户端；总超时会把超过 300 秒的长流从中间掐断
- 之所以暂不上 Keychain：当前是 ad-hoc 签名，每次重新编译签名标识都会变，Keychain ACL 随之失效，开发期每次构建都要重新授权；且 ad-hoc 签名对本地已提权攻击者防护有限。计划在拿到 Apple Developer ID 签名后再引入 `keyring`

改动这块时请同步更新 `README.md` 的安全说明。

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

# Rust 格式检查（CI 会跑）
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

# 依赖审计（CI 会跑）
pnpm audit --audit-level high
cargo audit --file src-tauri/Cargo.lock

# 生产构建
pnpm tauri build
```

## 数据流

```
React UI → Zustand Store → invoke() → Tauri Command → rusqlite → SQLite
                                     ↘ reqwest → AI API
                                       (Key 从 api_configs 明文列读取)
```

## Tauri Commands

| Command | 功能 |
|---------|------|
| create_prompt | 新建 Prompt |
| update_prompt | 更新 Prompt |
| delete_prompt | 删除 Prompt |
| get_prompt | 获取详情 |
| list_prompts | 列表（支持分类/收藏过滤） |
| record_prompt_use | 记录使用时间（驱动「最近使用」） |
| toggle_favorite | 切换收藏 |
| search_prompts | FTS5 + LIKE 搜索（含 CJK 特殊处理，见下） |
| get_prompt_tags / set_prompt_tags | 标签管理 |
| apply_classification | 一次性写入自动分类结果（分类 + 摘要 + 标签，含推荐工具），单事务 |
| polish_prompt | AI 润色 |
| polish_prompt_stream | AI 润色（流式，Channel 推送 delta），参数含 `requestId` 用于取消 |
| cancel_polish_stream | 按 `requestId` 取消在途的流式润色（幂等，不存在的 id 静默成功） |
| analyze_polish_process | 生成「处理要点」展示（会额外消耗一次模型调用） |
| auto_classify | AI 自动分类 |
| test_connection | 测试 API 连接 |
| list_api_configs / get_active_config | API 配置查询 |
| save_api_config / update_api_config / delete_api_config / activate_api_config | API 配置管理 |

## 搜索实现的两条约束

改动 `commands.rs` 的搜索逻辑前请先读这里：

1. **FTS5 的 `unicode61` 分词器把一整段连续中文当作单个 token**，所以 `"词"*` 前缀查询
   只能匹配该段的开头：搜「视频」匹配不到「...的视频脚本」。因此含 CJK 的查询会
   **直接走 LIKE**，纯 ASCII 才走 FTS5。
2. **LIKE 兜底只在 FTS 返回空时触发**。若 FTS 返回了部分结果，兜底不会执行，用户会拿到
   一份不完整却无提示的结果 —— 这正是 CJK 必须绕过 FTS 的原因，别把这条路由删掉。

`like_search` 用 `ESCAPE '\'` 转义 `%` `_` `\`；不转义的话搜一个 `%` 会返回整张表。
多词查询按空格切分后是 **AND** 语义，与 FTS 路径保持一致。

代价：CJK 查询是全表 `LIKE` 扫描。几千条 Prompt 量级下是毫秒级，可接受；若数据量涨到
数万条再考虑给中文做 bigram 预处理（**不要**改回 FTS5 前缀查询，也不要换 `trigram`
分词器 —— 它有 3 字符下限，中文双字词会全部搜不到）。
