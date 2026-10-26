import { test, expect } from '@playwright/test';
for (const width of [360, 1440]) {
  test(`assist capture remains available without provider ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/assist');
    await expect(
      page.getByText('Provider assistance is disabled. You can still capture tasks.'),
    ).toBeVisible();
    const text = `Synthetic assisted capture ${width} ${Date.now()}\nExact Unicode café 🧩`;
    await page.getByLabel('Original text', { exact: true }).fill(text);
    await page
      .getByRole('combobox', { name: 'Destination', exact: true })
      .selectOption('inbox');
    let providerRequests = 0;
    page.on('request', (request) => {
      if (/\/api\/v1\/assist\/(classify|drafts)$/.test(request.url())) providerRequests++;
    });
    await page.getByRole('button', { name: 'Save task', exact: true }).click();
    const saved = page.getByRole('link', {
      name: `Saved: ${text.split('\n')[0]}`,
      exact: true,
    });
    await expect(saved).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'Suggest a destination', exact: true }),
    ).toBeDisabled();
    await expect(
      page.getByRole('button', { name: 'Generate editable draft', exact: true }),
    ).toBeDisabled();
    expect(providerRequests).toBe(0);
    const id = (await saved.getAttribute('href')).split('/').pop();
    const response = await page.request.get(`/api/v1/tasks/${id}/subtree`);
    expect(response.ok()).toBe(true);
    expect((await response.json())[0].description).toBe(text);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
    await expect(page.getByLabel('Original text', { exact: true })).toHaveValue(text);
  });
}
