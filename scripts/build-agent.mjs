import { build } from 'esbuild';
import { copyFile, mkdir, stat, readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
const require = createRequire(import.meta.url);
const destination = 'src-tauri/resources/claude-agent';
await mkdir(destination, { recursive: true });
await build({
  entryPoints: ['agent/worker.ts'],
  outfile: join(destination, 'worker.mjs'),
  bundle: true,
  platform: 'node',
  target: 'node22',
  format: 'esm',
  banner: {
    js: "import { createRequire as __createRequire } from 'node:module'; const require = __createRequire(import.meta.url);",
  },
});
const pkg = `@anthropic-ai/claude-agent-sdk-${process.platform}-${process.arch}`;
const root = dirname(require.resolve(`${pkg}/package.json`));
const files = [
  [process.execPath, process.platform === 'win32' ? 'node.exe' : 'node'],
  [
    join(root, process.platform === 'win32' ? 'claude.exe' : 'claude'),
    process.platform === 'win32' ? 'claude.exe' : 'claude',
  ],
  [join(root, 'LICENSE.md'), 'CLAUDE_LICENSE.md'],
];
files.push(['agent/licenses/NODE_LICENSE', 'NODE_LICENSE']);
for (const [source, name] of files) {
  const target = join(destination, name);
  const current = await stat(target).catch(() => null),
    original = await stat(source);
  if (
    !current ||
    current.size !== original.size ||
    current.mtimeMs < original.mtimeMs
  )
    await copyFile(source, target);
}
const metadata = JSON.parse(
  await readFile(
    join(
      dirname(require.resolve('@anthropic-ai/claude-agent-sdk')),
      'package.json',
    ),
    'utf8',
  ),
);
console.log(
  `Claude Agent SDK ${metadata.version}: backend worker and native runtime prepared.`,
);
