import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { openFixturePage } from './setup-page.mjs';

const directory = path.dirname(fileURLToPath(import.meta.url));
const fixtureDirectory = path.join(directory, '..', 'fixtures');

await openFixturePage({
  template: fs.readFileSync(path.join(fixtureDirectory, 'pitch-script-template.md'), 'utf8'),
  profile: fs.readFileSync(path.join(fixtureDirectory, 'pitch-script-profile.md'), 'utf8'),
  submit: true,
});