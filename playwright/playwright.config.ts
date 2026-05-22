import { defineConfig, devices } from "@playwright/test";
const path = require("path");

/**
 * Read environment variables from file.
 * https://github.com/motdotla/dotenv
 */
// import dotenv from 'dotenv';
// import path from 'path';
// dotenv.config({ path: path.resolve(__dirname, '.env') });

/**
 * See https://playwright.dev/docs/test-configuration.
 */
export default defineConfig({
  testDir: ".",
  /* Run tests in files in parallel */
  fullyParallel: true,
  /* Fail the build on CI if you accidentally left test.only in the source code. */
  forbidOnly: !!process.env.CI,
  /* Retry on CI only — masks timing-sensitive flakes (webkit animation
     races in particular) while keeping local runs fast. */
  retries: process.env.CI ? 2 : 0,
  /* Opt out of parallel tests on CI. */
  workers: process.env.CI ? 1 : undefined,
  /* Reporter to use. See https://playwright.dev/docs/test-reporters */
  reporter: process.env.CI ? [["list"], ["blob"]] : "html",
  /* Shared settings for all the projects below. See https://playwright.dev/docs/api/class-testoptions. */
  use: {
    /* Collect a trace only when a test fails every retry.
       "on-first-retry" (Playwright's default) would write traces for the
       (often passing) retry instead of the failure we want to debug. */
    trace: "retain-on-failure",
  },

  // Each test is given 5 minutes.
  timeout: 5 * 60 * 1000,

  /* Configure projects for major browsers */
  projects: [
    {
      name: "chromium",
      grepInvert: /mobile/,
      use: { ...devices["Desktop Chrome"] },
    },

    {
      name: "firefox",
      grepInvert: /mobile/,
      use: { ...devices["Desktop Firefox"] },
    },

    {
      name: "webkit",
      grepInvert: /mobile/,
      use: { ...devices["Desktop Safari"] },
      // Webkit is slower, so we give it more time.
      expect: {
        timeout: 30 * 1000, // 30 seconds
      },
    },

    // Temporarily disabled mobile tests in CI. The mobile browser CI downloads acts different than the local tests which pass
    // /* Test against mobile viewports. */
    // {
    //   name: "Mobile Chrome",
    //   grep: /mobile/,
    //   use: { ...devices["Pixel 5"] },
    // },

    // {
    //   name: "Mobile Safari",
    //   grep: /mobile/,
    //   use: { ...devices["iPhone 12"] },
    // },
  ],

  /* Run your local dev server before starting the tests */
  webServer: {
    cwd: path.join(process.cwd(), "../preview"),
    command: "dx run --web --release",
    port: 8080,
    timeout: 50 * 60 * 1000,
    reuseExistingServer: !process.env.CI,
    stdout: "pipe",
  },
});
