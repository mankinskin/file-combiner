import { test, expect } from '@playwright/test';
import {
  token,
  templateText,
  profileText,
  pitchScriptTemplateText,
  pitchScriptProfileText,
} from './combine-shared';

test('a successful request populates the diagnostics sidebar', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  await page.getByLabel('Person information').fill(profileText);
  const generate = page.getByRole('button', { name: 'Generate guide' });
  await expect(generate).toBeInViewport();
  await expect(generate).toBeEnabled();
  await generate.click();

  await expect(page.getByRole('region', { name: 'Generated artifacts' })).toBeVisible();
  const diagnosticsRegion = page.getByRole('region', { name: 'Diagnostics' });
  await expect(diagnosticsRegion).not.toContainText('No diagnostics yet');
  await expect(diagnosticsRegion.getByText('Latent analysis completed')).toHaveCount(2);
  await expect(diagnosticsRegion.getByText('Generation completed')).toBeVisible();
});

test('a user can paste both sources and download each generated artifact', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('region', { name: 'Workflow inputs and representations' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Template analysis' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Person analysis' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Combined latent representation' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Generated artifacts' })).toBeVisible();

  const combinedBox = await page
    .getByRole('region', { name: 'Combined latent representation' })
    .boundingBox();
  const artifactsBox = await page
    .getByRole('region', { name: 'Generated artifacts' })
    .boundingBox();
  expect(combinedBox).not.toBeNull();
  expect(artifactsBox).not.toBeNull();
  expect(combinedBox!.y).toBeLessThan(artifactsBox!.y);

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

  await expect(page.getByRole('region', { name: 'Workflow inputs and representations' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Combined latent representation' })).toBeVisible();
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

  const representations = page.getByRole('region', { name: 'Workflow inputs and representations' });
  await expect(representations).toBeVisible();
  await expect(representations).toContainText('Stages');
  await expect(page.getByRole('region', { name: 'Diagnostics' })).not.toContainText('No diagnostics yet');
  await expect(page.getByRole('region', { name: 'Combined latent representation' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Generated artifacts' })).toBeVisible();
});

test('the workbench renders each artifact in place instead of only offering a download', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  const overview = page.getByLabel('closer-overview content');
  const guide = page.getByLabel('individualized-conversation-guide content');
  await expect(overview).toBeVisible();
  await expect(guide).toBeVisible();

  await expect(overview.getByRole('heading', { name: 'Closer overview for Closing conversation' })).toBeVisible();
  await expect(guide.getByRole('heading', { name: 'Individualized conversation guide for Closing conversation' })).toBeVisible();
  await expect(overview.locator('pre')).toHaveCount(0);
  await expect(guide.locator('pre')).toHaveCount(0);

  // The rendered block must carry the artifact's real text, not a placeholder.
  await expect(overview).toContainText('Closer overview for');
  await expect(guide).toContainText('Individualized conversation guide for');

  await expect(page.getByRole('button', { name: 'Copy closer-overview' })).toBeEnabled();
  await expect(
    page.getByRole('button', { name: 'Copy individualized-conversation-guide' }),
  ).toBeEnabled();
});

test('the workbench reports every pipeline stage as progress, ending completed', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  const progress = page.getByRole('status').filter({ hasText: 'Progress' });
  await expect(progress).toBeVisible();
  await expect(progress).toContainText('Analyzing conversation template');
  await expect(progress).toContainText('Analyzing person information');
  await expect(progress).toContainText('Combining representations');
  await expect(progress).toContainText('Generating artifacts');

  await expect(page.getByLabel('closer-overview content')).toBeVisible();
  await expect(progress.getByText('Generating artifacts — completed')).toBeVisible();
});

test('the workbench never exposes the session id or the access key', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  await page.getByLabel('Person information').fill(profileText);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByLabel('closer-overview content')).toBeVisible();

  const workbench = await page.locator('main').innerText();
  expect(workbench).not.toContain('session-');
  expect(workbench).not.toContain(token);
});
