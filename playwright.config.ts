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
  use: {
    baseURL: "http://localhost:1420",
    headless: true,
  },
  // 桌面端实际运行在 macOS 的 WKWebView 里。只跑 Chromium 会漏掉引擎差异 ——
  // 这正是「以为覆盖了、其实没有」的典型来源。两个项目用同一套用例。
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
    { name: "webkit", use: { browserName: "webkit" } },
  ],
});
