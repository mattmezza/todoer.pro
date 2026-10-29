import { test, expect } from '@playwright/test';
import crypto from 'node:crypto';
const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
async function api(request, path, method = 'GET', data) {
  const r = await request.fetch(`/api/v1${path}`, {
    method,
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data,
  });
  expect(r.ok(), await r.text()).toBeTruthy();
  return r.json();
}
async function fixture(request) {
  const workspace = await api(request, '/workspaces', 'POST', {
    title: `Design ${crypto.randomUUID().slice(0, 8)}`,
  });
  const list = await api(request, '/lists', 'POST', {
    workspace_id: workspace.id,
    title: 'Weekly plan',
    color: '#15803d',
  });
  const section = await api(request, `/lists/${list.id}/sections`, 'POST', {
    title: 'Next actions',
  });
  const tasks = await api(request, '/capture', 'POST', {
    list_id: list.id,
    section_id: section.id,
    tasks: [
      { title: 'First task', description: 'A **Markdown** brief.' },
      { title: 'Second task' },
    ],
  });
  return { workspace, list, section, tasks };
}
async function signin(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
}
for (const width of [390, 1440]) {
  test(`reference interactions and planning at ${width}px`, async ({ page, request }) => {
    await page.setViewportSize({ width, height: 900 });
    const f = await fixture(request);
    await signin(page);
    await page.goto(`/lists/${f.list.id}`);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
    await expect(page.locator('.entity-color')).toHaveCSS(
      'background-color',
      'rgb(21, 128, 61)',
    );
    await expect(page.locator('#show-completed')).toBeHidden();
    await expect(page.locator('#refresh-notice')).toBeHidden();
    await expect(page.locator('[data-section-capture]')).toHaveCount(1);
    await expect(page.locator('#capture-open')).toHaveCount(1);
    const section = page.locator(`[data-section-id="${f.section.id}"]`);
    await expect(section.locator('.section-progress')).toHaveText('0%');
    await section.locator('.completion').first().click();
    await expect(section.locator('.section-progress')).toHaveText('50%');
    await expect(section.locator('.completion svg')).toHaveCount(1);
    await page.locator('[data-list-edit-open]').first().click();
    await expect(page.locator('[data-list-display]')).toBeHidden();
    await expect(page.getByPlaceholder('Description (Markdown supported)')).toBeVisible();
    await page.locator('#list-edit-title').press('Escape');
    await expect(page.locator('[data-list-display]')).toBeVisible();
    await page.locator('[data-list-edit-open]').first().click();
    await page.locator('#list-edit-title').fill('Weekly plan revised');
    await page.locator('#list-edit-description').fill('**Updated** brief.');
    await page.locator('#list-edit-description').press('Control+Enter');
    await expect(page.locator('.list-description strong')).toHaveText('Updated');
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
    await page.locator('[data-section-add-open]').click();
    await expect(page.locator('[data-section-add-open]')).toBeHidden();
    await page.locator('#new-section-title').press('Escape');
    await expect(page.locator('[data-section-add-open]')).toBeVisible();
    await page.locator('.page-heading .entity-menu>summary').click();
    await page.getByRole('button', { name: 'List settings', exact: true }).click();
    await expect(page.getByRole('dialog', { name: 'List settings' })).toBeVisible();
    await expect(page.locator('.list-settings-panel input[type=color]')).toBeVisible();
    await page.locator('.list-settings-panel input[type=color]').fill('#aa2244');
    await page.getByRole('button', { name: 'Save color', exact: true }).click();
    await expect(page.locator('.list-settings-panel>p[role=status]')).toHaveText(
      'Saved.',
    );
    await expect(page.locator('.entity-color')).toHaveCSS(
      'background-color',
      'rgb(170, 34, 68)',
    );
    await page.getByRole('tab', { name: 'Sharing', exact: true }).click();
    await expect(page.locator('.settings-content #sharing-heading')).toBeVisible();
    await page.getByRole('button', { name: '← Back' }).click();
    await expect(page.locator('.list-settings-panel')).toHaveCount(0);
    for (const path of ['/today', '/upcoming']) {
      await page.goto(path);
      await expect(page.locator('#main')).toBeVisible();
      expect(
        await page.locator('#main').evaluate((el) => el.getBoundingClientRect().width),
      ).toBeGreaterThan(width === 390 ? 300 : 700);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      ).toBe(true);
    }
    await page.goto(`/tasks/${f.tasks[0].id}`);
    await page.locator('[data-saved-description]').click();
    await expect(page.locator('#task-page-description')).toHaveValue(
      'A **Markdown** brief.',
    );
    await expect(page.locator('.task-page-brief')).toBeHidden();
    await page.locator('#task-page-description').press('Escape');
    await expect(page.locator('.task-page-brief')).toBeVisible();
    await page.screenshot({
      path: `.local/rebuild/design-feedback-${width}.png`,
      fullPage: true,
    });
  });
}
test('workspace lifecycle has two-step confirmation and retained permission-safe trash', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  await signin(page);
  await page.goto('/workspaces');
  await expect(page.locator('#navigation nav[aria-label=Lists] a')).toHaveCount(0);
  await expect(page.locator('.workspace-header .breadcrumbs')).toHaveCount(0);
  for (let n = 1; n <= 20; n++) {
    await page.goto(`/workspaces?page=${n}`);
    if (await page.locator(`[data-workspace="${f.workspace.id}"]`).count()) break;
  }
  const row = page.locator(`[data-workspace="${f.workspace.id}"]`);
  await row.getByRole('button', { name: 'Archive', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Archive', exact: true })
    .click();
  await expect(row.getByRole('button', { name: 'Restore', exact: true })).toBeVisible();
  await expect(page.locator('body')).toHaveAttribute('data-workspaces-ready', 'true');
  await row.getByRole('button', { name: 'Delete…', exact: true }).click();
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await page.getByLabel('Workspace name confirmation').fill('wrong');
  await page.getByRole('button', { name: 'Delete workspace', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('exact workspace name');
  await page.getByLabel('Workspace name confirmation').fill(f.workspace.title);
  await page.getByRole('button', { name: 'Delete workspace', exact: true }).click();
  await expect(row).toHaveCount(0);
  const response = await request.get(`/api/v1/lists/${f.list.id}`, { headers: auth });
  expect([403, 404]).toContain(response.status());
});
test('pointer grip persists sibling ordering on touch-sized screen', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  await page.setViewportSize({ width: 390, height: 900 });
  await signin(page);
  await page.goto(`/lists/${f.list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const source = page.locator(`[data-id="${f.tasks[1].id}"] [data-touch-order]`);
  const target = page.locator(`[data-id="${f.tasks[0].id}"]`);
  const from = await source.boundingBox(),
    to = await target.boundingBox();
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + to.width / 2, to.y + 3, { steps: 10 });
  await page.mouse.up();
  await expect
    .poll(async () => {
      const tasks = await api(request, `/lists/${f.list.id}/tasks`);
      return tasks[0].id;
    })
    .toBe(f.tasks[1].id);
});
test('workspace pages bound rows and navigate without account-wide sidebar lists', async ({
  page,
  request,
}) => {
  test.setTimeout(45000);
  for (let i = 0; i < 22; i++)
    await api(request, '/workspaces', 'POST', {
      title: `Pagination ${crypto.randomUUID().slice(0, 8)}`,
    });
  await signin(page);
  await page.goto('/workspaces');
  await expect(page.locator('.workspace-rows>.workspace-row')).toHaveCount(20);
  await expect(page.locator('.workspace-header .breadcrumbs')).toHaveCount(0);
  await page
    .getByRole('navigation', { name: 'Workspace pages' })
    .getByRole('link', { name: 'Next' })
    .click();
  await expect(page).toHaveURL(/page=2/);
  await expect(page.getByRole('link', { name: 'Previous', exact: true })).toBeVisible();
  expect(
    await page.locator('.workspace-rows>.workspace-row').count(),
  ).toBeLessThanOrEqual(20);
});
