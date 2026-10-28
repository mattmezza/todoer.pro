import { test, expect } from '@playwright/test';
test.describe.configure({ mode: 'serial' });
async function enter(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/welcome');
  await expect(page.locator('[data-welcome-setup]')).toBeVisible();
}
test('welcome resumes, reuses defaults, and safely retries a committed first task', async ({
  page,
}) => {
  const taskTitle = `A first welcome task ${Date.now()}`;
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await enter(page);
  const original = await (await page.request.get('/api/v1/lists')).json();
  await page.screenshot({ path: '.local/rebuild/welcome-desktop.png', fullPage: true });
  const form = page.locator('[data-welcome-setup]');
  await form.getByLabel('Your space', { exact: false }).fill('My life');
  await form.getByLabel('Your first list', { exact: false }).fill('Little things');
  await form.getByLabel('Violet', { exact: true }).check();
  await expect(page.locator('[data-preview-space]')).toHaveText('My life');
  await expect(page.locator('[data-preview-list]')).toHaveText('Little things');
  await form.getByRole('button', { name: 'Make it mine' }).click();
  await expect(page.locator('[data-welcome-task]')).toBeVisible();
  await page.reload();
  const task = page.locator('[data-welcome-task]');
  await expect(task).toBeVisible();
  await expect(page.locator('[data-preview-list]')).toHaveText('Little things');
  await task.getByLabel('One thing on your mind', { exact: false }).fill(taskTitle);
  await task.getByRole('checkbox').check();
  await expect(page.locator('[data-preview-planned]')).toBeVisible();
  await page.screenshot({
    path: '.local/rebuild/welcome-task-desktop.png',
    fullPage: true,
    animations: 'disabled',
  });
  let lost = false;
  await page.route('**/api/v1/onboarding/finish', async (route) => {
    if (lost) return route.continue();
    lost = true;
    await route.fetch();
    await route.abort('failed');
  });
  await task.getByRole('button', { name: 'Add task & get started' }).click();
  await expect(page.locator('[data-welcome-error]')).toBeVisible();
  await expect(task.locator('[name=task]')).toHaveValue(taskTitle);
  await task.getByRole('button', { name: 'Add task & get started' }).click();
  await expect(page).toHaveURL(/\/today(?:\?.*)?$/);
  const current = await (await page.request.get('/api/v1/lists')).json();
  expect(current.map((list) => list.id).sort()).toEqual(
    original.map((list) => list.id).sort(),
  );
  const list = current.find((list) => list.title === 'Little things');
  expect(list).toBeTruthy();
  const tasks = await (await page.request.get(`/api/v1/lists/${list.id}/tasks`)).json();
  expect(tasks.filter((task) => task.title === taskTitle)).toHaveLength(1);
  expect((await (await page.request.get('/api/v1/onboarding')).json()).status).toBe(
    'complete',
  );
  expect(errors).toEqual([]);
});

test('mobile welcome stays compact, suppresses installation, and can be skipped', async ({
  browser,
  browserName,
}) => {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    isMobile: browserName !== 'firefox',
    hasTouch: true,
    userAgent:
      'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148 Safari/604.1',
    baseURL: process.env.TODOER_BROWSER_URL || 'http://127.0.0.1:8092',
  });
  const page = await context.newPage();
  await page.addInitScript(() => {
    if (location.pathname === '/welcome')
      localStorage.removeItem('todoer-install-notice-v1');
  });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/welcome');
  await expect(page.locator('[data-welcome-setup]')).toBeVisible();
  expect(
    await page.evaluate(() => localStorage.getItem('todoer-install-notice-v1')),
  ).toBeNull();
  await expect(page.locator('.pwa-install-notice')).toHaveCount(0);
  await page.getByRole('button', { name: 'Work', exact: true }).click();
  await expect(page.locator('[data-preview-space]')).toHaveText('Work');
  await page.screenshot({ path: '.local/rebuild/welcome-mobile.png', fullPage: true });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  await page.getByRole('button', { name: 'Make it mine' }).click();
  await expect(page.locator('[data-welcome-task]')).toBeVisible();
  await page.getByRole('button', { name: 'Skip for now' }).click();
  await expect(page).not.toHaveURL(/\/welcome/);
  expect((await (await page.request.get('/api/v1/onboarding')).json()).status).toBe(
    'skipped',
  );
  await context.close();
});
