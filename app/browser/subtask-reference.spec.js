import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('task page displays and copies each subtask reference', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const post = async (path, data) => {
    const response = await page.request.post(path, {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  };
  const space = await post('/api/v1/workspaces', { title: `Reference ${randomUUID()}` });
  const list = await post('/api/v1/lists', {
    workspace_id: space.id,
    title: 'Reference list',
  });
  const parent = (
    await post('/api/v1/capture', { list_id: list.id, tasks: [{ title: 'Parent task' }] })
  )[0];
  const child = (
    await post('/api/v1/capture', {
      list_id: list.id,
      tasks: [{ title: 'Child task', parent_id: parent.id }],
    })
  )[0];
  await page.goto(`/tasks/${parent.id}`);
  const row = page.locator(`[data-child-id="${child.id}"]`);
  const reference = row.getByRole('button', { name: 'Copy reference for Child task' });
  await expect(reference).toHaveText(/^\^[a-z0-9]{7}$/);
  await expect(row.locator('.task-child-menu')).toBeVisible();
  await page.evaluate(() => {
    window.copiedReference = '';
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: async (text) => {
          window.copiedReference = text;
        },
      },
    });
  });
  const expected = await reference.textContent();
  await reference.click();
  await expect.poll(() => page.evaluate(() => window.copiedReference)).toBe(expected);
});
