import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('task and subtask context menus move items to Trash with dismissal motion', async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.addInitScript(() => {
    const animate = Element.prototype.animate;
    Element.prototype.animate = function (frames, options) {
      if (options?.id === 'task-trash')
        document.documentElement.dataset.trashedSurface = this.className;
      return animate.call(this, frames, options);
    };
  });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  async function post(path, data) {
    const response = await page.request.post(`/api/v1/${path}`, {
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  }
  const workspace = await post('workspaces', { title: `Trash ${randomUUID()}` });
  const list = await post('lists', { workspace_id: workspace.id, title: 'Trash menus' });
  const capture = async (title, parent_id = null) =>
    (await post('capture', { list_id: list.id, tasks: [{ title, parent_id }] }))[0];
  const listed = await capture('List row to trash');
  const parent = await capture('Parent for testing child actions');
  const child = await capture('Child row to trash', parent.id);
  const panelChild = await capture('Panel child to trash', parent.id);
  const dedicated = await capture('Subtask page to trash', parent.id);
  const dedicatedRoot = await capture('Task page to trash');
  const removed = page.locator('html');

  async function openTrash(menu, toggle) {
    await menu.getByLabel(toggle, { exact: true }).click();
    const trash = menu.getByRole('button', { name: 'Move to trash…' });
    await expect(menu.locator('button:visible').last()).toHaveText('Move to trash…');
    await expect(trash).toHaveCSS('color', 'rgb(253, 164, 175)');
    await removed.evaluate((el) => delete el.dataset.trashedSurface);
    await trash.click();
    return page.getByRole('dialog', { name: 'Confirm change' });
  }
  const confirmTrash = (dialog) =>
    dialog.getByRole('button', { name: 'Move to trash', exact: true }).click();

  await page.goto(`/lists/${list.id}`);
  const listPath = new URL(page.url()).pathname;
  const listedRow = page.locator(`.task[data-id="${listed.id}"]`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  let dialog = await openTrash(listedRow, 'Actions for List row to trash');
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(listedRow).toBeVisible();
  await expect(removed).not.toHaveAttribute('data-trashed-surface');
  dialog = await openTrash(listedRow, 'Actions for List row to trash');
  await confirmTrash(dialog);
  await expect(removed).toHaveAttribute('data-trashed-surface', /\btask\b/);
  await expect(listedRow).toHaveCount(0);

  const parentRow = page.locator(`.task[data-id="${parent.id}"]`);
  await parentRow.getByLabel('Actions for Parent for testing child actions').click();
  await parentRow.getByRole('button', { name: 'Open details' }).click();
  const panel = page.getByRole('complementary', { name: 'Task details' });
  const panelRow = panel.locator(`[data-child-id="${panelChild.id}"]`);
  dialog = await openTrash(panelRow, 'Actions for Panel child to trash');
  await confirmTrash(dialog);
  await expect(removed).toHaveAttribute('data-trashed-surface', /task-page-child/);
  await expect(panelRow).toHaveCount(0);
  await expect(panel).toBeVisible();

  await page.goto(`/tasks/${parent.id}`);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const childRow = page.locator(`[data-child-id="${child.id}"]`);
  dialog = await openTrash(childRow, 'Actions for Child row to trash');
  await confirmTrash(dialog);
  await expect(childRow).toHaveCount(0);
  await expect(removed).not.toHaveAttribute('data-trashed-surface');

  await page.goto(`/tasks/${dedicated.id}`);
  dialog = await openTrash(page.locator('.task-context-menu'), 'Task actions');
  await confirmTrash(dialog);
  await expect(page).toHaveURL(new RegExp(listPath));
  await page.goto(`/tasks/${dedicatedRoot.id}`);
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  dialog = await openTrash(page.locator('.task-context-menu'), 'Task actions');
  await confirmTrash(dialog);
  await expect(removed).toHaveAttribute('data-trashed-surface', /task-page/);
  await expect(page).toHaveURL(new RegExp(listPath));

  await page.goto('/trash');
  const groups = page.locator('.trash-group').filter({ hasText: workspace.title });
  for (const title of [
    listed.title,
    child.title,
    panelChild.title,
    dedicated.title,
    dedicatedRoot.title,
  ])
    await expect(groups.getByText(title, { exact: true })).toBeVisible();
});
