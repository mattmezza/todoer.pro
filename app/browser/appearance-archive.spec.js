import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.beforeEach(async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
});

test('appearance previews both fonts and persists a selection across pages', async ({
  page,
}) => {
  await page.goto('/settings/appearance');
  await expect(
    page.getByRole('heading', { name: 'Appearance', exact: true }),
  ).toBeVisible();
  expect(
    await page
      .locator('.appearance-inter')
      .evaluate((el) => getComputedStyle(el).fontFamily),
  ).toContain('Inter');
  expect(
    await page
      .locator('.appearance-geist')
      .evaluate((el) => getComputedStyle(el).fontFamily),
  ).toContain('Geist');
  await page.getByRole('radio', { name: /Inter/ }).check();
  await page.getByRole('button', { name: 'Save appearance' }).click();
  await expect(page.getByRole('radio', { name: /Inter/ })).toBeChecked();
  await page.goto('/spaces');
  await expect(page.locator('#navigation')).toHaveAttribute('data-font', 'inter');
  expect(
    await page.locator('body').evaluate((el) => getComputedStyle(el).fontFamily),
  ).toContain('Inter');
  await page.goto('/settings/appearance');
  await page.getByRole('radio', { name: /Geist/ }).check();
  await page.getByRole('button', { name: 'Save appearance' }).click();
  await expect(page.locator('#navigation')).toHaveAttribute('data-font', 'geist');
});

test('archived lists are separate, show provenance, and can be restored', async ({
  page,
}) => {
  await page.goto('/spaces');
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const headers = { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() };
  const response = await page.request.post('/api/v1/workspaces', {
    headers,
    data: { title: `Archive ${randomUUID().slice(0, 8)}`, description: '' },
  });
  expect(response.ok()).toBeTruthy();
  const space = await response.json();
  const listResponse = await page.request.post('/api/v1/lists', {
    headers: { ...headers, 'Idempotency-Key': randomUUID() },
    data: {
      workspace_id: space.id,
      title: 'Archived example',
      color: '#ff1234',
      description: 'Keep these notes',
      restricted: false,
    },
  });
  expect(listResponse.ok()).toBeTruthy();
  const list = await listResponse.json();
  const captured = await page.request.post('/api/v1/capture', {
    headers: { ...headers, 'Idempotency-Key': randomUUID() },
    data: { list_id: list.id, tasks: [{ title: 'Retained task', description: '' }] },
  });
  expect(captured.ok()).toBeTruthy();
  const [task] = await captured.json();
  const children = await page.request.post('/api/v1/capture', {
    headers: { ...headers, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: list.id,
      tasks: [{ title: 'Retained subtask', description: '', parent_id: task.id }],
    },
  });
  expect(children.ok()).toBeTruthy();
  const [child] = await children.json();
  const favorite = await page.request.put('/api/v1/favorites', {
    headers,
    data: { kind: 'list', id: list.id, favorite: true },
  });
  expect(favorite.ok()).toBeTruthy();
  const noScripts = await page
    .context()
    .browser()
    .newContext({ javaScriptEnabled: false });
  await noScripts.addCookies(await page.context().cookies());
  const initialPage = await noScripts.newPage();
  await initialPage.goto(new URL('/today', page.url()).href);
  const dot = initialPage.locator(
    `nav[aria-label=Favorites] a[href="/lists/${list.id}"] .list-dot`,
  );
  await expect(dot.locator('circle')).toHaveAttribute('fill', '#ff1234');
  await expect(dot).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  await noScripts.close();
  const archived = await page.request.patch(`/api/v1/lists/${list.id}`, {
    headers,
    data: { revision: list.revision, archived: true },
  });
  expect(archived.ok()).toBeTruthy();
  await page.goto('/today');
  await expect(
    page.locator(`nav[aria-label=Favorites] a[href="/lists/${list.id}"]`),
  ).toHaveCount(0);
  await page.goto(`/space/${space.id}`);
  await expect(page.locator('[data-archived-lists]')).toBeHidden();
  await page.getByRole('checkbox', { name: 'Show archived lists' }).check();
  await expect(page.locator('[data-archived-lists]')).toContainText('Archived by you on');
  await page.locator('[data-archived-lists] a').click();
  await expect(page.getByText('Archived list · read-only')).toBeVisible();
  await expect(page.locator('body')).toHaveAttribute('data-can-write', 'false');
  await expect(page.locator('[data-list-edit-open]').first()).toBeDisabled();
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  const banner = page.getByRole('region', { name: 'Archived list', exact: true });
  const bounds = await banner.boundingBox();
  expect(bounds.x).toBe(0);
  expect(bounds.width).toBe(page.viewportSize().width);
  expect(bounds.height).toBeLessThanOrEqual(36);
  expect(bounds.y).toBe(0);
  await page.goto(`/tasks/${task.id}`);
  await expect(banner).toBeVisible();
  await expect(page.locator('[data-task-page]')).toHaveAttribute(
    'data-can-write',
    'false',
  );
  for (const current of [task, child]) {
    await page.goto(`/tasks/${current.id}`);
    await page.locator('.task-context-menu > summary').click();
    await page.locator('[data-open-advanced]').click();
    const panel = page.getByRole('dialog', { name: 'Task details', exact: true });
    for (const name of current === child
      ? ['Planning', 'Tags', 'Structure']
      : ['Planning', 'Tags']) {
      await panel.getByRole('tab', { name, exact: true }).click();
      await expect(panel.locator('.settings-content')).toContainText(
        'This list is archived. These settings are read-only.',
      );
    }
    await panel.getByRole('button', { name: '← Back', exact: true }).click();
  }
  await expect(banner).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  const mobileBounds = await banner.boundingBox();
  expect(mobileBounds.x).toBe(0);
  expect(mobileBounds.width).toBe(390);
  expect(mobileBounds.height).toBeLessThanOrEqual(36);
  const textBounds = await banner.locator('span').first().boundingBox();
  const buttonBounds = await page.locator('[data-list-restore]').boundingBox();
  expect(buttonBounds.x).toBeGreaterThan(textBounds.x + textBounds.width);
  expect(
    Math.abs(
      textBounds.y + textBounds.height / 2 - (buttonBounds.y + buttonBounds.height / 2),
    ),
  ).toBeLessThan(2);
  await page.getByRole('button', { name: 'Restore list...', exact: true }).click();
  const confirmation = page.getByRole('dialog', { name: 'Restore this list?' });
  await expect(confirmation).toContainText('search results');
  await expect(confirmation).toContainText(
    'Existing members and direct shares keep their access',
  );
  await confirmation.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(banner).toBeVisible();
  await page.getByRole('button', { name: 'Restore list...', exact: true }).click();
  await confirmation.getByRole('button', { name: 'Restore list', exact: true }).click();
  await expect(page.locator('[data-task-page]')).toHaveAttribute(
    'data-can-write',
    'true',
  );
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-can-write', 'true');
  await expect(page.locator('.list-archive-banner')).toHaveCount(0);
  await page.goto(`/space/${space.id}`);
  await expect(page.locator('.space-list-grid').first()).toContainText(
    'Archived example',
  );
});
