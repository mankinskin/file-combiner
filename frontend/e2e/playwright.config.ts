import { defineConfig } from '@playwright/test';

const baseURL = process.env.BASE_URL ?? 'http://127.0.0.1:8080';
process.env.COMBINE_TOKEN = process.env.COMBINE_TOKEN ?? '';

// A model-backed run performs three sequential prompts (template analysis,
// profile analysis, then the combination/generation chain). Measured live
// runs reach the first representation at ~13.5s and final artifacts at ~29s,
// so both the per-test budget and the per-assertion budget must exceed the
// slowest stage. The 5s Playwright default fails a working pipeline.
const testTimeout = Number(process.env.E2E_TEST_TIMEOUT_MS ?? 180_000);
const expectTimeout = Number(process.env.E2E_EXPECT_TIMEOUT_MS ?? 90_000);
const outputDir = process.env.PLAYWRIGHT_OUTPUT_DIR ?? 'test-results';

export default defineConfig({
  testDir: './',
  outputDir,
  timeout: testTimeout,
  expect: {
    timeout: expectTimeout,
  },
  use: {
    baseURL,
  },
});
