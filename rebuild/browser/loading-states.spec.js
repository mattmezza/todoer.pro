import { test, expect } from '@playwright/test';
async function login(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
}
function gate() {
  let release;
  const wait = new Promise(resolve => { release = resolve; });
  return { wait, release };
}
test('settings conceal unconfirmed values and retain their geometry during slow loads', async ({ page }) => {
  await login(page);
  const delayed = gate();
  await page.route('**/api/v1/notification-preferences', async route => {
    await delayed.wait;
    await route.continue();
  });
  await page.goto('/settings');
  const form = page.locator('[data-calendar-preferences]');
  await expect(form).toHaveAttribute('data-loading-pending', 'form');
  await expect(form.locator('[name=timezone]')).toBeHidden();
  expect(await form.evaluate(el => el.inert)).toBe(true);
  const height = (await form.boundingBox()).height;
  delayed.release();
  await expect(form).not.toHaveAttribute('data-loading-pending');
  await expect(form.locator('[name=timezone]')).toBeVisible();
  expect(Math.abs((await form.boundingBox()).height - height)).toBeLessThan(12);
  await expect(form.locator('button[type=submit]')).toBeEnabled();
});
test('failed initial loads stop shimmering, offer retry, and never expose editable defaults', async ({ page }) => {
  await login(page);
  let fail = true;
  await page.route('**/api/v1/capture/preferences', route => fail
    ? route.fulfill({ status: 503, contentType: 'application/json', body: '{}' })
    : route.continue());
  await page.goto('/settings/ai');
  const form = page.locator('[data-capture-preferences]');
  await expect(form.locator('.loading-error')).toBeVisible();
  await expect(form).not.toHaveAttribute('data-loading-pending');
  await expect(form.locator('[name=smart_enabled]')).toBeHidden();
  fail = false;
  await form.locator('.loading-error button').click();
  await expect(form.locator('.loading-error')).toHaveCount(0);
  await expect(form.locator('[name=smart_enabled]')).toBeVisible();
  await expect(form.locator('button[type=submit]')).toBeEnabled();
});
test('inbox uses a skeleton initially and retains loaded content and actions during refresh', async ({ page }) => {
  await login(page);
  let delayed = gate();
  await page.route('**/api/v1/inbox', async route => { await delayed.wait; await route.continue(); });
  await page.goto('/inbox');
  const content = page.locator('[data-workbench-content]');
  await expect(content).toHaveAttribute('data-loading-pending', 'rows');
  delayed.release();
  await expect(content).toHaveAttribute('data-loaded', 'true');
  const html = await content.innerHTML();
  delayed = gate();
  await page.evaluate(() => document.dispatchEvent(new Event('todoer:workbench-refresh')));
  await expect(content).toHaveAttribute('aria-busy', 'true');
  await expect(content).not.toHaveAttribute('data-loading-pending');
  expect(await content.evaluate(el => el.inert)).toBe(false);
  expect(await content.innerHTML()).toBe(html);
  delayed.release();
  await expect(content).not.toHaveAttribute('aria-busy');
});
test('loading ownership, retry and reduced motion work across viewport sizes', async ({ page }) => {
  await page.goto('/signin');
  await page.evaluate(async () => {
    const { beginLoading } = await import('/assets/loading.js');
    const target = document.createElement('section');
    target.id = 'loading-test';
    target.innerHTML = '<button>Real action</button>';
    document.querySelector('main').append(target);
    const old = beginLoading(target);
    const current = beginLoading(target, { variant: 'form' });
    old.finish();
    window.finishLoadingTest = current.finish;
  });
  const target = page.locator('#loading-test');
  await expect(target).toHaveAttribute('data-loading-pending', 'form');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  expect(await target.evaluate(el => getComputedStyle(el, '::after').animationName)).toBe('none');
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.evaluate(() => window.finishLoadingTest());
  await expect(target.getByRole('button')).toBeVisible();
  expect(await target.evaluate(el => el.inert)).toBe(false);
});
