import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('list menus review completion and archive and restore sections on mobile', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/spaces');
  const headers = {
    'X-CSRF-Token': await page.locator('meta[name=csrf-token]').getAttribute('content'),
  };
  async function post(path, data) {
    const response = await page.request.post(`/api/v1${path}`, {
      headers: { ...headers, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok()).toBeTruthy();
    return response.json();
  }
  const space = await post('/workspaces', {
    title: `Lifecycle ${randomUUID().slice(0, 8)}`,
    description: '',
  });
  const list = await post('/lists', {
    workspace_id: space.id,
    title: 'Lifecycle list',
    description: '',
    restricted: false,
  });
  const section = await post(`/lists/${list.id}/sections`, {
    title: 'Review section',
    description: '',
  });
  const [root] = await post('/capture', {
    list_id: list.id,
    section_id: section.id,
    tasks: [{ title: 'Root', description: '' }],
  });
  const [child] = await post('/capture', {
    list_id: list.id,
    tasks: [{ title: 'Child', description: '', parent_id: root.id }],
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const group = page.locator(`[data-section-id="${section.id}"]`);
  await group.locator('.entity-menu > summary').click();
  await group.getByRole('button', { name: 'Mark all tasks complete…' }).click();
  const confirmation = page.getByRole('dialog', { name: 'Mark all tasks complete?' });
  await expect(confirmation).toContainText('2 unfinished tasks');
  await confirmation.getByRole('button', { name: 'Cancel', exact: true }).click();
  const tasks = await (await page.request.get(`/api/v1/lists/${list.id}/tasks`)).json();
  expect(tasks.find((task) => task.id === child.id).complete).toBe(false);
  await group.locator('.entity-menu > summary').click();
  await group.getByRole('button', { name: 'Mark all tasks complete…' }).click();
  await confirmation
    .getByRole('button', { name: 'Mark all complete', exact: true })
    .click();
  await expect(group).toContainText('100%');
  await group.locator('.entity-menu > summary').click();
  await group.getByRole('button', { name: 'Archive section…' }).click();
  await page
    .getByRole('dialog', { name: 'Archive this section?' })
    .getByRole('button', { name: 'Archive section', exact: true })
    .click();
  await expect(group).toHaveCount(0);
  await page.locator('[data-list-display] .entity-menu > summary').click();
  await page.getByRole('button', { name: 'Archived sections…' }).click();
  await page.getByRole('button', { name: 'Restore section Review section' }).click();
  await page
    .getByRole('dialog', { name: 'Restore this section?' })
    .getByRole('button', { name: 'Restore section', exact: true })
    .click();
  await expect(group).toBeVisible();
  await page.goto(`/space/${space.id}`);
  await page.locator('[data-space-display] .entity-menu > summary').click();
  await page.getByRole('button', { name: 'Archive space…' }).click();
  await page
    .getByRole('dialog', { name: 'Archive this space?' })
    .getByRole('button', { name: 'Archive space', exact: true })
    .click();
  await expect(page).toHaveURL(/\/spaces$/);
});
