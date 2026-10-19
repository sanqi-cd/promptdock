import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30000,
  retries: 1,
  webServer: {
    command: "pnpm exec vite --host 127.0.0.1",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
  // html 报告供 CI 失败时上传；list 保留终端里的可读输出。
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: "http://localhost:1420",
    headless: true,
    // 失败时留下 trace 与截图，否则 CI 挂了只能靠猜。
    trace: "on-first-retry",
    screenshot: "only-on-failure",
  },
  // 桌面端实际运行在 macOS 的 WKWebView 里。只跑 Chromium 会漏掉引擎差异 ——
  // 这正是「以为覆盖了、其实没有」的典型来源。两个项目用同一套用例。
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
    { name: "webkit", use: { browserName: "webkit" } },
  ],
});
