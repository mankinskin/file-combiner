import { test, expect } from '@playwright/test';
import { token, templateText, profileText, tooManyStagesTemplate } from './combine-shared';

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

test('a failed stage preserves the source text and the representations already produced', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByLabel('Access key').fill(token);
  await page.getByLabel('Conversation template').fill(templateText);
  // The profile fails backend validation while the template analysis succeeds,
  // so the completed template block must survive the adjacent failure.
  await page.getByLabel('Person information').fill(tooManyStagesTemplate);
  await page.getByRole('button', { name: 'Generate guide' }).click();

  await expect(page.getByRole('alert')).toBeVisible();

  await expect(page.getByRole('region', { name: 'Latent representations' })).toBeVisible();
  await expect(page.getByLabel('Conversation template')).toHaveValue(templateText);
  await expect(page.getByLabel('Person information')).toHaveValue(tooManyStagesTemplate);
});
