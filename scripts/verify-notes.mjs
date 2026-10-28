import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';

const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 1000, height: 720 },
  });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.addInitScript(() => {
    let notes = [],
      next = 0,
      callback = 0;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { label: 'main' },
      },
      transformCallback() {
        return ++callback;
      },
      unregisterCallback() {},
      async invoke(command, args = {}) {
        if (command === 'plugin:store|load') return 1;
        if (command === 'plugin:store|get') return [null, false];
        if (command === 'plugin:event|listen') return 1;
        if (command === 'notes_list')
          return notes.filter((note) =>
            (note.title + note.body)
              .toLowerCase()
              .includes(args.query.toLowerCase()),
          );
        if (command === 'notes_save') {
          const input = args.input;
          const note = {
            ...input,
            id: input.id || String(++next),
            rev: (input.rev || 0) + 1,
            updated_at: 1,
            deleted_at: null,
            device_id: 'test',
          };
          notes = [note, ...notes.filter((item) => item.id !== note.id)];
          return note;
        }
        if (command === 'notes_delete') {
          notes = notes.filter((note) => note.id !== args.id);
          return null;
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=main#/notes');
  await page.getByRole('button', { name: '+ Ghi chú mới' }).click();
  await page.getByLabel('Tiêu đề', { exact: true }).fill('Nhật ký');
  await page
    .getByLabel('Nội dung Markdown')
    .fill('**Chào bạn** <img src=x onerror=alert(1)>');
  await page.getByLabel('Thẻ', { exact: true }).fill('cá nhân, hôm nay');
  await page.getByRole('checkbox', { name: 'Ghim ghi chú' }).check();
  await page.getByRole('button', { name: 'Lưu', exact: true }).click();
  await page.getByText('Chào bạn', { exact: true }).waitFor();
  assert.equal(await page.locator('.note-detail img').count(), 0);
  assert.equal(await page.getByText('cá nhân · hôm nay').count(), 1);
  await page.getByRole('button', { name: 'Sửa', exact: true }).click();
  await page.getByLabel('Tiêu đề', { exact: true }).fill('Bản nháp');
  await page
    .getByRole('navigation', { name: 'Điều hướng chính' })
    .getByRole('button', { name: 'Dịch', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Điều hướng chính' })
    .getByRole('button', { name: 'Ghi chú', exact: true })
    .click();
  assert.equal(
    await page.getByLabel('Tiêu đề', { exact: true }).inputValue(),
    'Bản nháp',
  );
  await page.getByRole('button', { name: 'Lưu', exact: true }).click();
  await page.getByRole('heading', { name: 'Bản nháp' }).waitFor();
  await page.getByRole('button', { name: 'Xóa', exact: true }).click();
  await page.getByText('Chưa có ghi chú phù hợp.').waitFor();
  assert.deepEqual(errors, []);
  console.log(
    'PASS: notes create, edit, preserve draft, sanitize Markdown, pin and delete (mock IPC).',
  );
} finally {
  await browser.close();
}
