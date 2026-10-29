import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';
const token = 'local-integration-token-not-for-production';
async function api(page, path, method = 'GET', data) {
  const r = await page.request.fetch(`/api/v1${path}`, {
    method,
    data,
    headers: { Authorization: `Bearer ${token}`, 'Idempotency-Key': randomUUID() },
  });
  expect(r.ok(), await r.text()).toBeTruthy();
  return r.json();
}
async function setup(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const workspace = await api(page, '/workspaces', 'POST', {
    title: `Space ${randomUUID().slice(0, 8)}`,
  });
  await page.goto(`/space/${workspace.id}`);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  return { workspace, list: await page.locator('body').getAttribute('data-list') };
}
test('space opens its default capture list and exposes tabbed settings', async ({
  page,
}) => {
  const { workspace, list } = await setup(page);
  expect(workspace.id).toMatch(/^s_[A-Za-z0-9_-]{12}$/);
  expect(list).toMatch(/^l_[A-Za-z0-9_-]{12}$/);
  await expect(page).toHaveURL(new RegExp(`/space/${workspace.id}$`));
  await expect(page.locator('.page-heading h1')).toHaveText('#inbox');
  await expect(page.locator('.section-heading')).toHaveCount(0);
  await page.locator('#capture-open').click();
  await page.locator('#capture-title').fill('First task');
  await page.locator('#capture-title').press('Control+Enter');
  await expect(
    page.locator('.task-title').filter({ hasText: 'First task' }),
  ).toBeVisible();
  expect(await page.locator('.task').first().getAttribute('data-id')).toMatch(
    /^t_[A-Za-z0-9_-]{12}$/,
  );
  await page.getByLabel('List actions', { exact: true }).click();
  await page.getByRole('button', { name: 'Space settings', exact: true }).click();
  const panel = page.getByRole('dialog', { name: 'Space settings' });
  await expect(panel).toBeVisible();
  await expect(panel).toHaveCSS('background-color', 'rgb(11, 16, 27)');
  await expect(panel.getByRole('tab', { name: 'Details' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await panel.getByRole('tab', { name: 'Sharing' }).click();
  await expect(panel.getByRole('heading', { name: 'Workspace members' })).toBeVisible();
  await panel.getByRole('tab', { name: 'Lists', exact: true }).click();
  await expect(panel.locator('#lists-heading')).toBeVisible();
  await panel.getByRole('button', { name: 'Back', exact: false }).click();
  await expect(panel).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`/space/${workspace.id}$`));
});
test('task brief edits, subtask composer, comments and confirmations use the reference presentation', async ({
  page,
}) => {
  const dialogs = [];
  page.on('dialog', async (d) => {
    dialogs.push(d.type());
    await d.dismiss();
  });
  const { list } = await setup(page);
  const [task] = await api(page, '/capture', 'POST', {
    list_id: list,
    tasks: [{ title: 'A long task '.repeat(20), description: 'Saved **Markdown**' }],
  });
  await page.goto(`/tasks/${task.id}`);
  await page.waitForFunction(
    () => document.querySelector('[data-connection]')?.textContent === 'Up to date',
  );
  const root = page.locator('[data-task-page]');
  await expect(page.locator('.breadcrumbs')).toHaveCSS('flex-wrap', 'nowrap');
  await page.locator('[data-edit-title]').click();
  await expect(page.locator('[data-copy-task-reference]')).toBeHidden();
  await expect(page.locator('.task-context-menu')).toBeHidden();
  await expect(page.locator('#task-page-description')).toBeHidden();
  await expect(page.locator('[data-saved-description]')).toBeVisible();
  await page.locator('#task-page-title').press('Escape');
  await page.locator('[data-saved-description]').click();
  await expect(page.locator('#task-page-title')).toBeHidden();
  await expect(page.locator('[data-brief-form]')).toHaveCSS('border-top-width', '0px');
  await page.locator('#task-page-description').press('Escape');
  await page.locator('[data-child-composer] > summary').click();
  await expect(page.locator('#task-child-title')).toBeFocused();
  await expect(page.locator('#task-child-description')).toBeVisible();
  await expect(page.locator('[data-child-form]')).toHaveCSS('border-top-width', '0px');
  await page.locator('#task-child-description').press('Escape');
  await expect(page.locator('[data-child-composer] > summary')).toBeVisible();
  await page.locator('[data-comment-composer] > summary').click();
  const text = page.locator('[data-new-comment-input]');
  await text.fill('A useful **comment**');
  await text.press('Control+Enter');
  await expect(page.locator('[data-comment-rendered] strong')).toHaveText('comment');
  await expect(page.locator('.comment-author')).toHaveText('alice');
  await expect(page.locator('.comment-time')).toHaveText('now');
  await page.locator('.comment-actions > summary').click();
  await page.locator('[data-comment-delete]').click();
  const confirmation = page.getByRole('dialog', { name: 'Confirm change' });
  await expect(confirmation).toBeVisible();
  await confirmation.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('[data-comment-rendered] strong')).toBeVisible();
  await page.locator('.task-context-menu > summary').click();
  await page.locator('[data-open-advanced]').click();
  const panel = page.getByRole('dialog', { name: 'Task details', exact: true });
  await expect(panel).toBeVisible();
  await expect(panel.getByRole('tab', { name: 'Planning', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await panel.getByRole('tab', { name: 'Tags & references' }).click();
  await expect(panel.locator('[data-metadata]')).toBeVisible();
  await panel.getByRole('button', { name: 'Back', exact: false }).click();
  await expect(panel).toHaveCount(0);
  await expect(root.locator('[data-task-advanced]')).toBeHidden();
  expect(dialogs).toEqual([]);
});
test('phone layouts, panel back navigation and row actions remain aligned', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { list } = await setup(page);
  await api(page, '/capture', 'POST', {
    list_id: list,
    tasks: [
      { title: 'A task', description: 'Multiple lines of context '.repeat(12) },
      { title: 'Another task' },
    ],
  });
  await page.reload();
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  const row = page.locator('.task').first();
  const grip = await row.locator('.task-drag-grip').boundingBox();
  const menu = await row.locator('.task-menu').boundingBox();
  expect(Math.abs(grip.y + grip.height / 2 - menu.y - menu.height / 2)).toBeLessThan(2);
  await row.locator('.task-menu').click();
  expect((await row.locator('.menu-items').boundingBox()).width).toBeGreaterThanOrEqual(
    240,
  );
  await row.locator('.task-menu').click();
  await page.getByLabel('List actions', { exact: true }).click();
  await page.getByRole('button', { name: 'List settings', exact: true }).click();
  const panel = page.getByRole('dialog', { name: 'List settings', exact: true });
  await expect(panel).toBeVisible();
  expect((await panel.boundingBox()).width).toBe(390);
  await panel.press('Escape');
  await expect(panel).toHaveCount(0);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(390);
});

test('global omnibar debounces authorized search and links to the filtered overview', async ({
  page,
}) => {
  const { list } = await setup(page);
  const needle = `omni${randomUUID().replaceAll('-', '').slice(0, 10)}`;
  const [task] = await api(page, '/capture', 'POST', {
    list_id: list,
    tasks: [{ title: `${needle} milk`, description: 'A **useful** shopping note' }],
  });
  await api(page, `/tasks/${task.id}`, 'PATCH', {
    revision: task.revision,
    complete: true,
  });
  let searches = 0;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/v1/query') searches++;
  });
  await page.keyboard.press('Control+k');
  const dialog = page.getByRole('dialog', { name: 'Quick search' });
  await expect(dialog).toBeVisible();
  const input = page.getByRole('combobox', { name: 'Search everything' });
  await input.fill(needle.slice(0, 3));
  await input.fill(needle.slice(0, 6));
  await input.fill(needle);
  await expect(dialog.getByRole('option')).toHaveCount(1);
  expect(searches).toBe(1);
  await expect(dialog.getByRole('option')).toContainText('completed');
  await input.press('Enter');
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}$`));
  await page.keyboard.press('/');
  const reopened = page.getByRole('dialog', { name: 'Quick search' });
  await expect(reopened).toBeVisible();
  await page.getByRole('combobox', { name: 'Search everything' }).fill(needle);
  await expect(reopened.getByRole('option')).toHaveCount(1);
  await reopened.getByRole('link', { name: 'View all results & filters →' }).click();
  await expect(page).toHaveURL(new RegExp('/search\\?query=' + needle));
  await expect(page.getByText('Filters', { exact: true })).toBeVisible();
});

test('spaces contextual add creates the complete hierarchy and destructive dialogs use a red action row', async ({
  page,
}) => {
  await setup(page);
  await page.goto('/spaces');
  await page.waitForFunction(() => document.body.dataset.workspacesReady === 'true');
  await expect(page.locator('#workspace-title')).toBeHidden();
  await page.getByRole('button', { name: 'Add space', exact: true }).click();
  const modal = page.getByRole('dialog', { name: 'Add a new space' });
  await expect(modal).toBeVisible();
  await expect(modal.locator('[name=title]')).toBeFocused();
  await modal.locator('[name=title]').fill(`New space ${randomUUID().slice(0, 8)}`);
  await modal.locator('[name=title]').press('Control+Enter');
  await expect(page).toHaveURL(/\/space\/s_[A-Za-z0-9_-]{12}$/);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  await expect(page.locator('.page-heading h1')).toHaveText('#inbox');
  const id = (await page.url()).split('/').pop();
  await page.goto('/spaces');
  await page.waitForFunction(() => document.body.dataset.workspacesReady === 'true');
  // The isolated account has many fixture spaces; navigate directly to the page containing this one.
  const row = page.locator(`[data-workspace="${id}"]`);
  while (!(await row.count())) {
    const next = page.getByRole('link', { name: 'Next', exact: true });
    await expect(next).toBeVisible();
    await next.click();
    await page.waitForFunction(() => document.body.dataset.workspacesReady === 'true');
  }
  await row.getByRole('button', { name: 'Archive', exact: true }).click();
  const confirmation = page.getByRole('dialog').filter({ hasText: 'Archive workspace?' });
  await expect(confirmation).toBeVisible();
  await expect(
    confirmation.getByRole('button', { name: 'Archive', exact: true }),
  ).toHaveCSS('background-color', 'rgb(220, 38, 38)');
  const cancel = await confirmation
    .getByRole('button', { name: 'Cancel', exact: true })
    .boundingBox();
  const archive = await confirmation
    .getByRole('button', { name: 'Archive', exact: true })
    .boundingBox();
  expect(Math.abs(cancel.y - archive.y)).toBeLessThan(2);
  await confirmation.getByRole('button', { name: 'Cancel', exact: true }).click();
});
