import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

for (const width of [390, 1280]) {
  test(`list rows share task panel and right-click brief editing at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/lists/inbox');
    const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
    const created = await page.request.post('/api/v1/capture', {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data: {
        list_id: 'inbox',
        tasks: [{ title: `Row ${randomUUID()}`, description: 'Original brief' }],
      },
    });
    expect(created.ok()).toBe(true);
    const [task] = await created.json();
    await page.reload();
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    const listUrl = page.url();
    const row = page.locator(`.task[data-id="${task.id}"]`);
    await row.locator('.task-title').click({ button: 'right' });
    await row.getByLabel('Task title', { exact: true }).fill('Edited inline');
    await row
      .getByLabel('Task description', { exact: true })
      .fill('Updated inline description');
    await expect(row.getByRole('checkbox')).toHaveCount(0);
    await row.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(row.locator('.task-title')).toHaveText('Edited inline');
    await expect(row.locator('.task-description')).toHaveText(
      'Updated inline description',
    );
    await row.locator('.task-content').click({ position: { x: 5, y: 5 } });
    const panel = page.getByRole('complementary', { name: 'Task details', exact: true });
    await expect(panel.locator('[data-task-page]')).toBeVisible();
    await expect(panel.locator('[data-edit-title]')).toHaveText('Edited inline');
    await expect(page).toHaveURL(listUrl);
    expect(await panel.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
    await page.getByRole('button', { name: 'Close task details', exact: true }).click();
    await expect(panel).toHaveCount(0);
    await row.getByLabel('Actions for Edited inline', { exact: true }).click();
    await expect(row.locator('[data-action=edit] .ui-icon')).toHaveCount(1);
    await row.getByRole('button', { name: 'Open details', exact: true }).click();
    await expect(panel.locator('[data-task-page]')).toBeVisible();
  });
}
