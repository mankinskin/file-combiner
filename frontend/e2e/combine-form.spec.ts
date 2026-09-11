import { test, expect } from '@playwright/test';
import { token, templateText, profileText } from './combine-shared';

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
