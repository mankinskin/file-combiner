import { test, expect } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';

const token = process.env.COMBINE_TOKEN ?? '';
const templateText = `# Closing conversation

## Opening
Are you ready to begin?

## Offer
Explain the next steps clearly.`;
const profileText = `# Customer profile

## Goal
Build a repeatable business.

## Situation
Already operates a small hospitality business.`;

const pitchScriptTemplateText = fs
  .readFileSync(path.join(__dirname, 'fixtures', 'pitch-script-template.md'), 'utf-8')
  .replace(/\r\n/g, '\n');
const pitchScriptProfileText = fs
  .readFileSync(path.join(__dirname, 'fixtures', 'pitch-script-profile.md'), 'utf-8')
  .replace(/\r\n/g, '\n');

// A template with more headings than the schema allows deterministically
// fails real backend validation, without mocking any network call.
const tooManyStagesTemplate = Array.from(
  { length: 40 },
  (_, index) => `# Stage ${index + 1}`,
).join('\n');

test('a failed analysis shows a full attempted/failed trace, not just a generic message', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(tooManyStagesTemplate);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  const alert = page.getByRole('alert');
  await expect(alert).toBeVisible();
  await expect(alert).toContainText('analyzing');
  await expect(alert).toContainText('too many stages');

  const alertText = ((await alert.textContent()) ?? '').trim();
  expect(alertText).not.toBe('artifact analysis failed');
});

test('a failed analysis also populates the diagnostics sidebar with the same trace', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(tooManyStagesTemplate);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  const diagnosticsRegion = page.getByRole('region', { name: 'Diagnostics' });
  await expect(diagnosticsRegion).toContainText('analyzing');
  await expect(diagnosticsRegion).toContainText('too many stages');
});

test('a successful request populates the diagnostics sidebar', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('region', { name: 'Generated artifacts' })).toBeVisible();
  const diagnosticsRegion = page.getByRole('region', { name: 'Diagnostics' });
  await expect(diagnosticsRegion).not.toContainText('No diagnostics yet');
});

test('a user can paste both sources and download each generated artifact', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('region', { name: 'Latent representations' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Generated artifacts' })).toBeVisible();

  const overview = page.getByRole('button', { name: 'Download closer-overview' });
  const guide = page.getByRole('button', {
    name: 'Download individualized-conversation-guide',
  });
  await expect(overview).toBeVisible();
  await expect(guide).toBeVisible();

  const [download] = await Promise.all([
    page.waitForEvent('download'),
    overview.click(),
  ]);
  expect(download.suggestedFilename()).toBe('closer-overview.md');

  const [secondDownload] = await Promise.all([
    page.waitForEvent('download'),
    guide.click(),
  ]);
  expect(secondDownload.suggestedFilename()).toBe(
    'individualized-conversation-guide.md',
  );

  await expect(page.getByLabel('Conversation template')).toHaveValue(templateText);
  await expect(page.getByLabel('Person information')).toHaveValue(profileText);
});

test('a user can generate artifacts from real pitch-script fixtures', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(pitchScriptTemplateText);
  await page.getByLabel('Person information').fill(pitchScriptProfileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('region', { name: 'Latent representations' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Generated artifacts' })).toBeVisible();

  const overview = page.getByRole('button', { name: 'Download closer-overview' });
  const guide = page.getByRole('button', {
    name: 'Download individualized-conversation-guide',
  });
  await expect(overview).toBeVisible();
  await expect(guide).toBeVisible();

  const [download] = await Promise.all([
    page.waitForEvent('download'),
    overview.click(),
  ]);
  expect(download.suggestedFilename()).toBe('closer-overview.md');

  await expect(page.getByLabel('Conversation template')).toHaveValue(pitchScriptTemplateText);
  await expect(page.getByLabel('Person information')).toHaveValue(pitchScriptProfileText);
});

test('a user can see intermediate analysis before final artifacts', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill('# Discovery\nWhat matters most?');
  await page.getByLabel('Person information').fill('# Profile\nGoal: improve operations');
  await page.getByRole('button', { name: 'Generate guide' }).click();

  const representations = page.getByRole('region', { name: 'Latent representations' });
  await expect(representations).toBeVisible();
  await expect(representations).toContainText('Stages');
  await expect(page.getByRole('region', { name: 'Generated artifacts' })).toBeVisible();
});

test('an incomplete form keeps Generate guide unavailable', async ({ page }) => {
  await page.goto('/');
  const generate = page.getByRole('button', { name: 'Generate guide' });
  await expect(generate).toBeDisabled();
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  await expect(generate).toBeDisabled();
  await page.getByLabel('Person information').fill(profileText);
  await expect(generate).toBeEnabled();
});

test('invalid group credentials show an accessible authentication error', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access method').selectOption('group');
  await page.getByLabel('Group name').fill('invalid-group');
  await page.getByLabel('Group password').fill('invalid-password');
  await page.getByLabel('Conversation template').fill(templateText);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('alert')).toContainText('invalid credentials');
  await expect(page.getByRole('alert').locator('ol')).toHaveCount(0);
  await expect(page.getByLabel('Conversation template')).toHaveValue(templateText);
  await expect(page.getByLabel('Person information')).toHaveValue(profileText);
});

test('pasted line endings and control characters are normalized in the editor', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Conversation template').fill('first line\r\nsecond line\r\u0007third line');
  await expect(page.getByLabel('Conversation template')).toHaveValue(
    'first line\nsecond line\nthird line',
  );
});

test('Reset clears the visible form and returns focus to Access key', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill('draft content');
  await page.getByRole('button', { name: 'Reset' }).click();

  await expect(page.getByLabel('Access key')).toHaveValue('');
  await expect(page.getByLabel('Access key')).toBeFocused();
  await expect(page.getByLabel('Conversation template')).toHaveValue('');
  await expect(page.getByLabel('Person information')).toHaveValue('');
});

