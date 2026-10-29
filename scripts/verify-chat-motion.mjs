import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const manifest = JSON.parse(await readFile('public/skins/reze/manifest.json', 'utf8'));
for (const state of ['idle', 'listening', 'thinking', 'talking', 'happy', 'error', 'sleep', 'walk', 'sit', 'wave', 'touched', 'petting', 'dragging']) {
  assert.ok(manifest.states[state]?.frames.length, `missing ${state}`);
}
assert.ok(manifest.states.walk.frames.length > 1);
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 480, height: 600 } });
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(() => { if (!localStorage.getItem('michiyo_messages')) localStorage.setItem('michiyo_messages', JSON.stringify([{ role: 'user', content: 'Tin cũ' }, { role: 'assistant', content: 'Trả lời cũ' }])); });
  const sprite = page.waitForResponse(r => r.url().includes('/skins/reze/spritesheet.png') && r.ok());
  await page.goto('http://127.0.0.1:1420/');
  await sprite;
  await page.locator('.companion-dock canvas').waitFor();
  assert.equal(await page.locator('.companion-bubble').count(), 0);
  assert.equal(await page.locator('.model-badge').count(), 0);
  assert.equal(await page.locator('.msg-bubble.arriving').count(), 0);
  const field = page.getByRole('textbox', { name: 'Tin nhắn' });
  await field.fill('Dòng đầu');
  await field.press('Shift+Enter');
  await field.pressSequentially('Dòng sau');
  assert.match(await field.inputValue(), /Dòng đầu\nDòng sau/);
  assert.equal(await page.locator('.companion-dock').getAttribute('data-pose'), 'listening');
  await field.press('Enter');
  await page.getByText('Đây là bản xem trước web preview.', { exact: false }).waitFor();
  assert.equal(await page.locator('.user-row').count(), 2);
  await page.reload();
  await page.getByText('Dòng đầu', { exact: false }).first().waitFor();
  assert.equal(await page.locator('.msg-bubble.arriving').count(), 0);
  await page.waitForFunction(() => document.querySelector('.companion-dock')?.dataset.pose === 'walk', null, { timeout: 22000 });
  await page.waitForTimeout(5600);
  const within = await page.evaluate(() => {
    const dock = document.querySelector('.companion-dock').getBoundingClientRect();
    const bot = document.querySelector('.companion-touch-zone').getBoundingClientRect();
    return bot.left >= dock.left - 1 && bot.right <= dock.right + 1;
  });
  assert.ok(within, 'companion stays inside input width');
  await page.locator('.companion-touch-zone').click();
  assert.equal(await page.locator('.companion-dock').getAttribute('data-pose'), 'touched');
  const reduced = await browser.newPage({ viewport: { width: 480, height: 600 }, reducedMotion: 'reduce' });
  await reduced.goto('http://127.0.0.1:1420/');
  await reduced.locator('.companion-dock.reduced').waitFor();
  assert.equal(await reduced.locator('.companion-dock').getAttribute('data-pose'), 'idle');
  assert.equal(await reduced.locator('.companion-touch-zone').evaluate(el => getComputedStyle(el).transitionDuration), '0s');
  assert.deepEqual(errors, []);
  console.log('PASS: Reze assets, message history and entry, compact composer, companion movement, interaction and reduced motion.');
} finally {
  await browser.close();
}
