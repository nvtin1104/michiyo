import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';

const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 720, height: 540 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.addInitScript(() => {
    let id = 0;
    const data = new Map();
    window.__saved = [];
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'settings' },
        currentWebview: { label: 'settings' },
      },
      transformCallback() {
        return ++id;
      },
      unregisterCallback() {},
      async invoke(command, args = {}) {
        if (command === 'plugin:store|load') return 1;
        if (command === 'plugin:store|get')
          return [data.get(args.key), data.has(args.key)];
        if (command === 'plugin:store|set') {
          data.set(args.key, args.value);
          window.__saved.push(args.value);
        }
        if (command === 'plugin:event|listen') return ++id;
        if (command === 'secret_status') return { cloud: !!window.__pair };
        if (command === 'cloud_pair') {
          window.__pair = args;
          return 'device-1';
        }
        if (command === 'list_skins')
          return { names: ['default'], directory: 'C:/example/skins' };
        if (command === 'ai_models') {
          window.__target = args.target;
          return ['llama3', 'phi3'];
        }
        if (command === 'local_status') {
          return {
            online: true,
            baseUrl: 'http://127.0.0.1:11434',
            version: '0.6.2',
            message: null,
            installedModels: window.__pulledGemma
              ? [
                  {
                    name: 'gemma3:4b',
                    sizeBytes: 3300000000,
                    modifiedAt: '2026-10-30T10:00:00Z',
                    family: 'gemma3',
                    parameterSize: '4.3B',
                    quantization: 'Q4_K_M',
                  },
                ]
              : [],
            activePull: null,
            recommendedModels: [],
          };
        }
        if (command === 'local_pull_model') {
          window.__pulledModel = args.model;
          window.__pulledGemma = true;
          return {
            model: args.model,
            status: 'completed',
            phase: 'Đã cài xong',
            completedBytes: 3300000000,
            totalBytes: 3300000000,
            percent: 100,
            error: null,
          };
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=settings');
  await page.getByRole('heading', { name: 'Cài đặt', exact: true }).waitFor();
  await page.locator('nav button').filter({ hasText: 'AI' }).click();

  const mode = page.getByRole('combobox', { name: 'Chế độ', exact: true });
  await mode.press('ArrowDown');
  await page.getByRole('option', { name: /Offline/ }).click();

  // Offline without a local model is rejected and not persisted.
  await page.getByText('Chế độ Offline cần model chạy trên máy này').waitFor();
  assert.equal(await page.evaluate(() => window.__saved.length), 0);

  await page.getByLabel('Model cục bộ').fill('llama3');
  await page.getByLabel('Model cục bộ').press('Tab');
  await page.getByText('Đã lưu thay đổi').waitFor();
  const saved = await page.evaluate(() => window.__saved.at(-1));
  assert.equal(saved.mode, 'offline');
  assert.equal(saved.localModel, 'llama3');

  await page.getByRole('button', { name: 'Kiểm tra kết nối' }).click();
  await page.getByText('Tìm thấy 2 model').waitFor();
  assert.equal(await page.evaluate(() => window.__target), 'local');
  await page.getByText('● Đang chạy (v0.6.2)').waitFor();

  // Pull recommended Google Gemma model (gemma3:4b) and select it
  const gemmaRow = page
    .locator('.local-model-row')
    .filter({ hasText: 'gemma3:4b' });
  await gemmaRow.getByRole('button', { name: 'Tải về' }).click();
  await page.getByText('Đã cài xong gemma3:4b').waitFor();
  assert.equal(await page.evaluate(() => window.__pulledModel), 'gemma3:4b');
  await gemmaRow.getByRole('button', { name: 'Chọn dùng' }).click();
  await page.getByText('Đã lưu thay đổi').waitFor();
  const savedAfterSelect = await page.evaluate(() => window.__saved.at(-1));
  assert.equal(savedAfterSelect.localModel, 'gemma3:4b');

  await page.getByRole('tab', { name: 'Model Cloudflare' }).click();
  await page.getByLabel('Địa chỉ Michiyo Cloud').fill('http://127.0.0.1:8787');
  await page.getByLabel('Địa chỉ Michiyo Cloud').press('Tab');
  await page.getByLabel('Mã ghép nối').fill('ABCD2345');
  await page.getByRole('button', { name: 'Ghép nối' }).click();
  await page.getByText('Đã ghép nối với Michiyo Cloud').waitFor();
  assert.deepEqual(await page.evaluate(() => window.__pair), {
    url: 'http://127.0.0.1:8787',
    code: 'ABCD2345',
  });
  await page.getByText('● Đã ghép nối').waitFor();

  await page.getByRole('tab', { name: /Model ngoài/ }).click();
  await page.getByRole('button', { name: 'OpenAI-compatible' }).click();
  await page.getByLabel('Địa chỉ API').waitFor();

  await page.getByRole('tab', { name: /Google Local/ }).click();
  await page.getByLabel('Địa chỉ cục bộ').fill('http://example.com/v1');
  await page.getByLabel('Địa chỉ cục bộ').press('Tab');
  await page.getByText('cần HTTPS hoặc localhost').waitFor();
  assert.deepEqual(errors, []);
  console.log(
    'PASS: AI settings enforce Offline needs a local model, save local config, list models and reject plain-HTTP remote hosts (mock IPC).',
  );
} finally {
  await browser.close();
}
