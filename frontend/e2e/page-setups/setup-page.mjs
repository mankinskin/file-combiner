import { chromium } from '@playwright/test';

const baseUrl = process.env.BASE_URL ?? 'http://127.0.0.1:8080';
const token = process.env.COMBINE_TOKEN ?? 'file-combiner-local-token';

export async function openFixturePage({ accessMethod = 'shared-key', template, profile, submit = false }) {
  const browser = await chromium.launch({ headless: false });
  const page = await browser.newPage();
  await page.goto(baseUrl);

  if (accessMethod === 'group') {
    await page.getByLabel('Access method').selectOption('group');
    await page.getByLabel('Group name').fill('invalid-group');
    await page.getByLabel('Group password').fill('invalid-password');
  } else {
    await page.getByLabel('Access key').fill(token);
  }

  await page.getByLabel('Conversation template').fill(template);
  await page.getByLabel('Person information').fill(profile);
  if (submit) {
    await page.getByRole('button', { name: 'Generate guide' }).click();
  }

  console.log(`Fixture page is ready at ${baseUrl}`);
  console.log('Press Ctrl+C to close the fixture browser.');
  await new Promise(() => {});
}