import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({ serviceWorkers: 'block' });

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
    expect(response.ok(), await response.text()).toBe(true);
    return response.json();
  };
  const space = await api('/workspaces', 'POST', { title: `Mobile ${randomUUID()}` });
  const list = await api('/lists', 'POST', { workspace_id: space.id, title: 'Mobile' });
  const capture = async (title, description = '', parent_id = null) =>
    (
      await api('/capture', 'POST', {
        list_id: list.id,
        tasks: [{ title, description, parent_id }],
      })
    )[0];
  const task = await capture(
    'Buy milk right now from coop or migros for the love of god',
  );
  const child = await capture(
    'Child title that needs plenty of space',
    'Child description',
    task.id,
  );
  const grandchild = await capture('Nested child', 'Nested description', child.id);
  await api(`/tasks/${task.id}/comments`, 'POST', { body: 'A comment' });
  await api(`/tasks/${child.id}/comments`, 'POST', { body: 'Child comment' });
  return { api, space, list, task, child, grandchild };
}
async function noOverflow(page) {
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(await page.evaluate(() => innerWidth));
}
async function openCommentComposer(surface) {
  const composer = surface.locator('[data-comment-composer]');
  // Returning to the task restores its local comment draft and opens the form.
  if (!(await composer.evaluate((el) => el.open)))
    await composer.locator(':scope > summary').click();
}
async function alignedDescription(row) {
  const title = await row.locator(':scope > .task-page-child-line > a').boundingBox();
  const brief = await row.locator(':scope > [data-child-description]').boundingBox();
  expect(Math.abs(title.x - brief.x)).toBeLessThanOrEqual(1);
}

test('capture close target stays clear of the title and description fields', async ({
  page,
}) => {
  await fixture(page);
  await page.evaluate(async () => {
    const base = document.querySelector('meta[name=asset-base]').content;
    await (await import(base + 'quick-capture.js')).openCapture();
  });
  const dialog = page.locator('dialog.capture-omnibar:has([name=title])');
  for (const width of [320, 390, 800, 1280]) {
    await page.setViewportSize({ width, height: 844 });
    const close = await dialog
      .getByRole('button', { name: 'Close capture' })
      .boundingBox();
    for (const name of ['title', 'description']) {
      const field = await dialog.locator(`[name=${name}]`).boundingBox();
      expect(close.y + close.height).toBeLessThanOrEqual(field.y);
    }
    await noOverflow(page);
  }
  await dialog.getByRole('button', { name: 'Close capture' }).click();
  await expect(dialog).not.toBeVisible();
});

test('mobile task titles have room, counters stack, and child briefs align on every surface', async ({
  page,
}) => {
  test.setTimeout(90000);
  const f = await fixture(page);
  for (const width of [320, 390, 640]) {
    await page.setViewportSize({ width, height: 844 });
    await page.goto(`/lists/${f.list.id}`);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
    const row = page.locator(`.task[data-id="${f.task.id}"]`);
    await expect(row).toBeVisible();
    const box = await row.boundingBox();
    const title = await row.locator('.task-content').boundingBox();
    expect(title.x - box.x).toBeLessThanOrEqual(48);
    expect(title.width).toBeGreaterThanOrEqual(width - 164);
    const counters = row.locator('.task-row-count');
    const first = await counters.nth(0).boundingBox();
    const second = await counters.nth(1).boundingBox();
    expect(Math.abs(first.x - second.x)).toBeLessThanOrEqual(1);
    expect(second.y).toBeGreaterThanOrEqual(first.y + first.height);
    await noOverflow(page);
    await row.locator('.task-menu').click();
    await row.locator('[data-action=edit]').click();
    const panel = page.getByRole('complementary', { name: 'Task details', exact: true });
    await expect(panel.locator(`[data-child-id="${f.child.id}"]`)).toBeVisible();
    await alignedDescription(panel.locator(`[data-child-id="${f.child.id}"]`));
    await alignedDescription(panel.locator(`[data-child-id="${f.grandchild.id}"]`));
    await openCommentComposer(panel);
    await panel.locator('[data-new-comment-input]').fill('longword'.repeat(100));
    await noOverflow(page);
    for (const task of [f.task, f.child]) {
      await page.goto(`/tasks/${task.id}`);
      await expect(page.locator('[data-connection]')).toHaveText('Up to date');
      const childId = task.id === f.task.id ? f.child.id : f.grandchild.id;
      const childRow = page.locator(`[data-child-id="${childId}"]`);
      const childBox = await childRow.boundingBox();
      expect(childBox.x).toBeLessThanOrEqual(17);
      await alignedDescription(childRow);
      await openCommentComposer(page);
      await page.locator('[data-new-comment-input]').fill('longword'.repeat(100));
      await noOverflow(page);
    }
  }
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(`/tasks/${f.task.id}`);
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  await alignedDescription(page.locator(`[data-child-id="${f.child.id}"]`));
});

test('empty descriptions invite editing on lists, spaces, tasks and subtasks', async ({
  page,
}) => {
  const f = await fixture(page);
  const [emptyChild] = await f.api('/capture', 'POST', {
    list_id: f.list.id,
    tasks: [{ title: 'Empty child', parent_id: f.task.id }],
  });
  for (const [url, placeholder, input] of [
    [`/lists/${f.list.id}`, '.list-description', '[data-list-edit] [name=description]'],
    [
      `/space/${f.space.id}`,
      '.space-overview-description',
      '[data-space-edit-form] [name=description]',
    ],
    [
      `/tasks/${f.task.id}`,
      '[data-edit-description]',
      '[data-brief-form] [name=description]',
    ],
    [
      `/tasks/${emptyChild.id}`,
      '[data-edit-description]',
      '[data-brief-form] [name=description]',
    ],
  ]) {
    await page.goto(url);
    await expect(page.locator(placeholder)).toHaveText('Add description');
    await page.locator(placeholder).click();
    await expect(page.locator(input)).toBeFocused();
  }
});
