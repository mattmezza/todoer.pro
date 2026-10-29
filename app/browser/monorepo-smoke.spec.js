import { test, expect } from '@playwright/test';
import crypto from 'node:crypto';
const headers = { Authorization: 'Bearer local-integration-token-not-for-production' };

for (const width of [390, 1440]) {
  test(`current capture and task navigation at ${width}px`, async ({ page, request }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/capture?list=groceries');
    const title = `Monorepo smoke ${crypto.randomUUID()}`;
    await page.getByLabel('Task', { exact: true }).fill(title);
    await page.getByRole('button', { name: 'Add task', exact: true }).click();
    await expect(page).toHaveURL(/\/lists\/[^/?]+$/);
    const link = page.getByRole('link', { name: title, exact: true });
    await expect(link).toBeVisible();
    await page.goto(await link.getAttribute('href'));
    await expect(page).toHaveURL(/\/tasks\//);
    await expect(page.locator('main')).toContainText(title);
    const list = await request.get('/api/v1/lists/groceries/tasks', { headers });
    expect(list.ok()).toBe(true);
    expect((await list.json()).some((task) => task.title === title)).toBe(true);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
    expect(errors).toEqual([]);
  });
}

test('app assets and public icon survive the monorepo move', async ({ request }) => {
  for (const path of [
    '/assets/app.css',
    '/assets/htmx.min.js',
    '/assets/site.js',
    '/favicon.ico',
    '/legacy-assets/icon512_rounded.png',
  ]) {
    const response = await request.get(path);
    expect(response.ok(), path).toBe(true);
    expect((await response.body()).length, path).toBeGreaterThan(100);
  }
  const privateList = await request.get('/api/v1/lists/private/tasks', { headers });
  expect([403, 404]).toContain(privateList.status());
});
