import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

async function fixture(page) {
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
  const source = await api('/workspaces', 'POST', { title: `Source ${randomUUID()}` });
  const destination = await api('/workspaces', 'POST', {
    title: `Destination ${randomUUID()}`,
  });
  await api(`/workspaces/${destination.id}/members`, 'POST', {
    user_id: 'other',
    role: 'editor',
    revision: destination.revision,
    acknowledge_access_change: true,
  });
  const list = await api('/lists', 'POST', {
    workspace_id: source.id,
    title: 'Source list',
  });
  const target = await api('/lists', 'POST', {
    workspace_id: destination.id,
    title: 'Destination list',
  });
  const section = await api(`/lists/${list.id}/sections`, 'POST', {
    title: 'Source section',
  });
  const capture = async (title, parent_id = null) =>
    (
      await api('/capture', 'POST', {
        list_id: list.id,
        tasks: [{ title, parent_id }],
      })
    )[0];
  const task = await capture('Move root');
  const child = await capture('Move child', task.id);
  return { api, source, destination, list, target, section, task, child };
}

for (const kind of ['task', 'section', 'list']) {
  test(`${kind} moves in one step, stays on the source page and shows a toast`, async ({
    page,
  }) => {
    const f = await fixture(page);
    if (kind === 'section')
      await f.api(`/tasks/${f.task.id}/section`, 'PUT', {
        revision: f.task.revision,
        section_id: f.section.id,
      });
    await page.goto(`/lists/${f.list.id}`);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
    const originalUrl = page.url();
    const trigger =
      kind === 'list'
        ? page.locator('[data-list-move-open]')
        : kind === 'section'
          ? page.locator(`[data-section-move-open="${f.section.id}"]`)
          : page.locator(`[data-task-move-open="${f.task.id}"]`);
    await trigger.locator('xpath=ancestor::details[1]/summary').click();
    await trigger.click();
    const dialog = page.getByRole('dialog', { name: `Move ${kind}`, exact: true });
    await expect(dialog).toBeVisible();
    const destination = dialog.locator(
      `select[name="${kind === 'list' ? 'workspace_id' : 'list_id'}"]`,
    );
    await destination.selectOption(kind === 'list' ? f.destination.id : f.target.id);
    await dialog.getByRole('button', { name: 'Move', exact: true }).click();
    await expect(dialog).not.toBeVisible();
    await expect(page).toHaveURL(originalUrl);
    await expect(page.locator('.toast')).toHaveText(
      `${kind[0].toUpperCase() + kind.slice(1)} moved.`,
    );
    if (kind === 'list')
      expect((await f.api(`/lists/${f.list.id}`)).workspace_id).toBe(f.destination.id);
    else {
      expect((await f.api(`/tasks/${f.task.id}`)).list_id).toBe(f.target.id);
      expect((await f.api(`/tasks/${f.child.id}`)).list_id).toBe(f.target.id);
      await expect(page.locator(`.task[data-id="${f.task.id}"]`)).toHaveCount(0);
    }
  });
}

test('a rejected move keeps the dialog open without a success toast or redirect', async ({
  page,
}) => {
  const f = await fixture(page);
  await page.goto(`/lists/${f.list.id}`);
  const originalUrl = page.url();
  const trigger = page.locator(`[data-task-move-open="${f.task.id}"]`);
  await trigger.locator('xpath=ancestor::details[1]/summary').click();
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: 'Move task', exact: true });
  await dialog.locator('select[name=list_id]').selectOption(f.target.id);
  await page.route('**/api/v1/bulk/apply', (route) =>
    route.fulfill({
      status: 409,
      json: { error: { message: 'Destination changed. Try again.' } },
    }),
  );
  await dialog.getByRole('button', { name: 'Move', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('Destination changed');
  await expect(dialog).toBeVisible();
  await expect(page).toHaveURL(originalUrl);
  await expect(page.locator('.toast.is-visible')).toHaveCount(0);
  expect((await f.api(`/tasks/${f.task.id}`)).list_id).toBe(f.list.id);
});

test('move to parent applies immediately and stays on the task', async ({ page }) => {
  const f = await fixture(page);
  const [grandchild] = await f.api('/capture', 'POST', {
    list_id: f.list.id,
    tasks: [{ title: 'Keep descendant', parent_id: f.child.id }],
  });
  await page.goto(`/tasks/${f.child.id}`);
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  const originalUrl = page.url();
  await page.locator('.task-context-menu summary').click();
  await page.locator('.task-context-menu [data-task-promote]').click();
  await expect(page.locator('.toast')).toHaveText('Task moved to parent.');
  await expect(page).toHaveURL(originalUrl);
  expect((await f.api(`/tasks/${f.child.id}`)).parent_id).toBeNull();
  expect((await f.api(`/tasks/${grandchild.id}`)).parent_id).toBe(f.child.id);
  await expect(page.locator('dialog[open]')).toHaveCount(0);
});
