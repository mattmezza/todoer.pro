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
  await page.locator('[data-inbox-more]').scrollIntoViewIfNeeded();
  await expect(rows).toHaveCount(85, { timeout: 30000 });
  expect(await neighbor.evaluate((element) => element.isConnected)).toBe(true);
});
