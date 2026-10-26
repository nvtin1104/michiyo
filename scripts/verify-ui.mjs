import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
const browser = await chromium.launch({
  executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe',
  headless: true,
});
const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
await page.goto('http://127.0.0.1:1420/');
await page.getByRole('heading', { name: 'Trò chuyện' }).waitFor();
await page.getByRole('button', { name: 'Giúp tôi viết một email' }).click();
assert.match(
  await page.getByRole('textbox', { name: 'Tin nhắn' }).inputValue(),
  /email/,
);
await page.getByRole('button', { name: 'Cài đặt', exact: true }).click();
await page.locator('nav button').filter({ hasText: 'API' }).click();
assert.equal(await page.locator('input[type=password]').count(), 3);
await page
  .locator('input[type=password]')
  .first()
  .fill('test-only-not-a-real-key');
await page.locator('nav button').filter({ hasText: 'Phím tắt' }).click();
assert.equal(
  await page.getByLabel('Dịch văn bản đã chọn').inputValue(),
  'Ctrl+Alt+T',
);
await mkdir('artifacts', { recursive: true });
await page.screenshot({ path: 'artifacts/settings.png', fullPage: true });
await page.locator('nav button').filter({ hasText: 'Chung' }).click();
await page.getByRole('combobox', { name: 'Ngôn ngữ giao diện' }).click();
await page.getByRole('option', { name: 'English', exact: true }).click();
await page.getByRole('heading', { name: /^Settings/ }).waitFor();
assert.equal(
  await page.evaluate(() =>
    localStorage
      .getItem('preview-config')
      ?.includes('test-only-not-a-real-key'),
  ),
  false,
);
await page.reload();
await page.getByRole('button', { name: 'Settings', exact: true }).click();
await page.getByRole('heading', { name: /^Settings/ }).waitFor();
assert.match(
  await page
    .getByRole('combobox', { name: 'Interface language' })
    .textContent(),
  /English/,
);
await page.getByRole('combobox', { name: 'Interface language' }).click();
await page.getByRole('option', { name: 'Tiếng Việt', exact: true }).click();
await page.getByRole('heading', { name: /^Cài đặt/ }).waitFor();
await page
  .locator('.preview-bar')
  .getByRole('button', { name: 'Dịch', exact: true })
  .click();
await page.getByRole('textbox', { name: 'Văn bản cần dịch' }).fill('Hello');
await page.getByRole('button', { name: 'Dịch ↗', exact: true }).click();
await page.getByRole('alert').waitFor();
assert.match(await page.getByRole('alert').textContent(), /desktop/);
await page.goto('http://127.0.0.1:1420/?view=mascot');
await page.locator('canvas').waitFor();
await page.waitForTimeout(600);
await page.screenshot({ path: 'artifacts/mascot.png' });
assert.deepEqual(errors, []);
console.log(
  'UI checks passed: chat input, settings tabs, secret inputs, shortcut defaults, desktop-only boundary, Pixi render.',
);
await browser.close();
