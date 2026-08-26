import { defineConfig } from '@playwright/test';

const baseURL = process.env.BASE_URL ?? 'http://127.0.0.1:8080';
process.env.COMBINE_TOKEN = process.env.COMBINE_TOKEN ?? '';

export default defineConfig({
  testDir: './',
  timeout: 30_000,
  use: {
    baseURL,
  },
});
