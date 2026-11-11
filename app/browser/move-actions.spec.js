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

for (const mode of [
  'same-list-section',
  'other-list-section',
  'other-list-unsectioned',
]) {
  test(`section contents move together to ${mode}, leaving the section in place`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const f = await fixture(page);
    const listId = mode === 'same-list-section' ? f.list.id : f.target.id;
    const targetSection = await f.api(`/lists/${listId}/sections`, 'POST', {
      title: 'Next section',
    });
    await f.api(`/tasks/${f.task.id}/section`, 'PUT', {
      revision: f.task.revision,
      section_id: f.section.id,
    });
    const [completed, outside] = await f.api('/capture', 'POST', {
      list_id: f.list.id,
      tasks: [{ title: 'Completed task' }, { title: 'Untouched task' }],
    });
    const assigned = await f.api(`/tasks/${completed.id}/section`, 'PUT', {
      revision: completed.revision,
      section_id: f.section.id,
    });
    await f.api(`/tasks/${completed.id}`, 'PATCH', {
      revision: assigned.revision,
      complete: true,
    });
    await page.goto(`/lists/${f.list.id}`);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
    const originalUrl = page.url();
    const sourceSection = page.locator(`[data-section-id="${f.section.id}"]`);
    await sourceSection.locator('.entity-menu > summary').click();
    await sourceSection
      .getByRole('button', { name: 'Move all tasks to…', exact: true })
      .click();
    const dialog = page.getByRole('dialog', { name: 'Move all tasks', exact: true });
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('select[name=list_id]')).toHaveValue(f.list.id);
    await expect(
      dialog.locator(`select[name=section_id] option[value="${f.section.id}"]`),
    ).toHaveCount(0);
    await dialog.locator('select[name=list_id]').selectOption(listId);
    const sectionId = mode === 'other-list-unsectioned' ? '' : targetSection.id;
    if (sectionId)
      await dialog.locator('select[name=section_id]').selectOption(sectionId);
    await expect(dialog.getByRole('button', { name: 'Move', exact: true })).toBeEnabled();
    expect(await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
    await dialog.getByRole('button', { name: 'Move', exact: true }).click();
    await expect(dialog).not.toBeVisible();
    await expect(page.locator('.toast')).toHaveText('All tasks moved.');
    await expect(page).toHaveURL(originalUrl);
    await expect(sourceSection).toHaveCount(1);
    for (const task of [f.task, f.child, completed]) {
      const moved = await f.api(`/tasks/${task.id}`);
      expect(moved.list_id).toBe(listId);
      expect((await f.api(`/tasks/${task.id}/section`)).section_id).toBe(
        sectionId || null,
      );
    }
    expect((await f.api(`/tasks/${f.child.id}`)).parent_id).toBe(f.task.id);
    expect((await f.api(`/tasks/${completed.id}`)).complete).toBe(true);
    expect((await f.api(`/tasks/${outside.id}`)).list_id).toBe(f.list.id);
    expect(
      (await f.api(`/lists/${f.list.id}/sections`)).some((s) => s.id === f.section.id),
    ).toBe(true);
  });
}

test('a stale section contents move leaves every task untouched and can be retried', async ({
  page,
}) => {
  const f = await fixture(page);
  await f.api(`/tasks/${f.task.id}/section`, 'PUT', {
    revision: f.task.revision,
    section_id: f.section.id,
  });
  await page.goto(`/lists/${f.list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const sourceSection = page.locator(`[data-section-id="${f.section.id}"]`);
  await sourceSection.locator('.entity-menu > summary').click();
  await sourceSection.locator('[data-section-tasks-move-open]').click();
  const dialog = page.getByRole('dialog', { name: 'Move all tasks', exact: true });
  await dialog.locator('select[name=list_id]').selectOption(f.target.id);
  await page.route('**/api/v1/sections/*/tasks/move', async (route) => {
    await route.fulfill({
      status: 409,
      json: { error: { message: 'Section tasks changed; try the move again.' } },
    });
  });
  await dialog.getByRole('button', { name: 'Move', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('Section tasks changed');
  await expect(dialog).toBeVisible();
  expect((await f.api(`/tasks/${f.task.id}`)).list_id).toBe(f.list.id);
  expect((await f.api(`/tasks/${f.child.id}`)).list_id).toBe(f.list.id);
  await page.unroute('**/api/v1/sections/*/tasks/move');
  await dialog.getByRole('button', { name: 'Move', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect((await f.api(`/tasks/${f.child.id}`)).list_id).toBe(f.target.id);
});
