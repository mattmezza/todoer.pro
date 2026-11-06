import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

const initial = new WeakMap();
test.beforeEach(async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const response = await page.request.get('/api/v1/notification-preferences');
  expect(response.ok()).toBeTruthy();
  initial.set(page, { preferences: await response.json(), sections: [] });
});
test.afterEach(async ({ page }) => {
  const state = initial.get(page);
  if (!state) return;
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const current = await (
    await page.request.get('/api/v1/notification-preferences')
  ).json();
  const response = await page.request.put('/api/v1/notification-preferences', {
    headers: { 'X-CSRF-Token': csrf },
    data: {
      ...current,
      show_favorites: state.preferences.show_favorites,
      swipe_left: state.preferences.swipe_left,
      swipe_right: state.preferences.swipe_right,
      collapsed_sections: current.collapsed_sections.filter(
        (id) => !state.sections.includes(id),
      ),
    },
  });
  expect(response.ok()).toBeTruthy();
});

test('appearance saves sidebar visibility and independent swipe choices', async ({
  page,
}) => {
  await page.goto('/settings/appearance');
  await page.getByRole('checkbox', { name: 'Show favorites in the sidebar' }).uncheck();
  await page.getByLabel('Swipe left', { exact: true }).selectOption('complete');
  await page.getByLabel('Swipe right', { exact: true }).selectOption('none');
  await page.getByRole('button', { name: 'Save appearance' }).click();
  await expect(page.getByLabel('Swipe left', { exact: true })).toHaveValue('complete');
  await expect(page.getByLabel('Swipe right', { exact: true })).toHaveValue('none');
  await page.goto('/spaces');
  await expect(page.locator('#navigation')).toHaveAttribute(
    'data-swipe-left',
    'complete',
  );
  await expect(page.locator('#navigation')).toHaveAttribute('data-swipe-right', 'none');
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const id = randomUUID();
  initial.get(page).sections.push(id);
  const response = await page.request.put(`/api/v1/section-preferences/${id}`, {
    headers: { 'X-CSRF-Token': csrf },
    data: { collapsed: true },
  });
  expect(response.ok()).toBeTruthy();
  const preferences = await (
    await page.request.get('/api/v1/notification-preferences')
  ).json();
  expect(preferences.collapsed_sections).toContain(id);
  expect(preferences.show_favorites).toBe(false);
  expect(preferences.swipe_right).toBe('none');
  await page.reload();
  expect(
    JSON.parse(await page.locator('#navigation').getAttribute('data-collapsed-sections')),
  ).toContain(id);
});

test('section collapse survives reload and a second browser tab', async ({
  page,
  context,
}) => {
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  async function post(path, data) {
    const response = await page.request.post(`/api/v1${path}`, {
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  }
  const space = await post('/workspaces', { title: `Collapse ${randomUUID()}` });
  const list = await post('/lists', {
    workspace_id: space.id,
    title: 'Remember sections',
  });
  const section = await post(`/lists/${list.id}/sections`, { title: 'Later' });
  initial.get(page).sections.push(section.id);
  await page.goto(`/lists/${list.id}`);
  const disclosure = page.locator(`[data-section-id="${section.id}"] .section-collapse`);
  await expect(disclosure).toHaveAttribute('open', '');
  const saved = page.waitForResponse(
    (response) =>
      response.url().endsWith(`/section-preferences/${section.id}`) &&
      response.request().method() === 'PUT',
  );
  await disclosure.locator('summary').click();
  expect((await saved).ok()).toBeTruthy();
  const serverHtml = await (await page.request.get(`/lists/${list.id}`)).text();
  expect(
    serverHtml.split(`data-section-id="${section.id}"`)[1].split('</details>')[0],
  ).not.toContain('class="section-collapse" open');
  await page.reload();
  await expect(disclosure).not.toHaveAttribute('open');
  const second = await context.newPage();
  await second.goto(`/lists/${list.id}`);
  await expect(
    second.locator(`[data-section-id="${section.id}"] .section-collapse`),
  ).not.toHaveAttribute('open');
  await second.close();
});
