import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({ serviceWorkers: 'block' });

async function fixture(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const api = async (path, method = 'GET', data) => {
    const response = await page.request.fetch(`/api/v1${path}`, {
      method,
      data,
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
    });
    expect(response.ok(), await response.text()).toBe(true);
    return response.json();
  };
  const space = await api('/workspaces', 'POST', { title: `Stable ${randomUUID()}` });
  const list = await api('/lists', 'POST', {
    workspace_id: space.id,
    title: 'Stable colored list',
    color: '#17ac6e',
    emoji: '❤️',
  });
  await api('/favorites', 'PUT', { kind: 'list', id: list.id, favorite: true });
  return { api, list, space };
}
async function holdScripts(page, url) {
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  const handler = async (route) => {
    await gate;
    await route.continue();
  };
  await page.route(/\/(?:static|assets)\/.*\.js$/, handler);
  await page.goto(url, { waitUntil: 'commit' });
  await expect(page.locator('#main')).toBeVisible();
  await page.waitForFunction(() =>
    getComputedStyle(document.body)
      .getPropertyValue('--font-sans')
      .includes('Todoer Numeric'),
  );
  // Firefox's FontFaceSet.ready also waits for DOMContentLoaded, which the
  // deliberately stalled deferred scripts prevent. Load the faces directly.
  await page.evaluate(() =>
    Promise.all([
      document.fonts.load('16px "Geist"', 'Stable'),
      document.fonts.load('16px "Todoer Numeric"', '0123456789'),
    ]),
  );
  return async () => {
    release();
    await page.waitForLoadState('domcontentloaded');
    await page.unroute(/\/(?:static|assets)\/.*\.js$/, handler);
  };
}
async function boxes(page, selectors) {
  return page.evaluate(
    (selectors) =>
      selectors.map((selector) => {
        const el = document.querySelector(selector);
        const { x, y, width, height } = el.getBoundingClientRect();
        return { x, y, width, height };
      }),
    selectors,
  );
}
function stable(before, after) {
  before.forEach((rect, index) =>
    Object.keys(rect).forEach((key) => {
      expect(
        Math.abs(rect[key] - after[index][key]),
        `element ${index} ${key}`,
      ).toBeLessThanOrEqual(1);
    }),
  );
}

test('list heading, controls, logo and sidebar colors are final before scripts arrive', async ({
  page,
}) => {
  const { list } = await fixture(page);
  const release = await holdScripts(page, `/lists/${list.id}`);
  const selectors = [
    '.page-heading h1',
    '#capture-open',
    '[data-section-add-open]',
    '.todoer-logo-pro',
  ];
  await expect(page.locator('#capture-open')).toBeVisible();
  const badges = page.locator(`#navigation a[href="/lists/${list.id}"] .list-dot`);
  expect(await badges.count()).toBe(2);
  for (const badge of await badges.all())
    await expect(badge).toHaveCSS('background-color', 'rgb(23, 172, 110)');
  const before = await boxes(page, selectors);
  await release();
  await expect(page.locator('.heading-list-color')).toBeVisible();
  stable(before, await boxes(page, selectors));
  for (const badge of await badges.all())
    await expect(badge).toHaveCSS('background-color', 'rgb(23, 172, 110)');
});

test('space card color and emoji are ready before scripts arrive', async ({ page }) => {
  const { list, space } = await fixture(page);
  const release = await holdScripts(page, `/space/${space.id}`);
  const dot = page
    .locator(`.space-list-card:has(a[href="/lists/${list.id}"]) .list-dot`)
    .first();
  await expect(dot).toHaveCSS('background-color', 'rgb(23, 172, 110)');
  const emoji = dot.locator('.list-emoji');
  const rect = await dot.boundingBox();
  const glyph = await emoji.boundingBox();
  expect(glyph.width).toBe(rect.width);
  expect(glyph.height).toBe(rect.height);
  expect(glyph.x).toBe(rect.x);
  expect(glyph.y).toBe(rect.y);
  await expect(dot.locator('..').locator('..')).toHaveCSS('--list-color', '#17ac6e');
  const before = await dot.boundingBox();
  await release();
  await page.evaluate(async () => {
    const base = document.querySelector('meta[name=asset-base]').content;
    await import(base + 'space-overview.js');
  });
  stable([before], [await dot.boundingBox()]);
  await expect(dot).toHaveCSS('background-color', 'rgb(23, 172, 110)');
});

test('task summary, nested children, progress and discussion keep their initial geometry', async ({
  page,
}) => {
  const { api, list } = await fixture(page);
  const capture = async (title, parent_id = null) =>
    (
      await api('/capture', 'POST', {
        list_id: list.id,
        tasks: [{ title, description: '', parent_id }],
      })
    )[0];
  const task = await capture('Stable task');
  const child = await capture('First subtask', task.id);
  await capture('Nested subtask', child.id);
  await api(`/tasks/${task.id}/comments`, 'POST', { body: 'Already present discussion' });
  const schedule = await api(`/tasks/${task.id}/schedule`);
  await api(`/tasks/${task.id}/schedule`, 'PUT', {
    revision: schedule.task_revision,
    schedule: {
      ...schedule.schedule,
      priority: 1,
      planned: { kind: 'date', date: '2026-11-20' },
    },
  });
  const release = await holdScripts(page, `/tasks/${task.id}`);
  const selectors = [
    '.task-page-heading',
    '[data-task-due]',
    '[data-subtask-completion-line]',
    '[data-children]',
    '[data-child-composer]',
    '[data-discussion]',
    '[data-comment-composer]',
  ];
  for (const selector of selectors) await expect(page.locator(selector)).toBeVisible();
  await expect(page.locator('[data-child-progress]')).toHaveText('0/1 tasks complete');
  await expect(page.locator('[data-children] > li > ol > li')).toHaveCount(1);
  const before = await boxes(page, selectors);
  let redundant = 0;
  page.on('request', (request) => {
    if (/\/subtree\/query$|\/schedule$|\/discussion$/.test(request.url())) redundant++;
  });
  await release();
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  stable(before, await boxes(page, selectors));
  expect(redundant).toBe(0);
});

test('capture fits narrow screens and long input without horizontal scrolling', async ({
  page,
}) => {
  await fixture(page);
  await page.evaluate(async () => {
    const base = document.querySelector('meta[name=asset-base]').content;
    await (await import(base + 'quick-capture.js')).openCapture();
  });
  const modal = page.locator('dialog.capture-omnibar:has([name=title])');
  await expect(modal).toBeVisible();
  await modal.locator('[name=title]').fill('LongTitle'.repeat(50));
  for (const width of [320, 360, 390, 768, 1280]) {
    await page.setViewportSize({ width, height: 844 });
    const size = await modal.evaluate((el) => ({
      scroll: el.scrollWidth,
      client: el.clientWidth,
      left: el.getBoundingClientRect().left,
      right: el.getBoundingClientRect().right,
      viewport: innerWidth,
    }));
    expect(size.scroll, `${width}px overflow`).toBeLessThanOrEqual(size.client);
    expect(size.left).toBeGreaterThanOrEqual(0);
    expect(size.right).toBeLessThanOrEqual(size.viewport);
  }
});
