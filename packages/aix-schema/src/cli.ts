import { readFileSync, statSync } from 'node:fs';
import { parseApp, validateApp } from './index.ts';
try {
  const path = process.argv[2];
  if (!path) throw new Error('Usage: npm run validate -- <app.json|app.yaml>');
  if (statSync(path).size > 1_048_576) throw new Error('App definition exceeds 1 MiB');
  const result = validateApp(parseApp(readFileSync(path, 'utf8')));
  if (!result.valid) throw new Error(result.errors.join('\n'));
  console.log(`Valid AIX App: ${path}`);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
