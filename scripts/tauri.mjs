import { run } from '@tauri-apps/cli';
import { join, delimiter } from 'node:path';
import { homedir } from 'node:os';
// rustup may have been installed without changing the user's shell profile.
process.env.PATH =
  join(homedir(), '.cargo', 'bin') + delimiter + (process.env.PATH || '');
await run(process.argv.slice(2), 'tauri');
