import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests-season2",
  timeout: 240_000,
  expect: { timeout: 180_000 },
  workers: 1,
  fullyParallel: false,
  use: {
    baseURL: process.env.MINIJAM_E2E_BASE_URL ?? "http://127.0.0.1:4173",
    trace: "retain-on-failure"
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 4173",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: true,
    env: {
      VITE_TEST_WALLET: "true"
    }
  },
  projects: [{
    name: "chromium",
    use: {
      browserName: "chromium",
      launchOptions: {
        executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
      }
    }
  }]
});
