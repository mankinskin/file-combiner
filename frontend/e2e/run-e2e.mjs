import { spawnSync } from 'node:child_process';

const args = process.argv.slice(2);

function takeArg(flag) {
  const index = args.indexOf(flag);
  if (index === -1) {
    return undefined;
  }
  const value = args[index + 1];
  args.splice(index, 2);
  return value;
}

const baseUrl = takeArg('--base-url');
const token = takeArg('--token');
if (baseUrl) {
  process.env.BASE_URL = baseUrl;
}
if (token) {
  process.env.COMBINE_TOKEN = token;
}

const result = spawnSync('npx', ['playwright', 'test', ...args], {
  stdio: 'inherit',
  shell: true,
});
process.exit(result.status ?? 1);
