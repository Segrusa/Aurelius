import { defineConfig } from "@playwright/test";

// One at a time: the measurements must not compete with each other for the CPU.
export default defineConfig({
  testDir: "tests",
  testMatch: "*.e2e.ts",
  globalSetup: "./tests/global-setup.ts",
  workers: 1,
  timeout: 90_000,
  use: {
    launchOptions: process.env.AURELIUS_CHROMIUM ? { executablePath: process.env.AURELIUS_CHROMIUM } : {},
    viewport: { width: 1440, height: 900 },
  },
});
