import { expect, test } from '@playwright/test';
import { randomUUID } from 'node:crypto';

const headers = { Authorization: 'Bearer local-integration-token-not-for-production' };

test('recurring screen links to the task and preserves pause and resume controls', async ({
  page,
  request,
}) => {
  const api = async (path, data) => {
    const response = await request.post(`/api/v1${path}`, {
      data,
      headers: { ...headers, 'Idempotency-Key': randomUUID() },
    });
    expect(response.ok(), await response.text()).toBe(true);
    return response.json();
  };
  const space = await api('/workspaces', { title: `Recurrence ${randomUUID()}` });
  const list = await api('/lists', { workspace_id: space.id, title: 'Routines' });
  const title = `Recurring task ${randomUUID()}`;
  const [task] = await api('/capture', { list_id: list.id, tasks: [{ title }] });
  await api(`/tasks/${task.id}/series`, {
    revision: task.revision,
    spec: {
      rule: { kind: 'daily', interval_days: 1 },
      start_local: '2030-05-01T10:00:00',
      timezone: 'Europe/Zurich',
    },
  });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/recurring');
  const row = page.locator('.recurring-series-list li').filter({ hasText: title });
  await expect(row.getByRole('link', { name: title })).toBeVisible();
  await expect(row).toContainText('Every day · Active');
  await row.getByRole('button', { name: 'Pause' }).click();
  await expect(row).toContainText('Every day · Paused');
  await row.getByRole('button', { name: 'Resume' }).click();
  await expect(row).toContainText('Every day · Active');
  await row.getByRole('link', { name: title }).click();
  await expect(page.locator('[data-task-page]')).toHaveAttribute('data-task-id', task.id);
});
