import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('Inbox keeps planned tasks until settled and can undo, with working details', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const headers = { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() };
  const spaceResponse = await page.request.post('/api/v1/workspaces', {
    headers,
    data: { title: `Inbox test ${randomUUID()}` },
  });
  expect(spaceResponse.ok()).toBeTruthy();
  const space = await spaceResponse.json();
  const listName = `Destination ${randomUUID()}`;
  const listResponse = await page.request.post('/api/v1/lists', {
    headers: { ...headers, 'Idempotency-Key': randomUUID() },
    data: {
      workspace_id: space.id,
      title: listName,
      description: '',
      color: '#3b82f6',
      restricted: false,
    },
  });
  expect(listResponse.ok()).toBeTruthy();
  const intake = await (await page.request.get('/api/v1/inbox')).json();
  const title = `Inbox stream ${randomUUID()}`;
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: intake.list_id,
      tasks: [{ title, description: 'A little context' }],
      metadata: { priority: 1, due_date: null, tags: ['errands'] },
    },
  });
  expect(response.ok()).toBeTruthy();
  await page.reload();
  const row = page.locator('.inbox-task').filter({ hasText: title });
  await expect(row).toBeVisible();
  await expect(row).toContainText('A little context');
  await expect(row.locator('.inbox-location')).toHaveText('Inbox');
  await expect(row.locator('.inbox-tag')).toHaveText('+errands');
  await expect(row.locator('.inbox-priority')).toHaveText('P1');
  await expect(row.locator('.inbox-priority')).toHaveCSS('color', 'rgb(217, 154, 50)');
  await row.getByText('Plan…').click();
  await row.getByRole('button', { name: 'Tomorrow' }).click();
  await expect(row).toContainText('Planned');
  await row.getByRole('button', { name: 'Move…' }).click();
  const move = page.getByRole('dialog', { name: 'Move to a list' });
  await move.getByRole('searchbox').fill(listName);
  await move.getByRole('button', { name: new RegExp(listName) }).click();
  await expect(row).toContainText('#' + listName);
  await row.getByRole('link', { name: title }).click();
  await expect(page.locator('.planning-detail-content [data-task-page]')).toBeVisible();
  await expect(page.locator('.planning-detail-content .loading-error')).toHaveCount(0);
  await page.getByRole('button', { name: 'Close task details' }).click();
  await page.reload();
  await expect(row).toContainText('#' + listName);
  await row.getByRole('button', { name: 'Settle' }).click();
  await expect(row).toHaveCount(0);
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(row).toBeVisible();
  await page.reload();
  await expect(row).toBeVisible();
});
