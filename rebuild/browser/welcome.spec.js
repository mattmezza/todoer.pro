import { test, expect } from '@playwright/test';
test.describe.configure({ mode: 'serial' });
async function enter(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/welcome');
  await expect(page.locator('[data-welcome-setup]')).toBeVisible();
}
test('space then list: slug names, colors, resume and safe completion retry', async ({
  page,
}) => {
  await enter(page);
  const original = await (await page.request.get('/api/v1/lists')).json();
  await page.getByRole('button', { name: 'Together', exact: true }).click();
  await expect(page.locator('[data-preview-space]')).toHaveText('home');
  await expect(page.locator('[data-preview-list-card]')).toBeHidden();
  await page.getByLabel('Space name', { exact: false }).fill('Our Home');
  await page.screenshot({
    path: '.local/rebuild/welcome-desktop.png',
    fullPage: true,
    animations: 'disabled',
  });
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  const form = page.locator('[data-welcome-list]');
  await expect(form).toBeVisible();
  await expect(page.locator('[data-preview-space]')).toHaveText('our-home');
  await expect(form.locator('[name=list]')).toHaveValue('chores');
  await expect(form.locator('[name=color]:checked')).toHaveValue('green');
  await page.reload();
  await expect(form).toBeVisible();
  await form.getByLabel('List name', { exact: false }).fill('Home Chores');
  await expect(form.locator('[name=list]')).toHaveValue('home-chores');
  await form.getByLabel('Pink', { exact: true }).check();
  await expect(page.locator('[data-preview-color]')).toHaveClass(/swatch-pink/);
  await page.screenshot({
    path: '.local/rebuild/welcome-list-desktop.png',
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
  await form.getByRole('button', { name: 'Create my list' }).click();
  await expect(page.locator('[data-welcome-error]')).toBeVisible();
  await form.getByRole('button', { name: 'Create my list' }).click();
  await expect(page).toHaveURL(/\/lists\//);
  const current = await (await page.request.get('/api/v1/lists')).json();
  expect(current.map((l) => l.id).sort()).toEqual(original.map((l) => l.id).sort());
  expect(current.some((l) => l.title === 'home-chores')).toBeTruthy();
  expect((await (await page.request.get('/api/v1/onboarding')).json()).status).toBe(
    'complete',
  );
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
  await expect(page.locator('[data-preview-space]')).toHaveText('work');
  await page.screenshot({ path: '.local/rebuild/welcome-mobile.png', fullPage: true });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await expect(page.locator('[data-welcome-list]')).toBeVisible();
  await page.getByRole('button', { name: 'Skip for now' }).click();
  await expect(page).not.toHaveURL(/\/welcome/);
  expect((await (await page.request.get('/api/v1/onboarding')).json()).status).toBe(
    'skipped',
  );
  await context.close();
});
