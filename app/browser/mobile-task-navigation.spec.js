import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({
  hasTouch: true,
  viewport: { width: 390, height: 700 },
  serviceWorkers: 'block',
});

test('mobile Today toggles completion in place and task titles open the full page', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const title = `Mobile task ${randomUUID()}`;
  const created = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: { list_id: inbox.list_id, tasks: [{ title }] },
  });
  const [task] = await created.json();
  const schedule = await (
    await page.request.get(`/api/v1/tasks/${task.id}/schedule`)
  ).json();
  await page.goto('/today');
  const today = await page
    .locator('main[data-planning-today]')
    .getAttribute('data-planning-today');
  const saved = await page.request.put(`/api/v1/tasks/${task.id}/schedule`, {
    headers: { 'x-csrf-token': csrf },
    data: {
      revision: schedule.task_revision,
      schedule: { ...schedule.schedule, planned: { kind: 'date', date: today } },
    },
  });
  expect(saved.ok()).toBeTruthy();
  await page.reload();
  const row = page.locator(`[data-planning-task="${task.id}"]`);
  await row.getByRole('button', { name: 'Complete task' }).tap();
  await expect(row).toHaveClass(/is-complete/);
  await expect(page).toHaveURL(/\/today$/);
  const reopen = row.getByRole('button', { name: 'Reopen task' });
  await expect(reopen).toHaveAttribute('aria-pressed', 'true');
  await reopen.tap();
  await expect(row).not.toHaveClass(/is-complete/);
  await expect(page).toHaveURL(/\/today$/);
  await row.getByRole('link', { name: title }).tap();
  await expect(page).toHaveURL(/\/tasks\//);
  await expect(page.locator('main[data-task-page]')).toBeVisible();
  await page.goto('/inbox');
  const inboxRow = page.locator('.inbox-task').filter({ hasText: title });
  await inboxRow.getByRole('link', { name: title }).tap();
  await expect(page).toHaveURL(/\/tasks\//);
  await expect(page.locator('main[data-task-page]')).toBeVisible();
});
