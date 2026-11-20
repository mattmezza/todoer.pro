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

test('panel completion updates the list row and section progress without closing the panel', async ({
  page,
}) => {
  const { api, capture, list } = await setup(page);
  const section = await api(`/lists/${list.id}/sections`, 'POST', {
    title: 'Panel completion',
  });
  const [parent] = await api('/capture', 'POST', {
    list_id: list.id,
    section_id: section.id,
    tasks: [{ title: 'Panel parent' }],
  });
  const child = await capture('Panel child', parent.id);
  await page.route('**/api/v1/events?*', (route) => route.abort());
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const panel = page.getByRole('complementary', { name: 'Task details' });
  const row = page.locator(`.task[data-id="${parent.id}"]`);
  const button = row.locator('.completion');
  const progress = page.locator(`[data-section-id="${section.id}"] .section-progress`);
  const openPanel = async () => {
    await row.locator('.task-menu').click();
    await row.getByRole('button', { name: 'Open details' }).click();
  };
  await expect(progress).toHaveText('0%');
  await openPanel();
  await expect(row).toHaveClass(/is-selected/);

  await panel.getByRole('button', { name: 'Complete Panel child' }).click();
  await expect(progress).toHaveText('100%');
  await expect(button).toHaveAttribute('aria-pressed', 'false');

  const rootComplete = panel.locator('[data-root-complete]');
  await rootComplete.click();
  await expect(button).toHaveAccessibleName('Reopen Panel parent');
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  await expect(row).toHaveClass(/is-complete/);
  await expect(row).toHaveClass(/is-selected/);

  await panel.getByRole('button', { name: 'Reopen Panel child' }).click();
  await expect(progress).toHaveText('0%');
  await expect(button).toHaveAccessibleName('Complete Panel parent');
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  await expect(row).not.toHaveClass(/is-complete/);
  await expect(panel.locator(`[data-child-id="${child.id}"]`)).toBeVisible();

  const modal = page.getByRole('dialog', { name: 'Confirm change' });
  await expect(rootComplete).toHaveAccessibleName('Complete task');
  await rootComplete.click();
  await modal.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  await rootComplete.click();
  await modal.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect(progress).toHaveText('100%');
  await expect(button).toHaveAttribute('aria-pressed', 'true');

  await rootComplete.click();
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  await expect(progress).toHaveText('100%');
  await page.getByRole('button', { name: 'Close task details' }).click();
  await expect(panel).toHaveCount(0);

  await page.locator('summary[aria-label="List actions"]').click();
  await page.getByRole('button', { name: 'Hide completed' }).click();
  await openPanel();
  await rootComplete.click();
  await expect(row).toHaveCount(0);
  await expect(rootComplete).toHaveAccessibleName('Reopen task');
  await rootComplete.click();
  await expect(row).toHaveClass(/is-selected/);
  await expect(button).toHaveAccessibleName('Complete Panel parent');
});

test('completing and reopening a subtask preview updates its row on the parent page', async ({
  page,
}) => {
  const { api, capture } = await setup(page);
  const parent = await capture('Parent');
  const [child] = await api('/capture', 'POST', {
    list_id: parent.list_id,
    tasks: [{ title: 'Subtask', description: 'Subtask brief', parent_id: parent.id }],
  });
  await page.route('**/api/v1/events?*', (route) => route.abort());
  await page.goto(`/tasks/${parent.id}`);
  const row = page.locator(`main [data-child-id="${child.id}"]`);
  await row.locator('[data-child-description]').click();
  const panel = page.getByRole('complementary', { name: 'Task details' });
  const control = panel.locator('[data-root-complete]');
  await control.click();
  await expect(row).toHaveAttribute('data-complete', 'true');
  await expect(row.locator('.task-child-complete')).toHaveAccessibleName(
    'Reopen Subtask',
  );
  await control.click();
  await expect(row).toHaveAttribute('data-complete', 'false');
  await expect(row.locator('.task-child-complete')).toHaveAccessibleName(
    'Complete Subtask',
  );
  await expect(panel).toBeVisible();
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

test('Today shows completion and reopening from the task panel', async ({ page }) => {
  const { api, capture } = await setup(page);
  const task = await capture('Planned task');
  await page.route('**/api/v1/events*', (route) => route.abort());
  await page.goto('/today');
  const date = await page
    .locator('[data-planning-today]')
    .getAttribute('data-planning-today');
  const { schedule, task_revision } = await api(`/tasks/${task.id}/schedule`);
  await api(`/tasks/${task.id}/schedule`, 'PUT', {
    revision: task_revision,
    schedule: { ...schedule, planned: { kind: 'date', date } },
  });
  await page.reload();
  const row = page.locator(`[data-planning-task="${task.id}"]`);
  await row.getByRole('link', { name: 'Planned task' }).click();
  const panel = page.getByRole('complementary', { name: 'Task details' });
  const control = panel.locator('[data-root-complete]');
  await control.click();
  await expect(row).toHaveClass(/is-complete/);
  await control.click();
  await expect(row).not.toHaveClass(/is-complete/);
  await expect(row).toHaveClass(/is-selected/);
  await expect(panel).toBeVisible();
});

test('swiping a parent task asks before completing its subtree', async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== 'chromium', 'Uses Chromium touch input protocol');
  await page.setViewportSize({ width: 390, height: 844 });
  const { capture, list, current } = await setup(page);
  const parent = await capture('Swipe parent');
  const child = await capture('Swipe child', parent.id);
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const row = page.locator(`.task[data-id="${parent.id}"]`);
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true });
  async function swipe() {
    await row.scrollIntoViewIfNeeded();
    const bounds = await row.boundingBox();
    const x = bounds.x + 100,
      y = bounds.y + bounds.height / 2;
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [{ x, y }],
    });
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ x: x + 120, y }],
    });
    await expect(row).toHaveAttribute('data-swipe-action', 'done');
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  }
  const modal = page.getByRole('dialog', { name: 'Confirm change' });
  await swipe();
  await expect(modal).toBeVisible();
  await modal.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect((await current(parent.id)).complete).toBe(false);
  expect((await current(child.id)).complete).toBe(false);
  await swipe();
  await modal.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect.poll(async () => (await current(child.id)).complete).toBe(true);
});

test('completion and reopening animate once each and respect reduced motion', async ({
  page,
}) => {
  const { capture, list } = await setup(page);
  const task = await capture('Motion feedback');
  await page.addInitScript(() => {
    const animate = Element.prototype.animate;
    window.completionAnimations = [];
    Element.prototype.animate = function (...args) {
      if (this.matches('.completion'))
        window.completionAnimations.push(this.getAttribute('aria-pressed'));
      return animate.apply(this, args);
    };
  });
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const button = page.locator(`[data-id="${task.id}"] .completion`);
  await button.click();
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  await button.click();
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  expect(await page.evaluate(() => window.completionAnimations)).toEqual([
    'true',
    'false',
  ]);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await button.click();
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  expect(await page.evaluate(() => window.completionAnimations)).toEqual([
    'true',
    'false',
  ]);
});
