import fs from 'node:fs';
import path from 'node:path';

export const token = process.env.COMBINE_TOKEN ?? '';

export const templateText = `# Closing conversation

## Opening
Are you ready to begin?

## Offer
Explain the next steps clearly.`;

export const profileText = `# Customer profile

## Goal
Build a repeatable business.

## Situation
Already operates a small hospitality business.`;

export const pitchScriptTemplateText = fs
  .readFileSync(path.join(__dirname, 'fixtures', 'pitch-script-template.md'), 'utf-8')
  .replace(/\r\n/g, '\n');
export const pitchScriptProfileText = fs
  .readFileSync(path.join(__dirname, 'fixtures', 'pitch-script-profile.md'), 'utf-8')
  .replace(/\r\n/g, '\n');

// A template with more headings than the schema allows deterministically
// fails real backend validation, without mocking any network call.
export const tooManyStagesTemplate = Array.from(
  { length: 40 },
  (_, index) => `# Stage ${index + 1}`,
).join('\n');

export const liveRun = process.env.E2E_LIVE === '1';

export function redactUrl(value: string): string {
  try {
    const url = new URL(value);
    return `${url.origin}${url.pathname}`;
  } catch {
    return '[redacted-invalid-url]';
  }
}

export async function saveLiveEvidence(
  page: import('@playwright/test').Page,
  metrics: Record<string, unknown>,
  consoleErrors: string[],
  apiErrors: string[],
  artifacts: Record<string, string>,
) {
  const evidenceDirectory = path.join(__dirname, 'review-bundle');
  fs.mkdirSync(evidenceDirectory, { recursive: true });
  fs.writeFileSync(
    path.join(evidenceDirectory, 'metrics.json'),
    JSON.stringify(metrics, null, 2) + '\n',
  );
  fs.writeFileSync(
    path.join(evidenceDirectory, 'console-api-summary.json'),
    JSON.stringify(
      {
        console_errors: consoleErrors,
        api_errors: apiErrors.map(entry => ({ status: entry.split(' ')[0], url: redactUrl(entry.slice(entry.indexOf(' ') + 1)) })),
      },
      null,
      2,
    ) + '\n',
  );
  fs.writeFileSync(path.join(evidenceDirectory, 'artifacts.json'), JSON.stringify(artifacts, null, 2) + '\n');
  await page.screenshot({ path: path.join(evidenceDirectory, 'workbench.png'), fullPage: true });
}
