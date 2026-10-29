import { test, expect } from '@playwright/test';

test('AI setup is nested under AI and every settings icon exists', async ({
  page,
  request,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings/ai/setup');
  const settings = page.getByRole('navigation', { name: 'Settings', exact: true });
  const ai = settings.getByRole('group', { name: 'AI', exact: true });
  await expect(ai.getByRole('link', { name: 'Setup', exact: true })).toHaveAttribute(
    'href',
    '/settings/ai/setup',
  );
  await expect(ai.getByRole('link', { name: 'Usage', exact: true })).toHaveAttribute(
    'href',
    '/settings/ai/usage',
  );
  const sprite = await (await request.get('/assets/icons.svg')).text();
  const symbols = [...sprite.matchAll(/<symbol id="([^"]+)"/g)].map((match) => match[1]);
  const references = await settings
    .locator('svg use')
    .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('href').split('#')[1]));
  for (const reference of references) expect(symbols).toContain(reference);
  expect(new Set(references).size).toBe(references.length);
  await page.goto('/settings/billing');
  await expect(
    page
      .getByRole('navigation', { name: 'Settings', exact: true })
      .getByRole('group', { name: 'AI', exact: true }),
  ).toHaveCount(0);
});
