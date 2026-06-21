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
    browserName: "chromium",
    headless: true,
  },
});
