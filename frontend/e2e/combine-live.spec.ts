import { test, expect } from '@playwright/test';
import fs from 'node:fs';
import {
  token,
  pitchScriptTemplateText,
  pitchScriptProfileText,
  liveRun,
  redactUrl,
  saveLiveEvidence,
} from './combine-shared';

test.describe('credentialed live evidence', () => {
  test.skip(!liveRun, 'set E2E_LIVE=1 to run credentialed live evidence');

  test('@live renders and verifies both model-generated artifacts', async ({ page }) => {
    test.skip(!token, 'COMBINE_TOKEN must contain the ephemeral gh auth token');
    const startedAt = Date.now();
    const stageTimes: Record<string, number> = {};
    const consoleErrors: string[] = [];
    const apiErrors: string[] = [];
    page.on('console', message => {
      if (message.type() === 'error') consoleErrors.push(message.text());
    });
    page.on('response', response => {
      if (response.status() >= 400) {
        apiErrors.push(`${response.status()} ${redactUrl(response.url())}`);
      }
    });

    await page.goto('/');
    await page.getByLabel('Access method').selectOption('direct-pat');
    await page.getByLabel('Copilot personal access token').fill(token);
    await page.getByLabel('Conversation template').fill(pitchScriptTemplateText);
    await page.getByLabel('Person information').fill(pitchScriptProfileText);
    const submittedAt = Date.now();
    await page.getByRole('button', { name: 'Generate guide' }).click();

    const progress = page.getByRole('status').filter({ hasText: 'Progress' });
    await expect(progress).toContainText('Analyzing conversation template');
    stageTimes.analysis_started_ms = Date.now() - submittedAt;
    await expect(page.getByRole('region', { name: 'Latent representations' })).toBeVisible({
      timeout: 120_000,
    });
    stageTimes.analysis_completed_ms = Date.now() - submittedAt;
    await expect(progress).toContainText('Combining representations', { timeout: 120_000 });
    stageTimes.combination_completed_ms = Date.now() - submittedAt;
    await expect(page.getByRole('region', { name: 'Generated artifacts' })).toBeVisible({
      timeout: 120_000,
    });
    stageTimes.generation_completed_ms = Date.now() - submittedAt;

    const overview = page.getByLabel('closer-overview content');
    const guide = page.getByLabel('individualized-conversation-guide content');
    await expect(overview).toContainText('Closer overview for');
    await expect(guide).toContainText('Individualized conversation guide for');
    await expect(overview).toContainText('Source fact');
    await expect(guide).toContainText('Source fact');

    const [overviewDownload] = await Promise.all([
      page.waitForEvent('download'),
      page.getByRole('button', { name: 'Download closer-overview' }).click(),
    ]);
    const [guideDownload] = await Promise.all([
      page.waitForEvent('download'),
      page.getByRole('button', { name: 'Download individualized-conversation-guide' }).click(),
    ]);
    expect(overviewDownload.suggestedFilename()).toBe('closer-overview.md');
    expect(guideDownload.suggestedFilename()).toBe('individualized-conversation-guide.md');
    const overviewPath = await overviewDownload.path();
    const guidePath = await guideDownload.path();
    expect(overviewPath).toBeTruthy();
    expect(guidePath).toBeTruthy();
    const overviewText = fs.readFileSync(overviewPath!, 'utf8');
    const guideText = fs.readFileSync(guidePath!, 'utf8');
    expect(overviewText).toContain('Closer overview for');
    expect(guideText).toContain('Individualized conversation guide for');

    const diagnostics = await page.getByRole('region', { name: 'Diagnostics' }).innerText();
    expect(diagnostics).toContain('combine.combination_execution.succeeded');
    expect(diagnostics).toContain('combine.generation_execution.succeeded');

    await saveLiveEvidence(
      page,
      {
        recorded_at: new Date().toISOString(),
        total_latency_ms: Date.now() - startedAt,
        stages: stageTimes,
      },
      consoleErrors,
      apiErrors,
      {
        closer_overview: overviewText,
        individualized_conversation_guide: guideText,
      },
    );
    expect(apiErrors).toEqual([]);
  });
});
