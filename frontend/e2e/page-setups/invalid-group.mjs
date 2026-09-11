import { openFixturePage } from './setup-page.mjs';

await openFixturePage({
  accessMethod: 'group',
  template: '# Closing conversation\n\n## Opening\nAre you ready to begin?',
  profile: '# Customer profile\n\n## Goal\nBuild a repeatable business.',
  submit: true,
});