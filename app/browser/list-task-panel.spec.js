import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

for (const width of [390, 1280]) {
  test(`list rows share task panel and right-click brief editing at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/lists/inbox');
    const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
    const created = await page.request.post('/api/v1/capture', {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data: {
        list_id: 'inbox',
        tasks: [{ title: `Row ${randomUUID()}`, description: 'Original brief' }],
      },
    });
    expect(created.ok()).toBe(true);
    const [task] = await created.json();
    await page.reload();
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    const listUrl = page.url();
    const row = page.locator(`.task[data-id="${task.id}"]`);
    await row.locator('.task-title').click({ button: 'right' });
    await row.getByLabel('Task title', { exact: true }).fill('Edited inline');
    await row
      .getByLabel('Task description', { exact: true })
      .fill('Updated inline description');
    await expect(row.getByRole('checkbox')).toHaveCount(0);
    await row.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(row.locator('.task-title')).toHaveText('Edited inline');
    await expect(row.locator('.task-description')).toHaveText(
      'Updated inline description',
    );
    await row.click({ position: { x: 16, y: 6 } });
    const panel = page.getByRole('complementary', { name: 'Task details', exact: true });
    await expect(panel.locator('[data-task-page]')).toBeVisible();
    await expect(panel.locator('[data-edit-title]')).toHaveText('Edited inline');
    await expect(page).toHaveURL(listUrl);
    expect(await panel.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
    if (width === 390) {
      const bell = await page.locator('[data-notification-toggle]').boundingBox();
      const close = await page
        .getByRole('button', { name: 'Close task details', exact: true })
        .boundingBox();
      expect(bell.x + bell.width).toBeLessThanOrEqual(close.x);
    }
    await page.getByRole('button', { name: 'Close task details', exact: true }).click();
    await expect(panel).toHaveCount(0);
    await row.getByLabel('Actions for Edited inline', { exact: true }).click();
    await expect(row.locator('[data-action=edit] .ui-icon')).toHaveCount(1);
    await row.getByRole('button', { name: 'Open details', exact: true }).click();
    await expect(panel.locator('[data-task-page]')).toBeVisible();
    await page.getByRole('button', { name: 'Close task details', exact: true }).click();
    await expect(panel).toHaveCount(0);
    await page.evaluate(() => {
      window.listDocument = document;
    });
    await row.locator('.task-title').click();
    await expect(page.locator('main[data-task-page]')).toBeVisible();
    await expect(page).toHaveURL(/\/tasks\//);
    expect(await page.evaluate(() => window.listDocument === document)).toBe(true);
  });

  test(`task panel uses a stable spinner and supports retry at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
    const created = await page.request.post('/api/v1/capture', {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data: {
        list_id: 'inbox',
        tasks: [{ title: `Loading ${randomUUID()}` }],
      },
    });
    expect(created.ok()).toBe(true);
    const [task] = await created.json();
    await page.goto('/lists/inbox');
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
    const row = page.locator(`.task[data-id="${task.id}"]`);
    const href = await row.locator('.task-title').getAttribute('href');
    let release;
    const delayed = new Promise((resolve) => {
      release = resolve;
    });
    let fail = true;
    await page.route(`**${href}`, async (route) => {
      await delayed;
      if (fail) await route.fulfill({ status: 503, body: 'Unavailable' });
      else await route.continue();
    });
    await row.click({ position: { x: 16, y: 6 } });
    const panel = page.getByRole('complementary', { name: 'Task details', exact: true });
    const content = panel.locator('.planning-detail-content');
    const spinner = content.locator('.loading-spinner');
    await expect(content).toHaveAttribute('aria-busy', 'true');
    await expect(spinner).toBeVisible();
    await expect(content.locator('.skeleton-line, .skeleton-row')).toHaveCount(0);
    await expect(
      page.getByRole('button', { name: 'Close task details', exact: true }),
    ).toBeVisible();
    expect(await spinner.evaluate((el) => getComputedStyle(el).animationName)).toBe(
      'none',
    );
    const geometry = await panel.boundingBox();
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    expect(await spinner.evaluate((el) => getComputedStyle(el).animationName)).toBe(
      'quiet-saving-spin',
    );
    release();
    await expect(content.getByRole('alert')).toContainText('Could not open this task.');
    await expect(spinner).toHaveCount(0);
    await expect(content).not.toHaveAttribute('aria-busy');
    fail = false;
    await content.getByRole('button', { name: 'Try again' }).click();
    await expect(panel.locator('[data-edit-title]')).toHaveText(task.title);
    await expect(content.locator('.loading-error, .loading-spinner')).toHaveCount(0);
    expect(await panel.boundingBox()).toEqual(geometry);
    await page.unrouteAll({ behavior: 'wait' });
  });
}

test('switching task panels avoids duplicate reads and defers hidden metadata', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: 'inbox',
      tasks: [{ title: `Panel A ${randomUUID()}` }, { title: `Panel B ${randomUUID()}` }],
    },
  });
  expect(response.ok()).toBeTruthy();
  const tasks = await response.json();
  await page.goto('/lists/inbox');
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const reads = [];
  page.on('request', (request) => {
    if (request.method() === 'GET' || request.url().endsWith('/subtree/query'))
      reads.push(new URL(request.url()).pathname);
  });
  const panel = page.getByRole('complementary', { name: 'Task details', exact: true });
  for (const task of [tasks[0], tasks[1], tasks[0]]) {
    const start = reads.length;
    const row = page.locator(`.task[data-id="${task.id}"]`);
    const href = await row.locator('.task-title').getAttribute('href');
    await row.click({ position: { x: 16, y: 6 } });
    await expect(panel.locator('[data-edit-title]')).toHaveText(task.title);
    await expect(panel.locator('[data-connection]')).toHaveText('Up to date');
    await expect(panel.getByLabel('Change priority')).toHaveText('P3');
    await expect(panel.locator('[data-discussion-fragment]')).toBeVisible();
    const opened = reads.slice(start);
    expect(opened.filter((path) => path.startsWith('/tasks/'))).toEqual([href]);
    expect(opened.filter((path) => path.endsWith('/schedule'))).toHaveLength(1);
    expect(opened.filter((path) => path.endsWith('/subtree/query'))).toHaveLength(1);
    expect(opened.filter((path) => path.endsWith('/tags'))).toHaveLength(1);
    expect(
      opened.filter((path) =>
        /\/subtree$|\/section$|\/sections$|\/references\/|\/description$|\/color$/.test(
          path,
        ),
      ),
    ).toEqual([]);
  }
  expect(reads.filter((path) => path.endsWith('/notification-preferences'))).toHaveLength(
    1,
  );
  await panel.getByLabel('Task actions').click();
  await panel.getByRole('button', { name: 'Task settings…', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Task details', exact: true });
  await settings.getByRole('tab', { name: 'Tags', exact: true }).click();
  await expect(settings.locator('[name=new_tags]')).toBeEnabled();
  await settings.locator('[name=new_tags]').fill('panel-regression');
  await settings.locator('[name=new_tags]').press('Enter');
  await expect(settings.locator('[data-tag-chips]')).toContainText('+panel-regression');
  const saved = await (
    await page.request.get(`/api/v1/tasks/${tasks[0].id}/tags`)
  ).json();
  expect(saved.map((tag) => tag.name)).toContain('panel-regression');
});
