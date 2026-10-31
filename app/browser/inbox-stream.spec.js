import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('Inbox keeps planned tasks until settled and can undo, with working details', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const title = `Inbox stream ${randomUUID()}`;
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: { list_id: 'inbox', tasks: [{ title, description: 'A little context' }] },
  });
  expect(response.ok()).toBeTruthy();
  await page.reload();
  const row = page.locator('.inbox-task').filter({ hasText: title });
  await expect(row).toBeVisible();
  await expect(row).toContainText('A little context');
  await row.getByText('Plan…').click();
  await row.getByRole('button', { name: 'Tomorrow' }).click();
  await expect(row).toContainText('Planned');
  await row.getByRole('link', { name: title }).click();
  await expect(page.locator('.planning-detail-content [data-task-page]')).toBeVisible();
  await expect(page.locator('.planning-detail-content .loading-error')).toHaveCount(0);
  await page.getByRole('button', { name: 'Close task details' }).click();
  await row.getByRole('button', { name: 'Settle' }).click();
  await expect(row).toHaveCount(0);
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(row).toBeVisible();
  await page.reload();
  await expect(row).toBeVisible();
});
