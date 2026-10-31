import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('settling several tasks shows one Undo notice at a time, newest first', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const prefix = `Undo queue ${randomUUID()}`;
  const titles = [`${prefix} first`, `${prefix} second`, `${prefix} third`];
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: { list_id: inbox.list_id, tasks: titles.map((title) => ({ title })) },
  });
  expect(response.ok()).toBeTruthy();
  await page.reload();
  const row = (title) => page.locator('.inbox-task').filter({ hasText: title });
  for (const title of titles) {
    await row(title).getByRole('button', { name: 'Settle' }).click();
    await expect(page.locator('.inbox-undo:not([aria-hidden])')).toHaveCount(1);
    await expect(page.locator('.inbox-undo:not([aria-hidden])')).toContainText(title);
  }
  for (const title of [...titles].reverse()) {
    const notice = page.locator('.inbox-undo:not([aria-hidden])');
    await expect(notice).toContainText(title);
    await notice.getByRole('button', { name: 'Undo' }).click();
    await expect(row(title)).toBeVisible();
  }
  await expect(page.locator('.inbox-undo')).toHaveCount(0);
  await page.reload();
  for (const title of titles) await expect(row(title)).toBeVisible();
  await row(titles[0]).getByRole('button', { name: 'Settle' }).click();
  await expect(page.locator('.inbox-undo:not([aria-hidden])')).toContainText(titles[0]);
  await row(titles[1]).getByRole('button', { name: 'Settle' }).click();
  await expect(page.locator('.inbox-undo:not([aria-hidden])')).toContainText(titles[1]);
  await expect(page.locator('.inbox-undo:not([aria-hidden])')).toContainText(titles[0], {
    timeout: 15000,
  });
  await page.locator('.inbox-undo:not([aria-hidden]) button').click();
  await expect(row(titles[0])).toBeVisible();
});
