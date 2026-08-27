import { test, expect } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';

const token = process.env.COMBINE_TOKEN ?? '';
const firstPath = path.join(__dirname, 'fixtures', 'first.txt');
const secondPath = path.join(__dirname, 'fixtures', 'second.txt');
const invalidUtf8Path = path.join(__dirname, 'fixtures', 'invalid-utf8.txt');

test('combines two ordered files and clears state after download', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);

  const fileInputs = page.locator('input[type=file]');
  await fileInputs.nth(0).setInputFiles(firstPath);
  await fileInputs.nth(1).setInputFiles(secondPath);

  // Selected text is rendered as an inert, read-only preview before submission.
  const expectedFirst = fs.readFileSync(firstPath, 'utf-8');
  await expect(page.getByRole('region', { name: /First file preview/ })).toContainText(
    expectedFirst.trim(),
  );

  await page.getByRole('button', { name: 'Combine' }).click();
  await expect(page.getByRole('status')).toBeVisible();

  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('button', { name: 'Download combined.txt' }).click(),
  ]);

  expect(download.suggestedFilename()).toBe('combined.txt');

  const downloadPath = await download.path();
  const content = fs.readFileSync(downloadPath as string, 'utf-8');
  const expected =
    fs.readFileSync(firstPath, 'utf-8') + '\n' + fs.readFileSync(secondPath, 'utf-8');
  expect(content).toBe(expected);

  // Completing the download clears the access key and selections.
  await expect(page.getByLabel('Access key')).toHaveValue('');
  await expect(page.locator('input[type=file]').nth(0)).toHaveValue('');
});

test('rejects an invalid UTF-8 file locally and never renders a preview', async ({ page }) => {
  await page.goto('/');
  const fileInputs = page.locator('input[type=file]');
  await fileInputs.nth(0).setInputFiles(invalidUtf8Path);

  await expect(page.getByRole('alert')).toContainText('not valid UTF-8');
  await expect(page.getByRole('region', { name: /First file preview/ })).toHaveCount(0);
});

test('retains the access key and selections after a failed request for retry', async ({
  page,
}) => {
  await page.route('**/api/combine', (route) =>
    route.fulfill({
      status: 401,
      contentType: 'application/json',
      body: JSON.stringify({ code: 'unauthorized', message: 'invalid credentials' }),
    }),
  );

  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  const fileInputs = page.locator('input[type=file]');
  await fileInputs.nth(0).setInputFiles(firstPath);
  await fileInputs.nth(1).setInputFiles(secondPath);

  await page.getByRole('button', { name: 'Combine' }).click();

  await expect(page.getByRole('alert', { name: '' })).toContainText('invalid credentials');
  await expect(page.getByLabel('Access key')).toHaveValue(token);
  await expect(page.getByRole('region', { name: /First file preview/ })).toBeVisible();
});

test('reset clears all local state and returns focus to the access key field', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  const fileInputs = page.locator('input[type=file]');
  await fileInputs.nth(0).setInputFiles(firstPath);

  await page.getByRole('button', { name: 'Reset' }).click();

  await expect(page.getByLabel('Access key')).toHaveValue('');
  await expect(page.getByLabel('Access key')).toBeFocused();
  await expect(page.locator('input[type=file]').nth(0)).toHaveValue('');
});

