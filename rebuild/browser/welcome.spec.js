import { test, expect } from '@playwright/test';
test.describe.configure({ mode: 'serial' });
async function enter(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/welcome');
  // A previous scenario may have left a resumable draft.
  const back = page.locator('[data-welcome-back]');
  await expect(page.locator('[data-welcome-loading]')).toBeHidden();
  if (await back.isVisible()) await back.click();
  const form = page.locator('[data-welcome-setup]');
  await expect(form).toBeVisible();
  for (const input of await form.locator('input').all()) await input.uncheck();
  return form;
}
async function inventory(page) {
  return (await (await page.request.get('/api/v1/lists')).json()).map((l) => l.id).sort();
}
test('two-question welcome defers creation, resumes, and safely retries completion', async ({
  page,
}) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const form = await enter(page);
  const original = await inventory(page);
  await expect(
    form.getByRole('button', { name: 'Continue', exact: true }),
  ).toBeDisabled();
  await form.getByRole('checkbox', { name: 'Personal', exact: false }).check();
  await form.getByRole('checkbox', { name: 'Work', exact: false }).check();
  await page.screenshot({
    path: '.local/rebuild/welcome-desktop.png',
    fullPage: true,
    animations: 'disabled',
  });
  await form.getByRole('button', { name: 'Continue', exact: true }).click();
  const lists = page.locator('[data-welcome-lists]');
  await expect(lists).toBeVisible();
  expect(await inventory(page)).toEqual(original);
  await page.reload();
  await expect(lists).toBeVisible();
  const personal = lists.getByRole('group', { name: '@personal', exact: true });
  const work = lists.getByRole('group', { name: '@work', exact: true });
  await personal.getByRole('checkbox', { name: 'Groceries', exact: false }).check();
  await personal.getByRole('checkbox', { name: 'Home chores', exact: false }).check();
  await work.getByRole('checkbox', { name: 'Weekly priorities', exact: false }).check();
  await page.screenshot({
    path: '.local/rebuild/welcome-lists-desktop.png',
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
  await lists.getByRole('button', { name: 'Start organizing' }).click();
  await expect(page.locator('[data-welcome-error]')).toBeVisible();
  await expect(
    personal.getByRole('checkbox', { name: 'Groceries', exact: false }),
  ).toBeChecked();
  await expect(page.locator('canvas.welcome-chase')).toHaveAttribute(
    'data-complete',
    'false',
  );
  await lists.getByRole('button', { name: 'Start organizing' }).click();
  await expect(page).toHaveURL(/\/lists\//);
  const after = await inventory(page);
  const workspaces = await (await page.request.get('/api/v1/workspaces')).json();
  for (const title of ['personal', 'work']) {
    const space = workspaces.find((item) => item.title.toLowerCase() === title);
    expect(space?.description.length).toBeGreaterThan(0);
  }
  expect((await (await page.request.get('/api/v1/onboarding')).json()).status).toBe(
    'complete',
  );
  // Reopening the same choices reuses destinations instead of duplicating them.
  await enter(page);
  await form.getByRole('checkbox', { name: 'Personal', exact: false }).check();
  await form.getByRole('checkbox', { name: 'Work', exact: false }).check();
  await form.getByRole('button', { name: 'Continue', exact: true }).click();
  await lists
    .getByRole('group', { name: '@personal', exact: true })
    .getByRole('checkbox', { name: 'Groceries', exact: false })
    .check();
  await lists
    .getByRole('group', { name: '@personal', exact: true })
    .getByRole('checkbox', { name: 'Home chores', exact: false })
    .check();
  await lists
    .getByRole('group', { name: '@work', exact: true })
    .getByRole('checkbox', { name: 'Weekly priorities', exact: false })
    .check();
  await lists.getByRole('button', { name: 'Start organizing' }).click();
  await expect(page).toHaveURL(/\/lists\//);
  expect(await inventory(page)).toEqual(after);
  expect(errors).toEqual([]);
});
test('desktop runner rests, never completes early, and celebrates on completion', async ({
  page,
}) => {
  await enter(page);
  const canvas = page.locator('canvas.welcome-chase');
  await expect(canvas).toBeVisible();
  expect(
    await canvas.evaluate((el) => el.height / el.getBoundingClientRect().height),
  ).toBeGreaterThanOrEqual(2);
  await expect(canvas).toHaveAttribute('data-complete', 'false');
  await expect(canvas).toHaveAttribute('data-phase', 'rest', { timeout: 16000 });
  await page.screenshot({
    path: '.local/rebuild/welcome-chase-rest.png',
    fullPage: true,
  });
  await expect(canvas).toHaveAttribute('data-complete', 'false');
  await expect(canvas).toHaveAttribute('data-phase', 'chase', { timeout: 7000 });
  await page
    .locator('[data-welcome-setup]')
    .getByRole('checkbox', { name: 'Personal', exact: false })
    .check();
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  // Hold navigation to observe the actual completion ceremony.
  await page.route('**/space/*', (route) => route.abort());
  await page.getByRole('button', { name: 'I’ll decide on lists later' }).click();
  await expect(page.locator('[data-welcome]')).toHaveAttribute('data-ready', 'true');
  await expect(canvas).toHaveAttribute('data-phase', 'celebrate', { timeout: 4000 });
  await expect(canvas).toHaveAttribute('data-complete', 'true');
  await expect(page.locator('.welcome-content')).toHaveCSS('opacity', '0');
  await expect(page.locator('[data-welcome-complete]')).toBeVisible();
  await page.screenshot({
    path: '.local/rebuild/welcome-chase-celebrate.png',
    fullPage: true,
  });
});
test('reduced motion skips the runner and completion delay', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await enter(page);
  await expect(page.locator('canvas.welcome-chase')).toHaveCount(0);
  await page
    .locator('[data-welcome-setup]')
    .getByRole('checkbox', { name: 'Work', exact: false })
    .check();
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await page.getByRole('button', { name: 'I’ll decide on lists later' }).click();
  await expect(page).toHaveURL(/\/space\//);
});
test('mobile stays simple, defers installation, and has no runner', async ({
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
  const form = await enter(page);
  await expect(page.locator('.pwa-install-notice')).toHaveCount(0);
  await expect(page.locator('canvas.welcome-chase')).toHaveCount(0);
  await form.getByRole('checkbox', { name: 'Personal', exact: false }).check();
  await page.screenshot({
    path: '.local/rebuild/welcome-mobile.png',
    fullPage: true,
    animations: 'disabled',
  });
  await form.getByRole('button', { name: 'Continue', exact: true }).click();
  await expect(page.locator('[data-welcome-lists]')).toBeVisible();
  await expect(page.getByRole('group', { name: '@work', exact: true })).toHaveCount(0);
  await page.screenshot({
    path: '.local/rebuild/welcome-lists-mobile.png',
    fullPage: true,
    animations: 'disabled',
  });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  const original = await inventory(page);
  await page.getByRole('button', { name: 'Skip for now' }).click();
  await expect(page).not.toHaveURL(/\/welcome/);
  expect(await inventory(page)).toEqual(original);
  await context.close();
});
