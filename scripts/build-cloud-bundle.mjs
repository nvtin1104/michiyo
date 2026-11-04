import { execSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFile,
  mkdir,
  readdir,
  readFile,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import { dirname, join, relative } from 'node:path';

const ROOT = process.cwd();
const DEST = join(ROOT, 'src-tauri', 'resources', 'cloud-backend');
const ADMIN_DIR = join(ROOT, 'admin');
const CLOUD_DIR = join(ROOT, 'cloud');
const TMP_DRY = join(CLOUD_DIR, 'artifacts', 'cloud-dry-run');

console.log('Building admin static app...');
execSync('npm run build', { cwd: ADMIN_DIR, stdio: 'inherit' });

console.log('Running wrangler deploy dry-run...');
await rm(TMP_DRY, { recursive: true, force: true });
execSync('npx wrangler deploy --dry-run --outdir ./artifacts/cloud-dry-run', {
  cwd: CLOUD_DIR,
  stdio: 'inherit',
});

await rm(DEST, { recursive: true, force: true });
await mkdir(join(DEST, 'worker'), { recursive: true });
await mkdir(join(DEST, 'assets'), { recursive: true });
await mkdir(join(DEST, 'migrations'), { recursive: true });

// 1. Copy worker files
const dryFiles = await readdir(TMP_DRY);
for (const file of dryFiles) {
  if (file.endsWith('.js')) {
    await copyFile(join(TMP_DRY, file), join(DEST, 'worker', file));
  }
}

// 2. Copy static assets recursively
async function copyDir(src, dest) {
  await mkdir(dest, { recursive: true });
  const entries = await readdir(src, { withFileTypes: true });
  for (const entry of entries) {
    const srcPath = join(src, entry.name);
    const destPath = join(dest, entry.name);
    if (entry.isDirectory()) {
      await copyDir(srcPath, destPath);
    } else {
      await copyFile(srcPath, destPath);
    }
  }
}
await copyDir(join(ADMIN_DIR, 'build'), join(DEST, 'assets'));

// 3. Copy migrations
const migrationFiles = (await readdir(join(CLOUD_DIR, 'migrations'))).sort();
for (const file of migrationFiles) {
  if (file.endsWith('.sql')) {
    await copyFile(
      join(CLOUD_DIR, 'migrations', file),
      join(DEST, 'migrations', file),
    );
  }
}

// 4. Build asset manifest for Cloudflare Direct Upload
// Asset hash = first 32 hex chars of sha256 (first 16 bytes of sha256 digest)
const assetEntries = [];
async function scanAssets(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory()) {
      await scanAssets(fullPath);
    } else {
      const buf = await readFile(fullPath);
      const hash = createHash('sha256').update(buf).digest('hex').slice(0, 32);
      const relPath =
        '/' + relative(join(DEST, 'assets'), fullPath).replace(/\\/g, '/');
      assetEntries.push({
        path: relPath,
        hash,
        size: buf.length,
        file: relative(DEST, fullPath).replace(/\\/g, '/'),
      });
    }
  }
}
await scanAssets(join(DEST, 'assets'));

// 5. Compute bundle version hash across worker, assets, migrations
const versionHasher = createHash('sha256');
const workerJs = await readFile(join(DEST, 'worker', 'index.js'));
versionHasher.update(workerJs);
for (const item of assetEntries) {
  versionHasher.update(item.path + ':' + item.hash);
}
for (const m of migrationFiles) {
  if (m.endsWith('.sql')) {
    versionHasher.update(await readFile(join(CLOUD_DIR, 'migrations', m)));
  }
}
const version = versionHasher.digest('hex');

const manifest = {
  version,
  main_module: 'index.js',
  modules: [{ name: 'index.js', type: 'application/javascript+module' }],
  compatibility_date: '2026-10-01',
  compatibility_flags: ['nodejs_compat'],
  vars: {
    ENVIRONMENT: 'production',
    DEFAULT_CHAT_MODEL: '@cf/meta/llama-3.1-8b-instruct',
    ALLOWED_CHAT_MODELS:
      '@cf/meta/llama-3.1-8b-instruct,@cf/meta/llama-3.3-70b-instruct-fp8-fast',
  },
  migrations: migrationFiles.filter((m) => m.endsWith('.sql')),
  assets: assetEntries,
  assets_config: {
    not_found_handling: 'single-page-application',
    run_worker_first: ['/api/*', '/v1/*'],
  },
};

await writeFile(
  join(DEST, 'manifest.json'),
  JSON.stringify(manifest, null, 2),
  'utf-8',
);
console.log(
  `Cloud bundle built successfully: version ${version.slice(0, 12)}, ${assetEntries.length} assets.`,
);
