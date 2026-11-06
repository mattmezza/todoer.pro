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
  const listPath = new URL(page.url()).pathname;
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
