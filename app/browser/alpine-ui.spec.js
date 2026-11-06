import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({ serviceWorkers: 'block' });

async function signin(page) {
  await page.addInitScript(() => {
    window.alpineViolations = [];
    document.addEventListener('securitypolicyviolation', (event) => {
      if (/^(script|style)-src/.test(event.effectiveDirective))
        window.alpineViolations.push(event.effectiveDirective);
    });
  });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
}

async function navigate(page, path) {
  await page.evaluate(async (path) => {
    const { navigate } = await import('/assets/page-navigation.js');
    await navigate(path);
  }, path);
  await expect(page.locator('#main')).not.toHaveAttribute('aria-busy', 'true');
}

test('list dialog keeps manual colors, drafts and focus across errors and swaps', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await signin(page);
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const spaceResponse = await page.request.post('/api/v1/workspaces', {
    headers: { 'X-CSRF-Token': csrf },
    data: { title: `Alpine space ${randomUUID()}` },
  });
  expect(spaceResponse.ok()).toBeTruthy();
  const space = await spaceResponse.json();
  const listResponse = await page.request.post('/api/v1/lists', {
    headers: { 'X-CSRF-Token': csrf },
    data: { workspace_id: space.id, title: 'Alpine fixture' },
  });
  expect(listResponse.ok()).toBeTruthy();
  const list = await listResponse.json();
  await navigate(page, `/lists/${list.id}`);
  const listPath = `/lists/${list.id}`;
  const dialog = page.locator('#list-create-dialog');
  let releaseSuggestion;
  const waiting = new Promise((resolve) => (releaseSuggestion = resolve));
  await page.route('**/list-color-suggestion', async (route) => {
    await waiting;
    await route.fulfill({ json: { color: '#78b500' } });
  });
  await page.locator('[data-list-create-open]').first().click();
  await expect(dialog.locator('[name=title]')).toBeFocused();
  await dialog.getByLabel('Choose list color').click();
  await dialog.getByRole('button', { name: 'Rose', exact: true }).click();
  const suggested = page.waitForResponse('**/list-color-suggestion');
  releaseSuggestion();
  await suggested;
  await expect(dialog.locator('[name=color]')).toHaveValue('#f43f5e');
  await dialog.getByLabel('Choose list color').click();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel('Choose list color')).toBeFocused();
  await expect(dialog.locator('.list-color-picker')).not.toHaveAttribute('open', '');

  const title = `Alpine ${randomUUID()}`;
  await dialog.locator('[name=title]').fill(title);
  await dialog.locator('[name=description]').fill('Keep this draft on failure.');
  let writes = 0;
  await page.route('**/api/v1/lists', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    writes++;
    await route.fulfill({
      status: 409,
      json: { error: { message: 'Synthetic create conflict' } },
    });
  });
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('Synthetic create conflict');
  await expect(dialog.locator('[name=title]')).toHaveValue(title);
  await expect(dialog.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  expect(writes).toBe(1);
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.unroute('**/api/v1/lists');
  await page.unroute('**/list-color-suggestion');

  for (const path of ['/today', '/settings', '/inbox']) {
    await navigate(page, path);
    await navigate(page, listPath);
    await page.locator('[data-list-create-open]').first().click();
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('.list-color-picker')).toHaveCount(1);
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  }
  const created = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/v1/lists') && response.request().method() === 'POST',
  );
  await page.locator('[data-list-create-open]').first().click();
  await dialog.locator('[name=title]').fill(title);
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  const result = await (await created).json();
  expect(result.id).toBeTruthy();
  await expect(page).toHaveTitle(`#${title} · Todoer`);
  expect(errors).toEqual([]);
  expect(await page.evaluate(() => window.alpineViolations)).toEqual([]);
});

test('confirmation requests stay queued and settle when their page is replaced', async ({
  page,
}) => {
  await signin(page);
  await page.evaluate(async () => {
    const { confirmAction } = await import('/assets/confirm.js');
    window.confirmed = [];
    void confirmAction('Apply the first change?', {
      title: 'First change',
      acceptLabel: 'Apply',
    }).then((result) => window.confirmed.push(result));
    void confirmAction('Apply the second change?', { title: 'Second change' }).then(
      (result) => window.confirmed.push(result),
    );
  });
  const first = page.getByRole('dialog', { name: 'First change', exact: true });
  await expect(first.getByRole('button', { name: 'Cancel', exact: true })).toBeFocused();
  await first.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'Second change' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect.poll(() => page.evaluate(() => window.confirmed)).toEqual([true, false]);
  await page.evaluate(async () => {
    const { confirmAction } = await import('/assets/confirm.js');
    void confirmAction('Keep waiting?', { title: 'Leaving this page' }).then((result) =>
      window.confirmed.push(result),
    );
  });
  await expect(page.getByRole('dialog', { name: 'Leaving this page' })).toBeVisible();
  await navigate(page, '/today');
  await expect
    .poll(() => page.evaluate(() => window.confirmed))
    .toEqual([true, false, false]);
  await expect(page.locator('[data-confirmation-dialog]')).toHaveCount(1);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('completed-task visibility persists, filters server HTML and synchronizes tabs', async ({
  page,
  context,
}) => {
  await signin(page);
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const title = `Alpine completed ${randomUUID()}`;
  const captured = await page.request.post('/api/v1/capture', {
    headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
    data: { list_id: 'inbox', tasks: [{ title }] },
  });
  expect(captured.ok()).toBeTruthy();
  const [task] = await captured.json();
  const completed = await page.request.patch(`/api/v1/tasks/${task.id}`, {
    headers: { 'X-CSRF-Token': csrf },
    data: { revision: task.revision, complete: true },
  });
  expect(completed.ok()).toBeTruthy();
  await navigate(page, '/lists/inbox');
  const row = page.locator(`[data-id="${task.id}"]`);
  await expect(row).toBeVisible();
  await page.locator('.page-heading .entity-menu > summary').click();
  await page.getByRole('button', { name: 'Hide completed', exact: true }).click();
  await expect(row).not.toBeVisible();
  const fragment = await page.request.get('/fragments/lists/inbox/tasks');
  expect(await fragment.text()).not.toContain(title);

  const other = await context.newPage();
  await other.goto(new URL('/lists/inbox', page.url()).href);
  await expect(other.locator('[data-toggle-completed]')).toHaveText('Show completed');
  await expect(other.locator(`[data-id="${task.id}"]`)).toHaveCount(0);
  await other.locator('.page-heading .entity-menu > summary').click();
  await other.getByRole('button', { name: 'Show completed', exact: true }).click();
  await expect(other.locator(`[data-id="${task.id}"]`)).toBeVisible();
  await expect(row).toBeVisible();
  await expect(page.locator('[data-toggle-completed]')).toHaveText('Hide completed');
  await other.close();
});

test('search renders plain text, tracks keyboard selection and reopens after navigation', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await signin(page);
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const captured = await page.request.post('/api/v1/capture', {
    headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: 'inbox',
      tasks: [{ title: 'Search first' }, { title: 'Search second' }],
    },
  });
  expect(captured.ok()).toBeTruthy();
  const tasks = await captured.json();
  await page.route('**/api/v1/query?**', (route) =>
    route.fulfill({
      json: {
        total: 2,
        hits: tasks.map((task, index) => ({
          ...task,
          kind: 'task',
          title: index ? 'Second result' : 'First <b>plain text</b> result',
        })),
      },
    }),
  );
  await page.mouse.move(0, 0);
  await page.keyboard.press('Control+k');
  const dialog = page.getByRole('dialog', { name: 'Quick search' });
  const input = dialog.getByRole('combobox', { name: 'Search everything' });
  await input.fill('search');
  await expect(dialog.getByRole('row')).toHaveCount(2);
  await expect(dialog.locator('strong').first()).toHaveText(
    'First <b>plain text</b> result',
  );
  await expect(dialog.locator('b')).toHaveCount(0);
  await expect(input).toHaveAttribute('aria-activedescendant', 'omnibar-result-0');
  await input.press('ArrowDown');
  await expect(input).toHaveAttribute('aria-activedescendant', 'omnibar-result-1');
  await expect(dialog.getByRole('row').nth(1)).toHaveAttribute('aria-selected', 'true');
  await input.press('Enter');
  await expect(page.locator('[data-task-page]')).toHaveAttribute(
    'data-task-id',
    tasks[1].id,
  );
  await page.keyboard.press('/');
  await expect(dialog).toBeVisible();
  await expect(input).toHaveValue('');
  await expect(dialog.getByRole('row')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(dialog).not.toBeVisible();
  expect(errors).toEqual([]);
});

test('export keeps its format controls usable after errors and downloads the selected report', async ({
  page,
}) => {
  await signin(page);
  let failing = true;
  const requests = [];
  await page.route('**/api/v1/reports/export?**', async (route) => {
    requests.push(new URL(route.request().url()).searchParams.get('format'));
    await route.fulfill(
      failing
        ? { status: 503, json: { error: { message: 'Synthetic export unavailable' } } }
        : { contentType: 'text/csv', body: 'title\nSynthetic export\n' },
    );
  });
  await page.evaluate(async () => {
    const { openExport } = await import('/assets/export-menu.js');
    openExport({ kind: 'list', id: 'inbox' });
  });
  const dialog = page.getByRole('dialog', { name: 'Export', exact: true });
  await dialog.getByRole('button', { name: 'CSV', exact: true }).click();
  await expect(dialog.locator('.quiet-info')).toHaveAttribute('data-save-state', 'error');
  await expect(dialog.getByRole('button', { name: 'CSV', exact: true })).toBeEnabled();
  failing = false;
  const downloaded = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'CSV', exact: true }).click();
  expect((await downloaded).suggestedFilename()).toBe('todoer-list.csv');
  await expect(dialog).not.toBeVisible();
  expect(requests).toEqual(['csv', 'csv']);
});

test('workspace actions preserve two-stage deletion and recover from a wrong name', async ({
  page,
}) => {
  await signin(page);
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const title = `Alpine workspace ${randomUUID()}`;
  const response = await page.request.post('/api/v1/workspaces', {
    headers: { 'X-CSRF-Token': csrf },
    data: { title },
  });
  expect(response.ok()).toBeTruthy();
  const space = await response.json();
  await navigate(page, '/spaces?archived=true');
  const row = page.locator(`[data-workspace="${space.id}"]`);
  await row.getByRole('button', { name: 'Archive', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Archive', exact: true })
    .click();
  await expect(row.getByRole('button', { name: 'Restore', exact: true })).toBeVisible();
  await row.getByRole('button', { name: 'Delete…', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'Continue', exact: true }).click();
  await expect(dialog.getByLabel('Workspace name confirmation')).toBeFocused();
  await dialog.getByLabel('Workspace name confirmation').fill('wrong');
  await dialog.getByRole('button', { name: 'Delete workspace', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('exact workspace name');
  await dialog.getByLabel('Workspace name confirmation').fill(title);
  await dialog.getByRole('button', { name: 'Delete workspace', exact: true }).click();
  await expect(row).toHaveCount(0);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});
