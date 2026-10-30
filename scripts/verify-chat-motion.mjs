import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const manifest = JSON.parse(
  await readFile('public/skins/reze/manifest.json', 'utf8'),
);
for (const state of [
  'idle',
  'listening',
  'thinking',
  'talking',
  'happy',
  'error',
  'sleep',
  'walk',
  'sit',
  'wave',
  'touched',
  'petting',
  'dragging',
]) {
  assert.ok(manifest.states[state]?.frames.length, `missing ${state}`);
}
assert.ok(manifest.states.walk.frames.length > 1);
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 480, height: 600 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.addInitScript(() => {
    if (!localStorage.getItem('michiyo_messages'))
      localStorage.setItem(
        'michiyo_messages',
        JSON.stringify([
          { role: 'user', content: 'Tin cũ' },
          { role: 'assistant', content: 'Trả lời cũ' },
        ]),
      );
  });
  const sprite = page.waitForResponse(
    (r) => r.url().includes('/skins/reze/spritesheet.png') && r.ok(),
  );
  await page.goto('http://127.0.0.1:1420/');
  await sprite;
  await page.locator('.companion-dock canvas').waitFor();
  // Compare the purple hair in the top of the source frames, excluding waving hands.
  const anchors = await page.evaluate(async (offsets) => {
    const image = new Image();
    image.src = '/skins/reze/spritesheet.png';
    await image.decode();
    const canvas = document.createElement('canvas');
    canvas.width = 320;
    canvas.height = 100;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    return [0, 1, 8, 9, 13].map((index) => {
      ctx.clearRect(0, 0, 320, 100);
      ctx.drawImage(
        image,
        (index % 4) * 320,
        Math.floor(index / 4) * 320,
        320,
        100,
        0,
        0,
        320,
        100,
      );
      const pixels = ctx.getImageData(0, 0, 320, 100).data;
      let sum = 0,
        count = 0;
      for (let y = 0; y < 100; y++)
        for (let x = 0; x < 320; x++) {
          const i = (y * 320 + x) * 4;
          if (
            pixels[i + 3] > 200 &&
            pixels[i + 2] > pixels[i] * 1.2 &&
            pixels[i + 2] > pixels[i + 1] * 1.2 &&
            pixels[i + 2] > 25
          ) {
            sum += x;
            count++;
          }
        }
      return sum / count + (offsets[index]?.x || 0);
    });
  }, manifest.frameOffsets);
  assert.ok(
    Math.max(...anchors) - Math.min(...anchors) < 3,
    `rest pose hair anchors diverged: ${anchors}`,
  );
  assert.equal(await page.locator('.companion-bubble').count(), 0);
  assert.equal(await page.locator('.model-badge').count(), 0);
  assert.equal(await page.locator('.msg-bubble.arriving').count(), 0);
  const field = page.getByRole('textbox', { name: 'Tin nhắn' });
  await field.fill('Dòng đầu');
  await field.press('Shift+Enter');
  await field.pressSequentially('Dòng sau');
  assert.match(await field.inputValue(), /Dòng đầu\nDòng sau/);
  assert.equal(
    await page.locator('.companion-dock').getAttribute('data-pose'),
    'listening',
  );
  await field.press('Enter');
  await page
    .getByText('Đây là bản xem trước web preview.', { exact: false })
    .waitFor();
  assert.equal(await page.locator('.user-row').count(), 2);
  await page.reload();
  await page.getByText('Dòng đầu', { exact: false }).first().waitFor();
  assert.equal(await page.locator('.msg-bubble.arriving').count(), 0);
  const initialLeft = await page
    .locator('.companion-touch-zone')
    .evaluate((el) => el.getBoundingClientRect().left);
  await page.waitForTimeout(6500);
  const finalLeft = await page
    .locator('.companion-touch-zone')
    .evaluate((el) => el.getBoundingClientRect().left);
  assert.equal(
    finalLeft,
    initialLeft,
    'mascot stays in place while chat is open',
  );
  const within = await page.evaluate(() => {
    const dock = document
      .querySelector('.companion-dock')
      .getBoundingClientRect();
    const bot = document
      .querySelector('.companion-touch-zone')
      .getBoundingClientRect();
    return bot.left >= dock.left - 1 && bot.right <= dock.right + 1;
  });
  assert.ok(within, 'companion stays inside input width');
  await page.locator('.companion-touch-zone').click();
  assert.equal(
    await page.locator('.companion-dock').getAttribute('data-pose'),
    'touched',
  );
  const reduced = await browser.newPage({
    viewport: { width: 480, height: 600 },
    reducedMotion: 'reduce',
  });
  await reduced.goto('http://127.0.0.1:1420/');
  await reduced.locator('.companion-dock.reduced').waitFor();
  assert.equal(
    await reduced.locator('.companion-dock').getAttribute('data-pose'),
    'idle',
  );
  assert.equal(
    await reduced
      .locator('.companion-touch-zone')
      .evaluate((el) => getComputedStyle(el).transitionDuration),
    '0s',
  );
  assert.deepEqual(errors, []);
  console.log(
    'PASS: Reze assets, message history and entry, compact composer, companion movement, interaction and reduced motion.',
  );
} finally {
  await browser.close();
}
