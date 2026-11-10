import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({ serviceWorkers: 'block' });

test('cold space requests stay bounded and app icons reuse immutable HTTP cache', async ({
  page,
  browser,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const response = await page.request.post('/api/v1/workspaces', {
    headers: { 'X-CSRF-Token': csrf },
    data: { title: `Asset budget ${randomUUID()}` },
  });
  expect(response.ok()).toBe(true);
  const space = await response.json();
  const context = await browser.newContext({ serviceWorkers: 'block' });
  await context.addCookies(await page.context().cookies());
  const cold = await context.newPage();
  try {
    const requests = [];
    const images = [];
    cold.on('request', (request) => requests.push(request));
    cold.on('response', (response) => {
      if (/\/assets\/images\//.test(response.url())) images.push(response);
    });
    await cold.goto(`/space/${space.id}`);
    await cold.waitForLoadState('networkidle');
    await cold.evaluate(() => document.fonts.ready);
    expect(
      requests.filter(
        (request) => new URL(request.url()).origin !== new URL(cold.url()).origin,
      ),
    ).toHaveLength(0);
    expect(requests.length).toBeLessThanOrEqual(40);
    expect(
      requests.filter((request) => request.resourceType() === 'font').length,
    ).toBeLessThanOrEqual(3);
    expect(
      requests.some((request) =>
        /\/assets\/(?:icons\.svg|icon192-v2\.png)$/.test(request.url()),
      ),
    ).toBe(false);
    expect(images.length).toBeGreaterThanOrEqual(2);
    for (const image of images)
      expect(image.headers()['cache-control']).toBe(
        'public, max-age=31536000, immutable',
      );
    console.log(
      `Cold space: ${requests.length} requests, ${requests.filter((request) => request.resourceType() === 'script').length} scripts, no third-party font requests`,
    );
    await cold.reload();
    await cold.waitForLoadState('networkidle');
    const icons = await cold.evaluate(() =>
      performance
        .getEntriesByType('resource')
        .filter((entry) => entry.name.includes('/assets/images/'))
        .map((entry) => ({ url: entry.name, transfer: entry.transferSize })),
    );
    expect(icons.length).toBeGreaterThanOrEqual(1);
    for (const icon of icons) expect(icon.transfer, icon.url).toBe(0);
  } finally {
    await context.close();
  }
});
