import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('long Inbox loads as you scroll and settles without replacing neighboring rows', async ({
  page,
}) => {
  test.setTimeout(90000);
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const prefix = `Long stream ${randomUUID()}`;
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: inbox.list_id,
      tasks: Array.from({ length: 85 }, (_, index) => ({ title: `${prefix} ${index}` })),
    },
  });
  expect(response.ok()).toBeTruthy();
  await page.route('**/api/v1/inbox?limit=40*', async (route) => {
    const upstream = await route.fetch();
    const data = await upstream.json();
    for (const entry of data.entries) {
      const number = Number(entry.task.title.slice(prefix.length + 1));
      if (entry.task.title.startsWith(prefix) && number < 75) entry.created_at -= 86400;
    }
    await route.fulfill({ response: upstream, json: data });
  });
  await page.reload();
  const rows = page.locator('.inbox-task').filter({ hasText: prefix });
  await expect(rows).toHaveCount(40);
  await expect
    .poll(() =>
      rows
        .first()
        .evaluate((element) =>
          Number(element.style.getPropertyValue('--inbox-roll-scale')),
        ),
    )
    .toBe(1);
  await expect
    .poll(() =>
      rows.nth(1).evaluate((element) =>
        Number(element.style.getPropertyValue('--inbox-roll-scale')),
      ),
    )
    .toBe(1);
  await expect(page.locator('.inbox-gap').first()).toContainText(
    '1 day between captures',
  );
  const neighbor = await rows.nth(1).elementHandle();
  await rows.first().getByRole('button', { name: 'Settle' }).click();
  await expect(rows).toHaveCount(39);
  expect(await neighbor.evaluate((element) => element.isConnected)).toBe(true);
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(rows).toHaveCount(40);
  await expect(page.locator('.inbox-capture')).toBeVisible();
  await page.locator('[data-inbox-more]').scrollIntoViewIfNeeded();
  await expect(rows).toHaveCount(80, { timeout: 30000 });
  expect((await page.locator('.inbox-capture').boundingBox()).y).toBeLessThan(32);
  const topRow = rows.nth(65);
  await topRow.scrollIntoViewIfNeeded();
  await topRow.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    const toolbar = document
      .querySelector('[data-workbench-toolbar]')
      .getBoundingClientRect();
    window.scrollBy(0, bounds.top + bounds.height / 2 - toolbar.bottom - 35);
  });
  await expect
    .poll(() =>
      topRow.evaluate((element) =>
        Number(element.style.getPropertyValue('--inbox-roll-scale')),
      ),
    )
    .toBeLessThan(0.99);
  const edgeRow = rows.nth(75);
  await edgeRow.scrollIntoViewIfNeeded();
  await edgeRow.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    window.scrollBy(0, bounds.top + bounds.height / 2 - (window.innerHeight - 30));
  });
  await expect
    .poll(() =>
      edgeRow.evaluate((element) =>
        Number(element.style.getPropertyValue('--inbox-roll-scale')),
      ),
    )
    .toBeLessThan(0.99);
  const edgeScale = await edgeRow.evaluate((element) =>
    Number(element.style.getPropertyValue('--inbox-roll-scale')),
  );
  const innerScale = await rows
    .nth(74)
    .evaluate((element) => Number(element.style.getPropertyValue('--inbox-roll-scale')));
  expect(innerScale).toBeGreaterThan(edgeScale);
  expect(innerScale).toBeLessThan(1);
  if ((await rows.count()) < 85 && (await page.locator('[data-inbox-more]').count()))
    await page.locator('[data-inbox-more]').scrollIntoViewIfNeeded();
  await expect(rows).toHaveCount(85, { timeout: 30000 });
  expect(await neighbor.evaluate((element) => element.isConnected)).toBe(true);
});
