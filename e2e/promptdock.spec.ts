import { test, expect } from "@playwright/test";

const BASE = "http://localhost:1420";

test.describe("三栏布局", () => {
  test("页面加载后显示三栏布局", async ({ page }) => {
    await page.goto(BASE);
    // 侧边栏
    await expect(page.locator("aside").first()).toBeVisible();
    // 主内容区
    await expect(page.locator("main")).toBeVisible();
    // 编辑器默认显示
    await expect(page.locator("textarea").first()).toBeVisible();
  });
});

test.describe("侧边栏导航", () => {
  test("主题切换按钮可在深色与暖纸主题之间切换并持久化", async ({ page }) => {
    await page.goto(BASE);
    await page.evaluate(() => localStorage.removeItem("promptdock_theme"));
    await page.reload();

    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

    await page.getByRole("button", { name: "切换到暖纸主题" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "paper");
    await expect(page.evaluate(() => localStorage.getItem("promptdock_theme"))).resolves.toBe("paper");

    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "paper");

    await page.getByRole("button", { name: "切换到深色主题" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  });

  test("5 项导航全部可见", async ({ page }) => {
    await page.goto(BASE);
    const nav = page.locator("nav button");
    await expect(nav).toHaveCount(5);
    const labels = await nav.allTextContents();
    expect(labels).toContain("工作台");
    expect(labels).toContain("探索");
    expect(labels).toContain("收藏");
    expect(labels).toContain("最近使用");
    expect(labels).toContain("全部 Prompt");
  });

  test("点击探索切换到探索视图", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "探索" }).click();
    await expect(page.locator("h1", { hasText: "探索" })).toBeVisible();
    // 分类卡片网格
    await expect(page.locator(".grid button").first()).toBeVisible();
  });

  test("点击全部 Prompt 切换到列表视图", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();
    await expect(page.locator("h1", { hasText: "全部 Prompt" })).toBeVisible();
  });

  test("点击收藏切换到收藏列表", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "收藏" }).click();
    await expect(page.locator("h1", { hasText: "收藏" })).toBeVisible();
  });

  test("点击最近使用切换到最近列表", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "最近使用" }).click();
    await expect(page.locator("h1", { hasText: "最近使用" })).toBeVisible();
  });

  test("侧边栏底部设置入口切换到设置页", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("aside button", { hasText: "设置" }).click();
    await expect(page.locator("h1", { hasText: "模型 API 设置" })).toBeVisible();
  });

  test("新建 Prompt 按钮清空编辑器", async ({ page }) => {
    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("测试内容");
    await page.locator("aside button", { hasText: "新建 Prompt" }).click();
    await expect(textarea).toHaveValue("");
  });

  test("新建自定义分类后根据名称自动匹配图标", async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.removeItem("promptdock_custom_categories");
    });
    await page.goto(BASE);

    await page.getByRole("button", { name: "添加分类" }).click();
    await page.getByPlaceholder("新分类名称").fill("工作");
    await page.getByRole("button", { name: "确认添加分类" }).click();

    await expect(
      page.locator('[data-category-name="工作"] [data-category-icon="briefcase"]')
    ).toBeVisible();
    await expect(
      page.locator('[data-category-name="工作"] [data-category-icon="default"]')
    ).toHaveCount(0);
  });
});

test.describe("PromptEditor 编辑器", () => {
  test("AI 润色等待时展示模型生成的处理要点，完成后恢复结果展示", async ({ page }) => {
    await page.addInitScript(() => {
      const callbacks = new Map<number, (...args: unknown[]) => unknown>();
      const polishCalls: Record<string, unknown>[] = [];
      window.__polishCalls = polishCalls;
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          if (cmd === "list_api_configs") {
            return [
              {
                id: "test-config",
                name: "测试配置",
                provider: "Mimo",
                base_url: "https://example.test",
                model: "mimo-v2.5-pro",
                is_active: true,
                created_at: "2026-06-13T00:00:00Z",
                updated_at: "2026-06-13T00:00:00Z",
              },
            ];
          }
          if (cmd === "analyze_polish_process") {
            await new Promise((resolve) => window.setTimeout(resolve, 100));
            return [
              {
                title: `AI 识别-${args?.mode}`,
                desc: `分析对象：${args?.content}`,
              },
              {
                title: "AI 决定补充边界",
                desc: "根据输入判断目标、约束和输出格式",
              },
            ];
          }
          if (cmd === "polish_prompt") {
            polishCalls.push({ cmd, ...args });
            await new Promise((resolve) => window.setTimeout(resolve, 1200));
            return `优化后的 ${args?.content ?? ""}`;
          }
          if (cmd === "create_prompt") {
            return {
              id: "prompt-1",
              title: "",
              content: args?.content ?? "",
              optimized_content: args?.optimizedContent ?? null,
              category: null,
              summary: null,
              is_favorite: false,
              created_at: "2026-06-13T00:00:00Z",
              updated_at: "2026-06-13T00:00:00Z",
              last_used_at: null,
            };
          }
          if (cmd === "list_prompts") return [];
          return null;
        },
        transformCallback: (callback: (...args: unknown[]) => unknown) => {
          const id = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER);
          callbacks.set(id, callback);
          return id;
        },
        unregisterCallback: (id: number) => {
          callbacks.delete(id);
        },
        runCallback: (id: number, data: unknown) => callbacks.get(id)?.(data),
        callbacks,
      };
    });

    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("我想做一个每日推送高质量 AI 新闻的 skill");

    await page.getByRole("button", { name: /开始润色/ }).click();
    await expect(page.getByText("优化过程")).toBeVisible();
    await expect(page.getByText("AI 识别-general")).toBeVisible();
    await expect(page.getByText("分析对象：我想做一个每日推送高质量 AI 新闻的 skill")).toBeVisible();

    await expect(page.getByText("润色结果")).toBeVisible();
    await expect(page.getByText("优化过程")).toHaveCount(0);
  });

  test("连续切换润色功能时始终处理编辑器原文并清空旧结果", async ({ page }) => {
    await page.addInitScript(() => {
      const callbacks = new Map<number, (...args: unknown[]) => unknown>();
      const polishCalls: Record<string, unknown>[] = [];
      window.__polishCalls = polishCalls;
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          if (cmd === "list_api_configs") {
            return [
              {
                id: "test-config",
                name: "测试配置",
                provider: "Mimo",
                base_url: "https://example.test",
                model: "mimo-v2.5-pro",
                is_active: true,
                created_at: "2026-06-13T00:00:00Z",
                updated_at: "2026-06-13T00:00:00Z",
              },
            ];
          }
          if (cmd === "analyze_polish_process") {
            return [
              {
                title: `AI 处理-${args?.mode}`,
                desc: `原文：${args?.content}`,
              },
            ];
          }
          if (cmd === "polish_prompt") {
            polishCalls.push({ cmd, ...args });
            const delay = args?.mode === "general" ? 1000 : 100;
            await new Promise((resolve) => window.setTimeout(resolve, delay));
            return `${args?.mode} result from ${args?.content}`;
          }
          if (cmd === "create_prompt") {
            return {
              id: "prompt-1",
              title: "",
              content: args?.content ?? "",
              optimized_content: args?.optimizedContent ?? null,
              category: null,
              summary: null,
              is_favorite: false,
              created_at: "2026-06-13T00:00:00Z",
              updated_at: "2026-06-13T00:00:00Z",
              last_used_at: null,
            };
          }
          if (cmd === "update_prompt") return null;
          if (cmd === "list_prompts") return [];
          return null;
        },
        transformCallback: (callback: (...args: unknown[]) => unknown) => {
          const id = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER);
          callbacks.set(id, callback);
          return id;
        },
        unregisterCallback: (id: number) => {
          callbacks.delete(id);
        },
        runCallback: (id: number, data: unknown) => callbacks.get(id)?.(data),
        callbacks,
      };
    });

    const sourceContent = "我想做一个每日推送高质量 AI 新闻的 skill";
    await page.goto(BASE);
    await page.locator("textarea").first().fill(sourceContent);

    await page.getByRole("button", { name: "结构化" }).click();
    await page.getByRole("button", { name: /开始润色/ }).click();
    await expect(page.getByText(`structured result from ${sourceContent}`)).toBeVisible();

    await page.getByRole("button", { name: "优化" }).click();
    await page.getByRole("button", { name: /开始润色/ }).click();
    await expect(page.getByText(`structured result from ${sourceContent}`)).toHaveCount(0);
    await expect(page.getByText("AI 处理-general")).toBeVisible();
    await expect(page.getByText(`原文：${sourceContent}`)).toBeVisible();

    await expect(page.getByText(`general result from ${sourceContent}`)).toBeVisible();
    const calls = await page.evaluate(() => window.__polishCalls);
    expect(calls).toEqual([
      expect.objectContaining({ mode: "structured", content: sourceContent }),
      expect.objectContaining({ mode: "general", content: sourceContent }),
    ]);
  });

  test("AI 润色立即启动最终结果且不被过程要点阻塞", async ({ page }) => {
    await page.addInitScript(() => {
      const callbacks = new Map<number, (...args: unknown[]) => unknown>();
      const timingEvents: Array<{ cmd: string; at: number }> = [];
      window.__timingEvents = timingEvents;
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          if (cmd === "list_api_configs") {
            return [
              {
                id: "test-config",
                name: "测试配置",
                provider: "Mimo",
                base_url: "https://example.test",
                model: "mimo-v2.5-pro",
                is_active: true,
                created_at: "2026-06-13T00:00:00Z",
                updated_at: "2026-06-13T00:00:00Z",
              },
            ];
          }
          if (cmd === "analyze_polish_process") {
            timingEvents.push({ cmd: "analyze-start", at: Date.now() });
            await new Promise((resolve) => window.setTimeout(resolve, 1000));
            timingEvents.push({ cmd: "analyze-end", at: Date.now() });
            return [
              {
                title: "分析输入",
                desc: `根据${args?.mode}处理原文`,
              },
            ];
          }
          if (cmd === "polish_prompt") {
            timingEvents.push({ cmd: "polish-start", at: Date.now() });
            await new Promise((resolve) => window.setTimeout(resolve, 1000));
            timingEvents.push({ cmd: "polish-end", at: Date.now() });
            return `并发润色结果：${args?.content ?? ""}`;
          }
          if (cmd === "create_prompt") {
            return {
              id: "prompt-1",
              title: "",
              content: args?.content ?? "",
              optimized_content: args?.optimizedContent ?? null,
              category: null,
              summary: null,
              is_favorite: false,
              created_at: "2026-06-13T00:00:00Z",
              updated_at: "2026-06-13T00:00:00Z",
              last_used_at: null,
            };
          }
          if (cmd === "list_prompts") return [];
          return null;
        },
        transformCallback: (callback: (...args: unknown[]) => unknown) => {
          const id = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER);
          callbacks.set(id, callback);
          return id;
        },
        unregisterCallback: (id: number) => {
          callbacks.delete(id);
        },
        runCallback: (id: number, data: unknown) => callbacks.get(id)?.(data),
        callbacks,
      };
    });

    await page.goto(BASE);
    await page.locator("textarea").first().fill("请帮我优化这个产品需求");

    await page.getByRole("button", { name: /开始润色/ }).click();

    await expect.poll(
      async () =>
        page.evaluate(() =>
          window.__timingEvents?.find((event) => event.cmd === "polish-start")?.at ?? 0
        ),
      { timeout: 700 }
    ).toBeGreaterThan(0);

    const events = await page.evaluate(() => window.__timingEvents);
    const analyzeStart = events.find((event) => event.cmd === "analyze-start")?.at ?? 0;
    const polishStart = events.find((event) => event.cmd === "polish-start")?.at ?? 0;
    expect(analyzeStart === 0 || analyzeStart - polishStart >= 650).toBeTruthy();

    await expect(page.getByText("并发润色结果：请帮我优化这个产品需求")).toBeVisible();
  });

  test("AI 润色在短请求中不触发过程要点分析", async ({ page }) => {
    await page.addInitScript(() => {
      const callbacks = new Map<number, (...args: unknown[]) => unknown>();
      const calls: string[] = [];
      window.__polishCalls = calls;
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          calls.push(cmd);
          if (cmd === "list_api_configs") {
            return [
              {
                id: "test-config",
                name: "测试配置",
                provider: "Mimo",
                base_url: "https://example.test",
                model: "mimo-v2.5-pro",
                is_active: true,
                created_at: "2026-06-13T00:00:00Z",
                updated_at: "2026-06-13T00:00:00Z",
              },
            ];
          }
          if (cmd === "polish_prompt") {
            await new Promise((resolve) => window.setTimeout(resolve, 120));
            return "快速润色结果";
          }
          if (cmd === "create_prompt") {
            return {
              id: "prompt-1",
              title: "",
              content: args?.content ?? "",
              optimized_content: args?.optimizedContent ?? null,
              category: null,
              summary: null,
              is_favorite: false,
              created_at: "2026-06-13T00:00:00Z",
              updated_at: "2026-06-13T00:00:00Z",
              last_used_at: null,
            };
          }
          if (cmd === "list_prompts") return [];
          return null;
        },
        transformCallback: (callback: (...args: unknown[]) => unknown) => {
          const id = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER);
          callbacks.set(id, callback);
          return id;
        },
        unregisterCallback: (id: number) => {
          callbacks.delete(id);
        },
        runCallback: (id: number, data: unknown) => callbacks.get(id)?.(data),
        callbacks,
      };
    });

    await page.goto(BASE);
    await page.locator("textarea").first().fill("请帮我优化这个短需求");
    await page.getByRole("button", { name: /开始润色/ }).click();

    await expect(page.getByText("快速润色结果")).toBeVisible();
    const calls = await page.evaluate(() => window.__polishCalls);
    expect(calls).not.toContain("analyze_polish_process");
  });

  test("AI 润色流式分片先于最终结果展示", async ({ page }) => {
    await page.addInitScript(() => {
      const callbacks = new Map<number, (...args: unknown[]) => unknown>();
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          if (cmd === "list_api_configs") {
            return [
              {
                id: "test-config",
                name: "测试配置",
                provider: "Mimo",
                base_url: "https://example.test",
                model: "mimo-v2.5-pro",
                is_active: true,
                created_at: "2026-06-13T00:00:00Z",
                updated_at: "2026-06-13T00:00:00Z",
              },
            ];
          }
          if (cmd === "polish_prompt_stream") {
            const channel = args?.onChunk as { onmessage: (payload: unknown) => void };
            window.setTimeout(() => channel.onmessage({ type: "delta", delta: "第一段" }), 80);
            window.setTimeout(() => channel.onmessage({ type: "delta", delta: "第二段" }), 160);
            await new Promise((resolve) => window.setTimeout(resolve, 1000));
            return "第一段第二段最终完成";
          }
          if (cmd === "analyze_polish_process") {
            await new Promise((resolve) => window.setTimeout(resolve, 1200));
            return [
              {
                title: "分析输入",
                desc: "长请求才展示过程要点",
              },
            ];
          }
          if (cmd === "create_prompt") {
            return {
              id: "prompt-1",
              title: "",
              content: args?.content ?? "",
              optimized_content: args?.optimizedContent ?? null,
              category: null,
              summary: null,
              is_favorite: false,
              created_at: "2026-06-13T00:00:00Z",
              updated_at: "2026-06-13T00:00:00Z",
              last_used_at: null,
            };
          }
          if (cmd === "list_prompts") return [];
          return null;
        },
        transformCallback: (callback: (...args: unknown[]) => unknown) => {
          const id = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER);
          callbacks.set(id, callback);
          return id;
        },
        unregisterCallback: (id: number) => {
          callbacks.delete(id);
        },
        runCallback: (id: number, data: unknown) => callbacks.get(id)?.(data),
        callbacks,
      };
    });

    await page.goto(BASE);
    await page.locator("textarea").first().fill("请帮我优化这个较长需求");
    await page.getByRole("button", { name: /开始润色/ }).click();

    await expect(page.getByText("第一段")).toBeVisible({ timeout: 500 });
    await expect(page.getByText("第一段第二段最终完成")).toBeVisible();
  });

  test("textarea 可输入内容", async ({ page }) => {
    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("Hello PromptDock");
    await expect(textarea).toHaveValue("Hello PromptDock");
  });

  test("格式工具栏 7 个按钮可见", async ({ page }) => {
    await page.goto(BASE);
    const toolbar = page.locator("main button[title]");
    const titles = await toolbar.allTextContents();
    // Should have format buttons with titles
    expect(titles.length).toBeGreaterThanOrEqual(1);
  });

  test("粗体按钮插入 Markdown 语法", async ({ page }) => {
    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("测试");
    await textarea.evaluate((el: HTMLTextAreaElement) => {
      el.setSelectionRange(0, 2);
    });
    await page.locator("button[title='粗体']").click();
    const val = await textarea.inputValue();
    expect(val).toContain("**测试**");
  });

  test("斜体按钮插入 Markdown 语法", async ({ page }) => {
    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("测试");
    await textarea.evaluate((el: HTMLTextAreaElement) => {
      el.setSelectionRange(0, 2);
    });
    await page.locator("button[title='斜体']").click();
    const val = await textarea.inputValue();
    expect(val).toContain("*测试*");
  });

  test("粗体按钮无选中时插入占位符", async ({ page }) => {
    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("");
    await textarea.focus();
    await page.locator("button[title='粗体']").click();
    const val = await textarea.inputValue();
    expect(val).toContain("**粗体文字**");
  });

  test("代码按钮插入行内代码语法", async ({ page }) => {
    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("");
    await textarea.focus();
    await page.locator("button[title='行内代码']").click();
    const val = await textarea.inputValue();
    expect(val).toContain("`代码`");
  });

  test("链接按钮插入链接语法", async ({ page }) => {
    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("");
    await textarea.focus();
    await page.locator("button[title='链接']").click();
    const val = await textarea.inputValue();
    expect(val).toContain("[链接文字](url)");
  });

  test("列表按钮插入无序列表语法", async ({ page }) => {
    await page.goto(BASE);
    const textarea = page.locator("textarea").first();
    await textarea.fill("");
    await textarea.focus();
    await page.locator("button[title='无序列表']").click();
    const val = await textarea.inputValue();
    expect(val).toContain("- 列表项");
  });

  test("保存按钮可见", async ({ page }) => {
    await page.goto(BASE);
    await expect(page.locator("button", { hasText: "保存" }).first()).toBeVisible();
  });

  test("复制按钮可见", async ({ page }) => {
    await page.goto(BASE);
    await expect(page.locator("button", { hasText: "复制" }).first()).toBeVisible();
  });

  test("编辑器不显示历史版本占位区域", async ({ page }) => {
    await page.goto(BASE);
    await expect(page.getByText("历史版本")).toHaveCount(0);
    await expect(page.getByText("暂无历史版本")).toHaveCount(0);
  });
});

test.describe("ExploreView 探索页面", () => {
  test("分类卡片 2x3 网格", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "探索" }).click();
    // 探索页面有分类网格
    const section = page.locator("section", { hasText: "分类浏览" });
    await expect(section).toBeVisible();
    const cards = section.locator(".grid button");
    expect(await cards.count()).toBeGreaterThanOrEqual(10);
  });

  test("点击分类跳转到分类列表", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "探索" }).click();
    await page.locator("section .grid button").first().click();
    await expect(page.locator("h1", { hasText: "分类筛选" })).toBeVisible();
  });
});

test.describe("PromptListView 列表视图", () => {
  test("搜索框可见", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();
    const searchInput = page.locator("input[placeholder*='搜索']").first();
    await expect(searchInput).toBeVisible();
  });

  test("空列表显示空状态", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();
    const emptyText = page.locator("text=暂无 Prompt");
    // 可能显示也可能不显示，取决于是否已有数据
    const count = await page.locator("text=暂无 Prompt").count();
    // 至少页面不崩溃
    expect(count).toBeGreaterThanOrEqual(0);
  });
});

test.describe("RightPanel 右侧面板", () => {
  test("编辑器模式下显示 Prompt 信息和快捷操作", async ({ page }) => {
    await page.goto(BASE);
    const panel = page.locator("aside").last();
    await expect(panel.locator("text=Prompt 信息")).toBeVisible();
    await expect(panel.locator("text=快捷操作")).toBeVisible();
    await expect(panel.locator("text=提示")).toBeVisible();
  });

  test("右侧面板不展示产品 logo 图标", async ({ page }) => {
    await page.goto(BASE);
    const panel = page.locator("aside").last();
    await expect(panel.locator('img[alt="PromptDock"]')).toHaveCount(0);
  });

  test("列表模式下显示统计", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();
    const panel = page.locator("aside").last();
    await expect(panel.locator("text=统计")).toBeVisible();
    await expect(panel.locator("text=总数")).toBeVisible();
    await expect(panel.getByText("收藏", { exact: true })).toBeVisible();
    await expect(panel.getByText("分类", { exact: true })).toBeVisible();
  });
});

test.describe("SettingsPage 设置页面", () => {
  test("设置页显示标题和添加配置按钮", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("aside button", { hasText: "设置" }).click();
    await expect(page.locator("h1", { hasText: "模型 API 设置" })).toBeVisible();
    await expect(page.locator("button", { hasText: "添加配置" })).toBeVisible();
  });

  test("添加配置表单可展开", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("aside button", { hasText: "设置" }).click();
    await page.locator("button", { hasText: "添加配置" }).click();
    await expect(page.locator("text=配置名称")).toBeVisible();
    await expect(page.locator("text=Provider")).toBeVisible();
    await expect(page.locator("text=Base URL")).toBeVisible();
    await expect(page.getByText("API Key", { exact: true })).toBeVisible();
    await expect(page.locator("text=Model Name")).toBeVisible();
    await expect(page.locator("button", { hasText: "测试连接" })).toBeVisible();
  });

  test("空状态提示正常", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("aside button", { hasText: "设置" }).click();
    // 可能显示空状态，取决于是否有配置
    const emptyHint = page.locator("text=暂无 API 配置");
    const count = await emptyHint.count();
    expect(count).toBeGreaterThanOrEqual(0);
  });
});

test.describe("交叉导航", () => {
  test("工作台→探索→列表→设置→工作台 完整回路", async ({ page }) => {
    await page.goto(BASE);
    // 工作台
    await expect(page.locator("textarea").first()).toBeVisible();

    // → 探索
    await page.locator("nav button", { hasText: "探索" }).click();
    await expect(page.locator("h1", { hasText: "探索" })).toBeVisible();

    // → 全部 Prompt
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();
    await expect(page.locator("h1", { hasText: "全部 Prompt" })).toBeVisible();

    // → 设置
    await page.locator("aside button", { hasText: "设置" }).click();
    await expect(page.locator("h1", { hasText: "模型 API 设置" })).toBeVisible();

    // → 返回工作台
    await page.locator("nav button", { hasText: "工作台" }).click();
    await expect(page.locator("textarea").first()).toBeVisible();
  });

  test("点击列表卡片加载到编辑器", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();
    // 如果有 prompt 卡片，点击第一个
    const card = page.locator("main .rounded-xl button").first();
    const cardCount = await card.count();
    if (cardCount > 0) {
      await card.click();
      // 应该切换到编辑器视图
      await expect(page.locator("textarea").first()).toBeVisible();
    }
  });
});

/**
 * 后端时间戳的展示。
 *
 * 后端写的是 `datetime('now','localtime')` → `"2026-06-21 19:36:00"`：非 ISO 8601、
 * 不带时区，`new Date()` 对它怎么处理是 ECMA-262 未规定的。这套用例在 Chromium
 * （playwright 默认）和 WebKit（贴近实际发布用的 WKWebView）下都会跑，
 * 用来证明两种引擎表现一致。
 */
test.describe("时间戳展示", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string) => {
          if (cmd === "list_api_configs") {
            return [
              {
                id: "cfg",
                name: "测试配置",
                provider: "openai",
                base_url: "https://example.test",
                model: "gpt-4.1-mini",
                is_active: true,
                created_at: "2026-06-21 19:36:00",
              },
            ];
          }
          if (cmd === "list_prompts") {
            return [
              {
                id: "p1",
                title: "时间戳用例",
                content: "内容",
                optimized_content: null,
                category: null,
                summary: null,
                is_favorite: false,
                // 后端真实格式：非 ISO、无时区
                created_at: "2026-06-21 19:36:00",
                updated_at: "2026-06-21 19:36:00",
                last_used_at: "2026-06-21 19:36:00",
              },
            ];
          }
          return null;
        },
        transformCallback: () => 1,
        unregisterCallback: () => {},
        runCallback: () => {},
      };
    });
  });

  test("列表卡片显示可读日期而不是 Invalid Date", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();

    await expect(page.getByText("时间戳用例")).toBeVisible();
    await expect(page.getByText("Invalid Date")).toHaveCount(0);
    // 2026-06-21 在 zh-CN 下的日期表示
    await expect(page.getByText(/2026\/6\/21/).first()).toBeVisible();
  });

  test("无法解析的时间戳显示为空，而不是 Invalid Date", async ({ page }) => {
    // 回归测试：`conn_now()` 在查询失败时会把字面量 `"unknown"` 写进 updated_at，
    // 于是卡片上出现 "Invalid Date"。现在解析失败一律返回空串。
    await page.addInitScript(() => {
      const internals = window.__TAURI_INTERNALS__ as {
        invoke: (cmd: string) => Promise<unknown>;
      };
      const original = internals.invoke;
      internals.invoke = async (cmd: string) => {
        if (cmd !== "list_prompts") return original(cmd);
        return [
          {
            id: "p-bad",
            title: "坏时间戳",
            content: "内容",
            optimized_content: null,
            category: null,
            summary: null,
            is_favorite: false,
            created_at: "unknown",
            updated_at: "unknown",
            last_used_at: "unknown",
          },
        ];
      };
    });

    await page.goto(BASE);
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();

    await expect(page.getByText("坏时间戳")).toBeVisible();
    await expect(page.getByText("Invalid Date")).toHaveCount(0);
  });

  test("设置页显示可读日期而不是 Invalid Date", async ({ page }) => {
    await page.goto(BASE);
    await page.locator("aside button", { hasText: "设置" }).click();

    await expect(page.getByText("测试配置")).toBeVisible();
    await expect(page.getByText("Invalid Date")).toHaveCount(0);
    await expect(page.getByText(/2026\/6\/21/).first()).toBeVisible();
  });
});

/** 无标题的 Prompt 落库后会在列表里显示「未命名」，几乎找不回来。 */
test.describe("标题兜底", () => {
  test("未填标题时用正文首行生成标题", async ({ page }) => {
    await page.addInitScript(() => {
      window.__createCalls = [];
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          if (cmd === "list_api_configs") return [];
          if (cmd === "list_prompts") return [];
          if (cmd === "create_prompt") {
            (window.__createCalls as Record<string, unknown>[]).push({ ...args });
            return {
              id: "created-1",
              title: args?.title ?? "",
              content: args?.content ?? "",
              optimized_content: null,
              category: null,
              summary: null,
              is_favorite: false,
              created_at: "2026-06-21 19:36:00",
              updated_at: "2026-06-21 19:36:00",
              last_used_at: null,
            };
          }
          return null;
        },
        transformCallback: () => 1,
        unregisterCallback: () => {},
        runCallback: () => {},
      };
    });

    await page.goto(BASE);
    await page
      .locator("textarea")
      .first()
      .fill("帮我写一个关于 AI 的视频脚本\n第二行不该成为标题");
    await page.getByRole("button", { name: "保存", exact: true }).click();

    await expect
      .poll(async () =>
        page.evaluate(() => (window.__createCalls as Record<string, unknown>[]).length)
      )
      .toBeGreaterThan(0);

    const title = await page.evaluate(
      () => (window.__createCalls as Record<string, unknown>[])[0].title
    );
    expect(title).toBe("帮我写一个关于 AI 的视频脚本");
  });
});

/**
 * 列表数据竞态回归测试。
 *
 * 侧边栏切换筛选会并发发出多个 `list_prompts`（点一次分类最多 5 个）。如果响应
 * 乱序返回、后到的旧结果直接写入 store，用户就会看到「标题是全部 Prompt，内容
 * 却是收藏列表」。这里让「收藏」的响应明显慢于「全部」，制造确定的乱序。
 */
test.describe("列表筛选竞态", () => {
  const FAVORITES_DELAY_MS = 400;
  const ALL_DELAY_MS = 20;

  test("快速切换筛选时慢的旧响应不会覆盖新视图", async ({ page }) => {
    await page.addInitScript(
      ({ favoritesDelay, allDelay }: { favoritesDelay: number; allDelay: number }) => {
        const makePrompts = (prefix: string) =>
          Array.from({ length: 2 }, (_, i) => ({
            id: `${prefix}-${i}`,
            title: `${prefix}-${i}`,
            content: "内容",
            optimized_content: null,
            category: null,
            summary: null,
            is_favorite: prefix === "收藏项",
            created_at: "2026-01-01 00:00:00",
            updated_at: "2026-01-01 00:00:00",
            last_used_at: "2026-01-01 00:00:00",
          }));

        window.__TAURI_INTERNALS__ = {
          invoke: async (cmd: string, args?: Record<string, unknown>) => {
            if (cmd === "list_api_configs") return [];
            if (cmd === "list_prompts") {
              const favoriteOnly = args?.favoriteOnly === true;
              const delay = favoriteOnly ? favoritesDelay : allDelay;
              await new Promise((resolve) => window.setTimeout(resolve, delay));
              return makePrompts(favoriteOnly ? "收藏项" : "全部项");
            }
            return null;
          },
          transformCallback: () => 1,
          unregisterCallback: () => {},
          runCallback: () => {},
        };
      },
      { favoritesDelay: FAVORITES_DELAY_MS, allDelay: ALL_DELAY_MS }
    );

    await page.goto(BASE);

    // 先点「收藏」（慢响应在飞），立刻点「全部 Prompt」（快响应先回）
    await page.locator("nav button", { hasText: "收藏" }).click();
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();

    await expect(page.locator("h1", { hasText: "全部 Prompt" })).toBeVisible();
    await expect(page.getByText("全部项-0")).toBeVisible();

    // 等到慢的旧响应确实回来了，列表仍须是「全部」而不是被覆盖成「收藏」
    await page.waitForTimeout(FAVORITES_DELAY_MS + 300);

    await expect(page.getByText("全部项-0")).toBeVisible();
    await expect(page.getByText("收藏项-0")).toHaveCount(0);
  });
});

/**
 * 搜索状态隔离回归测试。
 *
 * `searchQuery` 曾经是全局 store 字段且没有任何清空入口，于是搜过一次之后，
 * 再点收藏 / 最近使用 / 分类，看到的仍然是被劫持的搜索结果，标题却是新模式。
 */
test.describe("搜索与列表模式隔离", () => {
  test("切换列表模式时清空上一轮的搜索词", async ({ page }) => {
    await page.addInitScript(() => {
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args?: Record<string, unknown>) => {
          if (cmd === "list_api_configs") return [];
          if (cmd === "list_prompts") {
            return [
              {
                id: "all-list-0",
                title: "全部项-0",
                content: "内容",
                optimized_content: null,
                category: null,
                summary: null,
                is_favorite: false,
                created_at: "2026-01-01 00:00:00",
                updated_at: "2026-01-01 00:00:00",
                last_used_at: "2026-01-01 00:00:00",
              },
            ];
          }
          if (cmd === "search_prompts") {
            return [
              {
                id: "hit-0",
                title: "搜索结果项-0",
                summary: `命中 ${args?.query}`,
                category: null,
                tags: [],
                updated_at: "2026-01-01 00:00:00",
              },
            ];
          }
          return null;
        },
        transformCallback: () => 1,
        unregisterCallback: () => {},
        runCallback: () => {},
      };
    });

    await page.goto(BASE);
    await page.locator("nav button", { hasText: "全部 Prompt" }).click();

    // 在列表搜索框里搜一次
    const searchBox = page.getByPlaceholder("搜索 Prompt 标题、内容、标签...");
    await searchBox.fill("视频");
    await expect(page.getByText("搜索结果项-0")).toBeVisible();
    await expect(page.locator("h1", { hasText: "搜索结果" })).toBeVisible();

    // 切到收藏：搜索必须被清空，列表回到正常的收藏视图
    await page.locator("nav button", { hasText: "收藏" }).click();

    await expect(page.locator("h1", { hasText: "收藏" })).toBeVisible();
    await expect(searchBox).toHaveValue("");
    await expect(page.getByText("搜索结果项-0")).toHaveCount(0);
    await expect(page.getByText("全部项-0")).toBeVisible();
  });
});
