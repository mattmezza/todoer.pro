import { expect, test } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({ serviceWorkers: 'block' });

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
    expect(response.ok(), await response.text()).toBe(true);
    return response.json();
  };
  const space = await api('/workspaces', 'POST', {
    title: `Capture test ${randomUUID()}`,
  });
  const list = await api('/lists', 'POST', { workspace_id: space.id, title: 'Feedback' });
  return { api, list };
}

test('list capture appears in its selected section before the response and settles once', async ({
  page,
}) => {
  const { api, list } = await setup(page);
  const section = await api(`/lists/${list.id}/sections`, 'POST', {
    title: 'Destination',
  });
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const save = Promise.withResolvers();
  await page.route('**/api/v1/capture', async (route) => {
    await save.promise;
    await route.continue();
  });
  const group = page.locator(`[data-section-id="${section.id}"]`);
  await group.getByRole('button', { name: 'Add task', exact: true }).click();
  const form = group.locator('.section-capture');
  await form.getByLabel('Task name').fill('Optimistic list item');
  await form.getByRole('button', { name: 'Add task' }).click();
  try {
    await expect(group.locator('.pending-task')).toContainText('Optimistic list item');
    await expect(group.locator('.pending-task')).toContainText('Saving');
    await expect(group.getByRole('link', { name: 'Optimistic list item' })).toHaveCount(
      0,
    );
  } finally {
    save.resolve();
  }
  await expect(group.locator('.pending-task')).toHaveCount(0);
  await expect(group.getByRole('link', { name: 'Optimistic list item' })).toHaveCount(1);
});

test('a rejected subtask stops showing as pending and keeps its draft for a successful retry', async ({
  page,
}) => {
  const { api, list } = await setup(page);
  const [task] = await api('/capture', 'POST', {
    list_id: list.id,
    tasks: [{ title: 'Parent task' }],
  });
  await page.goto(`/tasks/${task.id}`);
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  const save = Promise.withResolvers();
  await page.route(
    '**/api/v1/capture',
    async (route) => {
      await save.promise;
      await route.fulfill({
        status: 422,
        json: { error: { message: 'Test rejection' } },
      });
    },
    { times: 1 },
  );
  await page.locator('[data-child-composer] > summary').click();
  const form = page.locator('[data-child-form]');
  const title = form.getByLabel('New sub-task');
  const submit = form.getByRole('button', { name: 'Add sub-task' });
  await title.fill('Optimistic child');
  await submit.click();
  try {
    await expect(page.locator('[data-children] .pending-task')).toContainText(
      'Optimistic child',
    );
  } finally {
    save.resolve();
  }
  await expect(page.locator('[data-children] .pending-task')).toHaveCount(0);
  await expect(page.locator('[data-task-notice]')).toContainText('Test rejection');
  await expect(title).toHaveValue('Optimistic child');
  await submit.click();
  await expect(
    page.locator('[data-children]').getByRole('link', { name: 'Optimistic child' }),
  ).toHaveCount(1);
  await expect(title).toBeEmpty();
});
