import { test, expect } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';

const token = process.env.COMBINE_TOKEN ?? '';
const firstPath = path.join(__dirname, 'fixtures', 'first.txt');
const secondPath = path.join(__dirname, 'fixtures', 'second.txt');
const firstMarkdownPath = path.join(__dirname, 'fixtures', 'first.md');
const invalidUtf8Path = path.join(__dirname, 'fixtures', 'invalid-utf8.txt');

test('combines pasted text and clears state after download', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);

  const firstText = fs.readFileSync(firstPath, 'utf-8');
  const secondText = fs.readFileSync(secondPath, 'utf-8');
  await page.getByLabel('Sales guide template').fill(firstText);
  await page.getByLabel('Customer information').fill(secondText);

  await page.getByRole('button', { name: 'Generate guide' }).click();
  await expect(page.getByRole('status')).toBeVisible();

  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('button', { name: 'Download guide' }).click(),
  ]);

  expect(download.suggestedFilename()).toBe('combined.txt');

  const downloadPath = await download.path();
  const content = fs.readFileSync(downloadPath as string, 'utf-8');
  expect(content).toBe(firstText + '\n' + secondText);

  // Completing the download clears the access key and both editors.
  await expect(page.getByLabel('Access key')).toHaveValue('');
  await expect(page.getByLabel('Sales guide template')).toHaveValue('');
  await expect(page.getByLabel('Customer information')).toHaveValue('');

  // The diagnostics sidebar is always present and never nested inside the
  // centered workspace column.
  await expect(page.getByRole('region', { name: 'Diagnostics' })).toBeVisible();
  await expect(page.locator('main.workspace .diagnostics-sidebar')).toHaveCount(0);
});

test('loads a Markdown file into an editor slot', async ({ page }) => {
  await page.goto('/');
  const fileInputs = page.locator('input[type=file]');
  await fileInputs.nth(0).setInputFiles(firstMarkdownPath);

  const expected = fs.readFileSync(firstMarkdownPath, 'utf-8').replace(/\r\n/g, '\n');
  await expect(page.getByLabel('Sales guide template')).toHaveValue(expected);
});

test('rejects an invalid UTF-8 file locally and leaves the editor untouched', async ({
  page,
}) => {
  await page.goto('/');
  const fileInputs = page.locator('input[type=file]');
  await fileInputs.nth(0).setInputFiles(invalidUtf8Path);

  await expect(page.getByRole('alert')).toContainText('not valid UTF-8');
  await expect(page.getByLabel('Sales guide template')).toHaveValue('');
});

test('never shows a visible marker for CR line endings or control characters', async ({
  page,
}) => {
  await page.goto('/');
  const pasted = 'first line\r\nsecond line\r\x07third line';
  await page.getByLabel('Sales guide template').fill(pasted);

  const value = await page.getByLabel('Sales guide template').inputValue();
  expect(value).not.toContain('\r');
  expect(value).not.toContain('\u0007');
  expect(value).not.toContain('\u{FFFD}');
  expect(value).toBe('first line\nsecond line\nthird line');
});

test('retains the access key and editor content after a failed request for retry', async ({
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
  const firstText = fs.readFileSync(firstPath, 'utf-8');
  const secondText = fs.readFileSync(secondPath, 'utf-8');
  await page.getByLabel('Sales guide template').fill(firstText);
  await page.getByLabel('Customer information').fill(secondText);

  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('alert', { name: '' })).toContainText('invalid credentials');
  await expect(page.getByLabel('Access key')).toHaveValue(token);
  await expect(page.getByLabel('Sales guide template')).toHaveValue(firstText);
});

test('sends group credentials with the Group authorization scheme', async ({ page }) => {
  let authorization = '';
  await page.route('**/api/combine', (route) => {
    authorization = route.request().headers().authorization ?? '';
    return route.fulfill({
      status: 401,
      contentType: 'application/json',
      body: JSON.stringify({ code: 'unauthorized', message: 'invalid credentials' }),
    });
  });

  await page.goto('/');
  await page.getByLabel('Access method').selectOption('group');
  await page.getByLabel('Group name').fill('sales');
  await page.getByLabel('Group password').fill('secret');
  await page.getByLabel('Sales guide template').fill('template');
  await page.getByLabel('Customer information').fill('customer');
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('alert', { name: '' })).toContainText('invalid credentials');
  expect(authorization).toBe(`Group ${Buffer.from('sales:secret').toString('base64')}`);
});

test('sends a direct PAT with the Bearer authorization scheme', async ({ page }) => {
  let authorization = '';
  await page.route('**/api/combine', (route) => {
    authorization = route.request().headers().authorization ?? '';
    return route.fulfill({
      status: 401,
      contentType: 'application/json',
      body: JSON.stringify({ code: 'unauthorized', message: 'invalid credentials' }),
    });
  });

  await page.goto('/');
  await page.getByLabel('Access method').selectOption('direct-pat');
  await page.getByLabel('Copilot personal access token').fill('ghp_test_token');
  await page.getByLabel('Sales guide template').fill('template');
  await page.getByLabel('Customer information').fill('customer');
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('alert', { name: '' })).toContainText('invalid credentials');
  expect(authorization).toBe('Bearer ghp_test_token');
});

test('reset clears all local state and returns focus to the access key field', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Sales guide template').fill('draft content');

  await page.getByRole('button', { name: 'Reset' }).click();

  await expect(page.getByLabel('Access key')).toHaveValue('');
  await expect(page.getByLabel('Access key')).toBeFocused();
  await expect(page.getByLabel('Sales guide template')).toHaveValue('');
  await expect(page.locator('input[type=file]').nth(0)).toHaveValue('');
});

