import { openFixturePage } from './setup-page.mjs';

const template = Array.from({ length: 40 }, (_, index) => `# Stage ${index + 1}`).join('\n');

await openFixturePage({
  template,
  profile: '# Customer profile\n\n## Goal\nBuild a repeatable business.',
  submit: true,
});