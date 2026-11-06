import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

async function setup(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const api = async (path, method = 'GET', data) => {
    const response = await page.request.fetch(`/api/v1${path}`, {
      method,
      data,
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  };
  const workspace = await api('/workspaces', 'POST', {
    title: `Completion ${randomUUID()}`,
  });
  const list = await api('/lists', 'POST', {
    workspace_id: workspace.id,
    title: `Completion ${randomUUID()}`,
  });
  const capture = async (title, parent_id = null) =>
    (
      await api('/capture', 'POST', {
        list_id: list.id,
        tasks: [{ title, parent_id }],
      })
    )[0];
  const current = (id) => api(`/tasks/${id}`);
  return { api, list, capture, current };
}

test('list completion confirms hidden unfinished descendants; cancel and Escape save nothing', async ({
  page,
}) => {
  const { list, capture, current } = await setup(page);
  const parent = await capture('Parent');
  const child = await capture('Child', parent.id);
  const grandchild = await capture('Grandchild', child.id);
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const row = page.locator(`.task[data-id="${parent.id}"]`);
  const modal = page.getByRole('dialog', { name: 'Confirm change' });
  const writes = [];
  page.on('request', (request) => {
    if (request.method() === 'PATCH') writes.push(request.url());
  });
  await row.locator('.completion').click();
  await expect(modal).toBeVisible();
  await expect(row).not.toHaveClass(/is-complete/);
  expect(writes).toHaveLength(0);
  await modal.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect((await current(parent.id)).complete).toBe(false);
  await row.locator('.task-title').dblclick();
  await expect(modal).toBeVisible();
  await modal.press('Escape');
  expect(writes).toHaveLength(0);
  await row.locator('.completion').click();
  await modal.getByRole('button', { name: 'Confirm', exact: true }).click();
  for (const task of [parent, child, grandchild])
    await expect.poll(async () => (await current(task.id)).complete).toBe(true);
  expect(writes).toHaveLength(1);
});

test('task detail confirms nested completion and skips confirmation when children are complete', async ({
  page,
}) => {
  const { capture, current } = await setup(page);
  const parent = await capture('Parent');
  const child = await capture('Child', parent.id);
  const grandchild = await capture('Grandchild', child.id);
  await page.goto(`/tasks/${parent.id}`);
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  const modal = page.getByRole('dialog', { name: 'Confirm change' });
  await page.getByRole('button', { name: 'Complete Child', exact: true }).click();
  await expect(modal).toBeVisible();
  await modal.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect.poll(async () => (await current(grandchild.id)).complete).toBe(true);
  await expect(page.locator('[data-root-complete]')).toBeEnabled();
  await page.locator('[data-root-complete]').click();
  await expect.poll(async () => (await current(parent.id)).complete).toBe(true);
  await expect(modal).not.toBeVisible();
  await page.locator('[data-root-complete]').click();
  await expect.poll(async () => (await current(parent.id)).complete).toBe(false);
  await expect(modal).not.toBeVisible();
});

test('completion checks additional subtree pages before changing a task', async ({
  page,
}) => {
  const { capture, list, current } = await setup(page);
  const parent = await capture('Paged parent');
  const child = await capture('Unfinished descendant', parent.id);
  const cursors = [];
  await page.route(`**/api/v1/tasks/${parent.id}/subtree/query`, async (route) => {
    const body = route.request().postDataJSON();
    if (body.fields.length !== 1 || body.fields[0] !== 'complete')
      return route.continue();
    cursors.push(body.cursor);
    await route.fulfill({
      json: body.cursor
        ? {
            items: [{ id: child.id, complete: false }],
            next_cursor: null,
          }
        : {
            items: [{ id: parent.id, complete: false }],
            next_cursor: 'synthetic-next-page',
          },
    });
  });
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  await page.locator(`.task[data-id="${parent.id}"] .completion`).click();
  const modal = page.getByRole('dialog', { name: 'Confirm change' });
  await expect(modal).toBeVisible();
  expect(cursors).toEqual([null, 'synthetic-next-page']);
  await modal.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect((await current(child.id)).complete).toBe(false);
});

test('planning card completion asks before completing unfinished subtasks', async ({
  page,
}) => {
  const { api, current } = await setup(page);
  const inbox = await api('/inbox');
  const [parent] = await api('/capture', 'POST', {
    list_id: inbox.list_id,
    tasks: [{ title: `Planning parent ${randomUUID()}` }],
  });
  const [child] = await api('/capture', 'POST', {
    list_id: inbox.list_id,
    tasks: [{ title: 'Planning child', parent_id: parent.id }],
  });
  await page.goto('/inbox');
  await page
    .locator(`.workbench-task[data-task-id="${parent.id}"] .workbench-check`)
    .click();
  const modal = page.getByRole('dialog', { name: 'Confirm change' });
  await expect(modal).toBeVisible();
  await modal.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect.poll(async () => (await current(child.id)).complete).toBe(true);
});
