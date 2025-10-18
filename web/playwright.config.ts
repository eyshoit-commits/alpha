import { defineConfig, devices } from "@playwright/test";
import path from "path";

const webDir = __dirname;

export default defineConfig({
  testDir: path.join(webDir, "tests/e2e"),
  timeout: 60_000,
  expect: {
    timeout: 5_000,
  },
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"], ["html", { outputFolder: "playwright-report" }]],
  use: {
    trace: "on-first-retry",
    video: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: [
    {
      command: "npm run dev -w admin",
      port: 3000,
      reuseExistingServer: !process.env.CI,
      cwd: webDir,
      env: {
        NODE_ENV: "development",
        PORT: "3000",
      },
    },
    {
      command: "npm run dev -w app",
      port: 3001,
      reuseExistingServer: !process.env.CI,
      cwd: webDir,
      env: {
        NODE_ENV: "development",
        PORT: "3001",
      },
    },
  ],
  projects: [
    {
      name: "admin",
      testMatch: /admin-.*\.spec\.ts/,
      use: {
        ...devices["Desktop Chrome"],
        baseURL: "http://127.0.0.1:3000",
      },
    },
    {
      name: "namespace",
      testMatch: /namespace-.*\.spec\.ts/,
      use: {
        ...devices["Desktop Chrome"],
        baseURL: "http://127.0.0.1:3001",
      },
    },
  ],
});
