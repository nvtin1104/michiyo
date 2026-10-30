import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1100, height: 800 } });
  const errors = [];
  const writes = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.route('**/api/**', async (route) => {
    const req = route.request(), path = new URL(req.url()).pathname;
    if (req.method() !== 'GET') writes.push({ path, body: req.postDataJSON() });
    const data = path.endsWith('/devices') ? { devices: [] } :
      path.endsWith('/api-keys') && req.method() === 'POST' ? { key: 'mk_test_only' } :
      path.endsWith('/api-keys') ? { api_keys: [] } :
      path.endsWith('/provider-keys') ? { provider_keys: [] } :
      path.endsWith('/usage') ? { usage: [] } :
      path.endsWith('/telegram/summary') ? { statuses: [{ status: 'sent', count: 3 }] } :
      path.endsWith('/telegram/issues') ? { issues: [] } :
      path.endsWith('/telegram/config') && req.method() === 'PUT' ? { config: req.postDataJSON() } :
      path.endsWith('/telegram/config') ? {
        config: {
          enabled: true,
          allowed_chat_ids: ['12345'],
          system_prompt: 'Bạn là Michiyo, trợ lý thân thiện và ngắn gọn.',
          model: '@cf/meta/llama-3.1-8b-instruct',
          hourly_limit: 20,
          allow_workspace_queries: true,
        },
        environment: {
          token_configured: true,
          webhook_secret_configured: true,
          queue_configured: true,
          allowed_models: ['@cf/meta/llama-3.1-8b-instruct'],
        },
      } :
      path.endsWith('/sync/records') ? { records: [{ id: 'note-1', title: 'Ghi chú thử' }], cursor: '1', more: false } :
      path.endsWith('/pair/start') ? { code: 'TEST2345', expires_at: Date.now() + 600000 } : { ok: true };
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(data) });
  });
  const loaded = page.waitForResponse((response) => response.url().endsWith('/api/admin/devices'));
  await page.goto('http://127.0.0.1:4174/');
  await loaded;
  await page.getByLabel('Tên thiết bị').fill('Laptop');
  await page.getByRole('button', { name: 'Tạo mã' }).click();
  await page.getByText('TEST2345').waitFor();
  assert.equal(writes[0].body.name, 'Laptop');
  await page.getByRole('button', { name: 'API', exact: true }).click();
  await page.getByLabel('Tên khóa').fill('client');
  await page.getByLabel('Hạn mức mỗi ngày (để trống nếu không giới hạn)').fill('5');
  await page.getByRole('button', { name: 'Tạo khóa' }).click();
  await page.getByText('mk_test_only').waitFor();
  assert.deepEqual(writes.at(-1).body, { name: 'client', scopes: ['ai:chat'], daily_limit: 5 });
  await page.getByRole('button', { name: 'Đã lưu' }).click();
  assert.equal(await page.getByText('mk_test_only').count(), 0);
  await page.getByLabel('Khóa mới').fill('test-provider-secret');
  await page.getByRole('button', { name: 'Lưu khóa' }).click();
  await page.getByText('Đã lưu khóa nhà cung cấp.').waitFor();
  assert.equal(await page.getByLabel('Khóa mới').inputValue(), '');
  await page.getByRole('button', { name: 'Dữ liệu', exact: true }).click();
  await page.getByText(/Ghi chú thử/).waitFor();
  const downloaded = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Xuất JSON' }).click();
  const download = await downloaded;
  assert.equal(download.suggestedFilename(), 'michiyo-note.json');
  assert.deepEqual(JSON.parse(await readFile(await download.path(), 'utf8')), { type: 'note', records: [{ id: 'note-1', title: 'Ghi chú thử' }] });

  // Test Telegram Bot configuration tab
  await page.getByRole('button', { name: 'Telegram', exact: true }).click();
  await page.getByLabel('Chat ID được phép (phân cách bằng dấu phẩy)').fill('12345, 67890');
  await page.getByRole('button', { name: 'Lưu cấu hình Bot' }).click();
  await page.getByText('Đã lưu cấu hình Telegram Bot.').waitFor();
  assert.deepEqual(writes.at(-1).body.allowed_chat_ids, ['12345', '67890']);

  await page.setViewportSize({ width: 390, height: 800 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  assert.deepEqual(errors, []);
  console.log('PASS: admin pairing, one-time API key, provider key clearing, Telegram bot config and mobile layout (mock API).');
} finally { await browser.close(); }
