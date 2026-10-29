import { test, expect } from '@playwright/test';
for (const viewport of [
  { width: 360, height: 800 },
  { width: 1440, height: 900 },
]) {
  test(`global native capture ${viewport.width}px preserves context and description`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/lists/groceries');
    if (viewport.width < 640)
      await page.getByRole('link', { name: 'Add a task' }).click();
    else await page.goto('/capture?list=groceries');
    await expect(page.getByLabel('Destination', { exact: true })).toHaveValue(
      'groceries',
    );
    const title = `Synthetic native capture ${viewport.width} ${Date.now()}`;
    await page.getByLabel('Task', { exact: true }).fill(title);
    await page.getByText('Add a description', { exact: true }).click();
    await page
      .getByLabel('Description, Markdown supported')
      .fill('Preserved **brief** with #Inbox reference.');
    await page.getByRole('button', { name: 'Add task', exact: true }).click();
    await expect(page).toHaveURL(/\/lists\/groceries$/);
    await expect(page.getByRole('button', { name: title, exact: true })).toBeVisible();
    const task = page
      .locator('.task')
      .filter({ has: page.getByRole('button', { name: title, exact: true }) });
    await expect(task.locator('strong')).toHaveText('brief');
    await expect(task.getByRole('link', { name: 'Inbox', exact: false })).toHaveAttribute(
      'href',
      '/lists/inbox',
    );
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
  });
}
