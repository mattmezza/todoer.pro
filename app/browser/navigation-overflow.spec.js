import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('long navigation labels scroll gently, pause on hover and respect reduced motion', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const post = async (path, data) => {
    const response = await page.request.post(path, {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  };
  const space = await post('/api/v1/workspaces', { title: `Labels ${randomUUID()}` });
  const first = await post('/api/v1/lists', {
    workspace_id: space.id,
    title: 'An exceptionally long list title for scrolling one',
  });
  const second = await post('/api/v1/lists', {
    workspace_id: space.id,
    title: 'Another exceptionally long list title for scrolling two',
  });
  await page.goto(`/lists/${first.id}`);
  const firstLink = page.locator(`.list-navigation[href="/lists/${first.id}"]`).first();
  const secondLink = page.locator(`.list-navigation[href="/lists/${second.id}"]`).first();
  const firstLabel = firstLink.locator('.nav-overflow-label');
  const secondLabel = secondLink.locator('.nav-overflow-label');
  await expect(firstLabel).toHaveAttribute('data-overflowing', '');
  await expect(secondLabel).toHaveAttribute('data-overflowing', '');
  const firstDelay = await firstLabel.evaluate((node) =>
    node.style.getPropertyValue('--nav-scroll-delay'),
  );
  const secondDelay = await secondLabel.evaluate((node) =>
    node.style.getPropertyValue('--nav-scroll-delay'),
  );
  expect(firstDelay).not.toBe(secondDelay);
  await firstLink.hover();
  await expect
    .poll(() =>
      firstLabel
        .locator('.nav-overflow-inner')
        .evaluate((node) => getComputedStyle(node).animationPlayState),
    )
    .toBe('paused');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect
    .poll(() =>
      firstLabel
        .locator('.nav-overflow-inner')
        .evaluate((node) => getComputedStyle(node).animationName),
    )
    .toBe('none');
});
