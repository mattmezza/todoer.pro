import { expect, test } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({ serviceWorkers: 'block' });

async function openList(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  async function post(path, data) {
    const response = await page.request.post(`/api/v1${path}`, {
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  }
  const space = await post('/workspaces', { title: `Columns ${randomUUID()}` });
  const list = await post('/lists', { workspace_id: space.id, title: 'Column test' });
  const section = await post(`/lists/${list.id}/sections`, { title: 'In progress' });
  const [task] = await post('/capture', {
    list_id: list.id,
    tasks: [{ title: 'Existing task' }],
  });
  const [child] = await post('/capture', {
    list_id: list.id,
    tasks: [{ title: 'Existing subtask', parent_id: task.id }],
  });
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const board = page.getByRole('region', { name: 'Tasks' });
  const unsectioned = board.locator('[data-section-id=""]');
  const destination = board.locator(`[data-section-id="${section.id}"]`);
  const menu = async (name) => {
    await page.getByLabel('List actions', { exact: true }).click();
    await page.getByRole('button', { name, exact: true }).click();
  };
  return { board, child, destination, menu, section, task, unsectioned };
}

test('list actions still initialize when the server omits column controls', async ({
  page,
}) => {
  await page.goto('/signin?next=/today');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const response = await page.request.get('/lists/inbox');
  expect(response.ok()).toBe(true);
  const body = (await response.text()).replace(
    /<button type="button" data-(?:switch|save)-list-view[^>]*>.*?<\/button>/g,
    '',
  );
  await page.route(response.url(), (route) => route.fulfill({ response, body }));
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto(response.url());
  expect(errors).toEqual([]);
  await expect(page.locator('[data-switch-list-view]')).toHaveCount(0);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  await page.getByRole('button', { name: 'Add section', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Section name' })).toBeFocused();
});

test('list menu switches views and persists the default while cards use task actions', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 820 });
  const { board, child, destination, menu, section, task, unsectioned } =
    await openList(page);
  const listUrl = page.url();
  const original = unsectioned.getByRole('link', { name: 'Existing task' });

  await expect(board).toHaveAttribute('data-list-view', 'list');
  await destination.getByLabel('Toggle section In progress').click();
  await menu('Show columns');
  await expect(board).toHaveAttribute('data-list-view', 'columns');
  await expect(unsectioned.getByRole('heading', { name: 'No section' })).toBeVisible();
  await expect(destination.getByRole('button', { name: 'Add task' })).toBeVisible();
  await menu('Show list');
  await expect(destination.getByRole('button', { name: 'Add task' })).toBeHidden();
  await menu('Show columns');
  const saved = page.waitForResponse((response) =>
    response.url().endsWith('/view-preference'),
  );
  await menu('Use columns as default');
  expect((await saved).status()).toBe(204);
  await page.goto('/lists/inbox');
  await expect(board).toHaveAttribute('data-list-view', 'list');
  await page.goto(listUrl);
  await expect(board).toHaveAttribute('data-list-view', 'columns');

  await destination.getByRole('button', { name: 'Add task' }).click();
  await destination.getByRole('combobox', { name: 'Task name' }).fill('Card draft');
  await menu('Show list');
  await menu('Show columns');
  await expect(destination.getByRole('combobox', { name: 'Task name' })).toHaveValue(
    'Card draft',
  );
  await destination.getByRole('button', { name: 'Add task' }).click();
  const created = destination.getByRole('link', { name: 'Card draft' });
  await expect(created).toBeVisible();
  await created.click();
  const panel = page.getByRole('complementary', { name: 'Task details' });
  await expect(panel.locator('[data-edit-title]')).toHaveText('Card draft');
  await expect(page).toHaveURL(listUrl);
  await page.getByRole('button', { name: 'Close task details' }).click();

  await original.dragTo(destination, { targetPosition: { x: 20, y: 20 } });
  const moved = destination.getByRole('link', { name: 'Existing task' });
  await expect(moved).toBeVisible();
  const assigned = await page.request.get(`/api/v1/tasks/${task.id}/section`);
  expect((await assigned.json()).section_id).toBe(section.id);
  const childSection = await page.request.get(`/api/v1/tasks/${child.id}/section`);
  expect((await childSection.json()).section_id).toBe(section.id);
  await moved.dragTo(unsectioned, { targetPosition: { x: 20, y: 20 } });
  await expect(original).toBeVisible();
  const unassigned = await page.request.get(`/api/v1/tasks/${task.id}/section`);
  expect((await unassigned.json()).section_id).toBeNull();
  await unsectioned.getByRole('button', { name: 'Move Existing task' }).press('Enter');
  const move = page.getByRole('dialog', { name: 'Move task' });
  await expect(move.getByLabel('Destination section (optional)')).toBeEnabled();
  await move.getByRole('button', { name: 'Cancel' }).click();

  await menu('Show list');
  await page.reload();
  await expect(board).toHaveAttribute('data-list-view', 'list');
  await page.goto(listUrl);
  await expect(board).toHaveAttribute('data-list-view', 'columns');
  await menu('Show list');
  const reset = page.waitForResponse((response) =>
    response.url().endsWith('/view-preference'),
  );
  await menu('Use list as default');
  expect((await reset).status()).toBe(204);
  await page.reload();
  await expect(board).toHaveAttribute('data-list-view', 'list');
  await original.click();
  await expect(page).toHaveURL(/\/tasks\//);
});

test('touch drag scrolls the board and drops a card in the next column', async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== 'chromium', 'Uses Chromium touch input protocol');
  await page.setViewportSize({ width: 390, height: 844 });
  const { board, destination, menu, unsectioned } = await openList(page);
  await menu('Show columns');
  const grip = unsectioned.getByRole('button', {
    name: 'Move Existing task',
    exact: true,
  });
  await grip.scrollIntoViewIfNeeded();
  const handle = await grip.boundingBox();
  const boardBox = await board.boundingBox();
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true });
  const touch = (type, touchPoints) =>
    cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
  const x = handle.x + handle.width / 2;
  const y = handle.y + handle.height / 2;
  await touch('touchStart', [{ x, y }]);
  await touch('touchMove', [{ x: boardBox.x + boardBox.width - 8, y }]);
  await expect.poll(() => board.evaluate((node) => node.scrollLeft)).toBeGreaterThan(180);
  await expect(destination).toHaveAttribute('data-column-drop', '');
  await touch('touchEnd', []);
  await expect(destination.getByRole('link', { name: 'Existing task' })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
});
