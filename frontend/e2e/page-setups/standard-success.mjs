import { openFixturePage } from './setup-page.mjs';

await openFixturePage({
  template: '# Closing conversation\n\n## Opening\nAre you ready to begin?\n\n## Offer\nExplain the next steps clearly.',
  profile: '# Customer profile\n\n## Goal\nBuild a repeatable business.\n\n## Situation\nAlready operates a small hospitality business.',
  submit: true,
});