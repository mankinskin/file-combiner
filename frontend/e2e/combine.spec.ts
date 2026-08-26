import { test, expect } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';

const token = process.env.COMBINE_TOKEN ?? '';
const firstPath = path.join(__dirname, 'fixtures', 'first.txt');
const secondPath = path.join(__dirname, 'fixtures', 'second.txt');

test('combines two ordered files and downloads combined.txt', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Token').fill(token);

  const fileInputs = page.locator('input[type=file]');
  await fileInputs.nth(0).setInputFiles(firstPath);
  await fileInputs.nth(1).setInputFiles(secondPath);

  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('button', { name: 'Combine and download' }).click(),
  ]);

  expect(download.suggestedFilename()).toBe('combined.txt');

  const downloadPath = await download.path();
  const content = fs.readFileSync(downloadPath as string, 'utf-8');
  const expected =
    fs.readFileSync(firstPath, 'utf-8') + '\n' + fs.readFileSync(secondPath, 'utf-8');
  expect(content).toBe(expected);
});
