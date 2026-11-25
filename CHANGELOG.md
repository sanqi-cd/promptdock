# 更新记录

## 0.1.3

- 定向升级 `cookie` 和 `url`，修复依赖安全公告 RUSTSEC-2026-0594 和 RUSTSEC-2026-0592。
- 发布前重新执行同一提交的前端测试、Rust 测试和依赖安全审计；检查失败时不会发布安装包。
- 增加 Dependabot，每周为 Rust、前端和 GitHub Actions 创建依赖更新 PR，需人工审核合并。
- 保留原有数据目录和数据库结构，支持从 0.1.2 覆盖安装。

macOS universal 安装包同时包含 Apple Silicon 和 Intel 架构。未配置 Apple 凭据时使用临时签名，未经 Apple 公证，首次打开仍可能出现系统安全提示。
