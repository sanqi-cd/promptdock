import { test, expect, type Page } from "@playwright/test";

const BASE = "http://localhost:1420";

type Scenario =
  | "lists"
  | "load-flush"
  | "save-snapshot"
  | "favorite"
  | "classify"
  | "fallback"
  | "replace"
  | "backup"
  | "backup-error";

type Call = { cmd: string; args?: Record<string, unknown> };

/** 安装可手动释放的 Tauri mock；各用例只选择场景，不重复运行环境样板。 */
async function installRaceMock(page: Page, scenario: Scenario) {
  await page.addInitScript((scenario: Scenario) => {
    const calls: Call[] = [];
    const pending = new Map<string, () => void>();
    const callbacks = new Map<number, (...args: unknown[]) => unknown>();
    let callbackId = 0;
    const testWindow = window as unknown as {
      __raceCalls: Call[];
      __raceRelease: (cmd: string) => void;
      __TAURI_INTERNALS__: unknown;
    };
    testWindow.__raceCalls = calls;
    testWindow.__raceRelease = (cmd) => {
      pending.get(cmd)?.();
      pending.delete(cmd);
    };
    const waitForRelease = (cmd: string) =>
      new Promise<void>((resolve) => pending.set(cmd, resolve));

    const promptA = {
      id: "prompt-a",
      title: "A 标题",
      content: "A 正文",
      optimized_content: null,
      category: null,
      summary: null,
      is_favorite: false,
      created_at: "2026-01-01 00:00:00",
      updated_at: "2026-01-01 00:00:00",
      last_used_at: null,
    };
    const promptB = { ...promptA, id: "prompt-b", title: "B 标题", content: "B 正文" };
    const normalSummary = {
      id: "normal",
      title: "普通项",
      content_preview: "普通摘要预览",
      category: "内容创作",
      summary: null,
      is_favorite: false,
      created_at: "2026-01-01 00:00:00",
      updated_at: "2026-01-02 00:00:00",
      last_used_at: "2026-01-02 00:00:00",
    };
    const favoriteSummary = {
      ...normalSummary,
      id: "favorite",
      title: "收藏项",
      content_preview: "收藏摘要预览",
      category: "代码开发",
      is_favorite: true,
    };
    const toSummary = (prompt: typeof promptA) => ({
      id: prompt.id,
      title: prompt.title,
      content_preview: prompt.content,
      category: prompt.category,
      summary: prompt.summary,
      is_favorite: prompt.is_favorite,
      created_at: prompt.created_at,
      updated_at: prompt.updated_at,
      last_used_at: prompt.last_used_at,
    });

    testWindow.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        calls.push({ cmd, args });
        if (cmd === "list_api_configs") {
          return [
            {
              id: "cfg",
              name: "测试配置",
              provider: "openai",
              base_url: "https://example.test",
              model: "test-model",
              is_active: true,
              created_at: "2026-01-01 00:00:00",
            },
          ];
        }
        if (cmd === "list_prompts") {
          if (scenario === "lists") {
            let items = [normalSummary, favoriteSummary];
            if (args?.favoriteOnly === true) {
              items = items.filter((item) => item.is_favorite);
            }
            if (typeof args?.category === "string") {
              items = items.filter((item) => item.category === args.category);
            }
            return items;
          }
          if (scenario === "load-flush") return [toSummary(promptB)];
          if (scenario === "favorite") return [toSummary(promptA)];
          if (scenario === "classify") return [toSummary(promptA), toSummary(promptB)];
          return [];
        }
        if (cmd === "get_prompt") {
          if (scenario === "load-flush" || args?.id === promptB.id) return promptB;
          return promptA;
        }
        if (cmd === "get_prompt_tags") return [];
        if (cmd === "record_prompt_use") return null;
        if (cmd === "create_prompt") {
          if (scenario === "load-flush" || scenario === "save-snapshot") {
            await waitForRelease(cmd);
          }
          return {
            ...promptA,
            id: "created-prompt",
            title: args?.title ?? "",
            content: args?.content ?? "",
            optimized_content: args?.optimizedContent ?? null,
          };
        }
        if (cmd === "update_prompt") return null;
        if (cmd === "toggle_favorite") {
          await waitForRelease(cmd);
          return true;
        }
        if (cmd === "auto_classify") {
          await waitForRelease(cmd);
          return {
            category: "旧分类",
            tags: ["旧标签"],
            summary: "旧摘要",
            recommended_tool: "旧工具",
          };
        }
        if (cmd === "apply_classification") {
          return [{ id: "tag-old", name: "旧标签" }];
        }
        if (cmd === "polish_prompt_stream") {
          if (scenario === "fallback") {
            await waitForRelease(cmd);
            throw "command polish_prompt_stream not found";
          }
          return null;
        }
        if (cmd === "polish_prompt") return "优化后的正文";
        if (cmd === "analyze_polish_process") return [];
        if (cmd === "backup_database") {
          if (scenario === "backup-error") throw "disk full";
          await waitForRelease(cmd);
          return "/Users/test/Library/Application Support/PromptDock/backups/promptdock-2026-11-11.db";
        }
        return null;
      },
      transformCallback: (callback: (...args: unknown[]) => unknown) => {
        callbackId += 1;
        callbacks.set(callbackId, callback);
        return callbackId;
      },
      unregisterCallback: (id: number) => callbacks.delete(id),
      runCallback: (id: number, data: unknown) => callbacks.get(id)?.(data),
    };
  }, scenario);
}

async function release(page: Page, cmd: string) {
  await page.evaluate((cmd) => {
    (window as unknown as { __raceRelease: (cmd: string) => void }).__raceRelease(cmd);
  }, cmd);
}

async function commandCalls(page: Page, cmd: string): Promise<Call[]> {
  return page.evaluate((cmd) => {
    const calls = (window as unknown as { __raceCalls: Call[] }).__raceCalls;
    return calls.filter((call) => call.cmd === cmd);
  }, cmd);
}

async function waitForCall(page: Page, cmd: string) {
  await expect.poll(async () => (await commandCalls(page, cmd)).length).toBeGreaterThan(0);
}

async function openPrompt(page: Page, title: string) {
  await page.locator("nav button", { hasText: "全部 Prompt" }).click();
  await page.locator("main .rounded-xl button", { hasText: title }).click();
}

test("筛选列表不污染全局统计，探索分类会清除收藏过滤", async ({ page }) => {
  await installRaceMock(page, "lists");
  await page.goto(BASE);

  await page.locator("nav button", { hasText: "收藏" }).click();
  await expect(page.getByText("收藏项", { exact: true })).toBeVisible();
  await expect(page.getByText("收藏摘要预览", { exact: true })).toBeVisible();
  await expect(page.getByText("普通项", { exact: true })).toHaveCount(0);
  const panel = page.locator("aside").last();
  await expect(
    panel.getByText("总数", { exact: true }).locator("..").getByText("2", { exact: true })
  ).toBeVisible();

  await page.locator("nav button", { hasText: "探索" }).click();
  await expect(page.getByText("普通项", { exact: true })).toBeVisible();
  await page.locator("section").first().locator(".grid button", { hasText: "内容创作" }).click();
  await expect(page.getByText("普通项", { exact: true })).toBeVisible();
  await expect(page.getByText("收藏项", { exact: true })).toHaveCount(0);
});

test("dirty flush 期间新建后，旧加载不再请求或覆盖详情", async ({ page }) => {
  await installRaceMock(page, "load-flush");
  await page.goto(BASE);
  const editor = page.locator("textarea").first();
  await editor.fill("A 的未保存内容");

  await openPrompt(page, "B 标题");
  await waitForCall(page, "create_prompt");
  await page.locator("aside button", { hasText: "新建 Prompt" }).click();
  await page.getByRole("button", { name: "丢弃并新建" }).click();
  await editor.fill("新建后的内容");
  await release(page, "create_prompt");
  await page.waitForTimeout(250);

  expect(await commandCalls(page, "get_prompt")).toHaveLength(0);
  await expect(editor).toHaveValue("新建后的内容");
});

test("保存响应保留同会话新输入，落库快照仍触发 dirty", async ({ page }) => {
  await installRaceMock(page, "save-snapshot");
  await page.goto(BASE);
  await page.locator("main").getByText("新建 Prompt", { exact: true }).click();
  const titleInput = page.getByPlaceholder("输入标题");
  const editor = page.locator("textarea").first();
  await titleInput.fill("旧标题");
  await editor.fill("旧正文");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await waitForCall(page, "create_prompt");

  await titleInput.fill("新标题");
  await editor.fill("新正文");
  await release(page, "create_prompt");
  await expect(titleInput).toHaveValue("新标题");
  await expect(editor).toHaveValue("新正文");
  const creates = await commandCalls(page, "create_prompt");
  expect(creates[0].args).toMatchObject({ title: "旧标题", content: "旧正文" });

  await page.locator("aside button", { hasText: "新建 Prompt" }).click();
  await expect(page.getByText("丢弃未保存的修改")).toBeVisible();
  await page.getByRole("button", { name: "继续编辑" }).click();
  await expect(editor).toHaveValue("新正文");
});

test("无 Tauri 时保存明确失败而不是显示假成功", async ({ page }) => {
  await page.goto(BASE);
  await page.locator("textarea").first().fill("无法落库的内容");
  await page.getByRole("button", { name: "保存", exact: true }).click();

  await expect(page.getByRole("alert")).toContainText("desktop runtime is unavailable");
  await expect(page.getByText("已保存", { exact: true })).toHaveCount(0);
});

test("收藏迟到的响应不会修改新建 Prompt", async ({ page }) => {
  await installRaceMock(page, "favorite");
  await page.goto(BASE);
  await openPrompt(page, "A 标题");
  await expect(page.locator("textarea").first()).toHaveValue("A 正文");

  await page.getByRole("button", { name: /收藏 Prompt/ }).click();
  await waitForCall(page, "toggle_favorite");
  await page.locator("aside button", { hasText: "新建 Prompt" }).click();
  await release(page, "toggle_favorite");
  await page.waitForTimeout(250);

  const panel = page.locator("aside").last();
  await expect(panel.getByRole("button", { name: /收藏 Prompt/ })).toBeVisible();
  await expect(panel.getByRole("button", { name: /取消收藏/ })).toHaveCount(0);
});

test("保存等待期间新建后不再发起自动分类", async ({ page }) => {
  await installRaceMock(page, "load-flush");
  await page.goto(BASE);
  await page.locator("textarea").first().fill("尚未保存的正文");
  await page.getByRole("button", { name: "自动分类 →" }).click();
  await waitForCall(page, "create_prompt");

  await page.locator("aside button", { hasText: "新建 Prompt" }).click();
  await page.getByRole("button", { name: "丢弃并新建" }).click();
  await release(page, "create_prompt");

  await expect(page.getByText("编辑目标已切换，已取消自动分类", { exact: true })).toBeVisible();
  expect(await commandCalls(page, "auto_classify")).toHaveLength(0);
});

test("动态分类传完整列表，新建会复位状态并丢弃旧结果", async ({ page }) => {
  await installRaceMock(page, "classify");
  await page.goto(BASE);
  await page.getByRole("button", { name: "添加分类" }).click();
  await page.getByPlaceholder("新分类名称").fill("我的分类");
  await page.getByRole("button", { name: "确认添加分类" }).click();
  await openPrompt(page, "A 标题");
  await expect(page.locator("textarea").first()).toHaveValue("A 正文");

  await page.getByRole("button", { name: "自动分类 →" }).click();
  await waitForCall(page, "auto_classify");
  const classifies = await commandCalls(page, "auto_classify");
  expect(classifies[0].args?.categories).toEqual(
    expect.arrayContaining(["内容创作", "代码开发", "我的分类"])
  );
  expect(new Set(classifies[0].args?.categories as string[]).size).toBe(
    (classifies[0].args?.categories as string[]).length
  );

  await page.locator("aside button", { hasText: "新建 Prompt" }).click();
  await page.locator("textarea").first().fill("新正文");
  await expect(page.getByRole("button", { name: "自动分类 →" })).toBeEnabled();
  await release(page, "auto_classify");

  await expect(page.getByText("编辑目标已切换，已丢弃旧分类结果")).toBeVisible();
  expect(await commandCalls(page, "apply_classification")).toHaveLength(0);
  await expect(page.getByText("旧分类", { exact: true })).toHaveCount(0);
});

test("成功加载其他 Prompt 也会复位并作废在途分类", async ({ page }) => {
  await installRaceMock(page, "classify");
  await page.goto(BASE);
  await openPrompt(page, "A 标题");
  await expect(page.locator("textarea").first()).toHaveValue("A 正文");
  await page.getByRole("button", { name: "自动分类 →" }).click();
  await waitForCall(page, "auto_classify");

  await openPrompt(page, "B 标题");
  await expect(page.locator("textarea").first()).toHaveValue("B 正文");
  await expect(page.getByRole("button", { name: "自动分类 →" })).toBeEnabled();
  await release(page, "auto_classify");

  await expect(page.getByText("编辑目标已切换，已丢弃旧分类结果")).toBeVisible();
  expect(await commandCalls(page, "apply_classification")).toHaveLength(0);
  await expect(page.locator("textarea").first()).toHaveValue("B 正文");
});

test("取消后收到流式命令缺失错误也不会回退重发", async ({ page }) => {
  await installRaceMock(page, "fallback");
  await page.goto(BASE);
  await page.locator("textarea").first().fill("需要润色的原文");
  await page.getByRole("button", { name: /开始润色/ }).click();
  await waitForCall(page, "polish_prompt_stream");

  await page.getByRole("button", { name: "取消", exact: true }).click();
  await release(page, "polish_prompt_stream");
  await page.waitForTimeout(300);
  expect(await commandCalls(page, "polish_prompt")).toHaveLength(0);
});

test("原文继续编辑后替换润色结果会保留新编辑并提示", async ({ page }) => {
  await installRaceMock(page, "replace");
  await page.goto(BASE);
  const editor = page.locator("textarea").first();
  await editor.fill("润色前的原文");
  await page.getByRole("button", { name: /开始润色/ }).click();
  await expect(page.getByText("润色结果", { exact: true })).toBeVisible();

  await editor.fill("润色前的原文 + 新编辑");
  await page.getByRole("button", { name: "替换原文", exact: true }).click();
  await expect(editor).toHaveValue("润色前的原文 + 新编辑");
  await expect(page.getByText("原文已继续编辑，未替换；请复制结果后手动合并")).toBeVisible();
});

test("自定义分类在探索页共享显示并持久化", async ({ page }) => {
  await installRaceMock(page, "replace");
  await page.goto(BASE);
  await page.getByRole("button", { name: "添加分类" }).click();
  await page.getByPlaceholder("新分类名称").fill("我的分类");
  await page.getByRole("button", { name: "确认添加分类" }).click();

  await page.locator("nav button", { hasText: "探索" }).click();
  await expect(page.locator("section").first().locator(".grid button", { hasText: "我的分类" })).toBeVisible();
  await page.reload();
  await page.locator("nav button", { hasText: "探索" }).click();
  await expect(page.locator("section").first().locator(".grid button", { hasText: "我的分类" })).toBeVisible();
});

test("备份中防重复，成功后展示完整路径与安全提示", async ({ page }) => {
  await installRaceMock(page, "backup");
  await page.goto(BASE);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByText("备份包含全部 Prompt 与敏感模型配置，请妥善保管。")).toBeVisible();

  await page.getByRole("button", { name: "备份数据", exact: true }).click();
  await waitForCall(page, "backup_database");
  await expect(page.getByRole("button", { name: "备份中...", exact: true })).toBeDisabled();
  await release(page, "backup_database");

  const path = "/Users/test/Library/Application Support/PromptDock/backups/promptdock-2026-11-11.db";
  await expect(page.getByLabel("备份文件路径")).toHaveValue(path);
  await expect(page.getByText("数据备份完成", { exact: true })).toBeVisible();
  expect(await commandCalls(page, "backup_database")).toHaveLength(1);
});

test("备份失败显示错误且允许重试，不展示假路径", async ({ page }) => {
  await installRaceMock(page, "backup-error");
  await page.goto(BASE);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.getByRole("button", { name: "备份数据", exact: true }).click();

  await expect(page.getByText("备份失败：disk full", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "备份数据", exact: true })).toBeEnabled();
  await expect(page.getByLabel("备份文件路径")).toHaveCount(0);
});

test("非 Tauri 环境备份明确失败且不伪装成功", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.getByRole("button", { name: "备份数据", exact: true }).click();

  await expect(page.getByText("桌面运行时不可用，无法备份数据", { exact: true })).toBeVisible();
  await expect(page.getByText("数据备份完成", { exact: true })).toHaveCount(0);
});
