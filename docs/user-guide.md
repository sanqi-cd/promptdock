# PromptDock 使用说明

PromptDock 是一款运行在 macOS 上的本地提示词管理工具，支持 Prompt 保存、分类、搜索、收藏、AI 润色和自动分类。

## 1. 安装应用

安装包位于项目的 `release/` 目录：

```text
release/PromptDock_0.1.1_universal_adhoc.dmg
```

双击打开 DMG 文件。

![打开安装包](images/install-open-dmg.png)

将 `PromptDock.app` 拖入 `Applications`。

![拖入 Applications](images/install-drag-applications.png)

## 2. 处理 macOS 安全提示

当前版本使用 ad-hoc 签名，尚未使用 Apple Developer ID 签名和 notarization。因此第一次打开时，macOS 可能会提示无法验证开发者。

如果出现安全提示，先选择完成或关闭提示。

![macOS 安全提示](images/gatekeeper-error.png)

然后进入系统设置，打开隐私与安全性。

![隐私与安全性](images/security-settings.png)

滑到页面下方，点击仍要打开。

![仍要打开](images/security-open-anyway.png)

macOS 可能会要求输入电脑密码。完成后即可正常打开 PromptDock。

## 3. 切换主题

PromptDock 提供两种主题：

- 暖纸色主题：适合长时间整理和阅读 Prompt。
- 深色主题：适合暗光环境或偏沉浸式工作。

![暖纸色主题](images/theme-light.png)

![深色主题](images/theme-dark.png)

## 4. 配置模型 API

点击左侧设置，进入模型配置页面，添加你的大模型 API Key。

![添加模型配置](images/settings-add-model.png)

PromptDock 内置了几个常见模型服务商的配置模板。选择对应服务商后，只需要填写 API Key 和模型名称即可。

![内置模型配置](images/settings-presets.png)

如果你使用的是其他 OpenAI-compatible 接口，可以选择其他，并填写：

- API URL
- API Key
- 模型名称

下面以小米 Mimo 为例。

![自定义模型配置](images/settings-custom-provider.png)

点击测试连接。如果显示连接成功，说明配置可用。

![连接成功](images/settings-connection-success.png)

保存后，所有模型配置会在列表中展示。你可以添加多个模型，并按需切换当前使用的模型。

![模型配置列表](images/settings-model-list.png)

## 5. 编写和管理 Prompt

回到工作台后，可以在编辑器里编写 Prompt。PromptDock 会帮助你保存常用提示词，方便后续搜索、分类和复用。

![工作台编辑器](images/workbench-editor.png)

编辑器支持基础 Markdown 格式，也可以保存、复制、收藏和删除 Prompt。

编辑内容会在停止输入 3 秒后自动保存。切换到其他 Prompt 前会先把未落库的修改保存；如果保存失败，顶部会显示错误提示，并按退避策略自动重试。有未保存内容时点击「新建 Prompt」会先询问是否丢弃。

如果列表、详情或搜索读取失败，顶部会显示可关闭的错误横幅，而不是把失败伪装成「没有数据」。

## 6. 使用 AI 润色

PromptDock 提供四种 AI 润色模式，适合不同场景。

### 优化

适合日常表达优化。它会在保留原始意图的基础上，让 Prompt 更清晰、更完整、更容易被模型理解。

![优化模式](images/polish-optimize.png)

### 结构化

适合把模糊、零散的想法整理成结构清晰的 Markdown Prompt，例如角色、任务、输入、输出和约束。

![结构化模式](images/polish-structure.png)

### 精简

适合压缩冗余内容，去掉重复表达，让 Prompt 更短、更聚焦。

![精简模式](images/polish-concise.png)

### 专家增强

适合复杂任务场景。它会补充角色定位、执行步骤、约束条件和质量标准，让 Prompt 更像一份完整任务说明。

![专家增强模式](images/polish-expert.png)

点击「开始润色」或按 `⌘↵` 开始。润色过程中可以随时点击「取消」中止请求；取消后不会保存任何部分结果。

润色完成后，可以点击替换原文，将编辑器中的内容替换为润色结果，再点击保存。

切换到另一条 Prompt 时，上一条的润色结果会自动清空，避免把结果误写进新的 Prompt。搜索、删除确认等弹窗打开时，全局快捷键不会生效。

## 7. 自动分类

Prompt 保存后，可以点击右侧的自动分类，让 AI 根据内容为 Prompt 匹配合适分类。

![自动分类](images/auto-category.png)

左侧分类区域也支持点击 `+` 手动新建分类。分类创建后，PromptDock 会根据分类名称自动匹配图标。

点击某个分类，可以查看该领域下的 Prompt。

![分类列表](images/category-list.png)

也可以在全部 Prompt 中查看和搜索所有提示词。

![全部 Prompt](images/all-prompts.png)

## 8. 探索页面

探索页面用于按分类浏览 Prompt，适合快速了解当前沉淀了哪些类型的提示词。

![探索页面](images/explore.png)

## 9. 使用建议

- 把常用 Prompt 保存到 PromptDock 中，避免散落在聊天记录或文档里。
- 对简单想法优先使用优化或结构化。
- 对过长 Prompt 使用精简。
- 对复杂任务、工作流或需要稳定输出质量的 Prompt 使用专家增强。
- API Key 建议使用单独创建的低额度 Key，便于控制风险。
