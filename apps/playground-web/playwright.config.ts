import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "retain-on-failure"
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 4173",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: true,
    env: {
      VITE_TEST_WALLET: "true",
      VITE_MINIJAM_GENESIS_HASH: `0x${"00".repeat(32)}`
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
