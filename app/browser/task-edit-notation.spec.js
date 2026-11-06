import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

for (const width of [390, 1280]) {
  test(`right-click edits title, description and notation together at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
    const captured = await page.request.post('/api/v1/capture', {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data: {
        list_id: 'inbox',
        tasks: [{ title: 'Before notation edit', description: 'Before description' }],
      },
    });
    expect(captured.ok()).toBe(true);
    const [task] = await captured.json();
    await page.goto(`/tasks/${task.id}`);
    await expect(page.locator('[data-connection]')).toHaveText('Up to date');
    await page.locator('[data-edit-title]').click({ button: 'right' });
    await page.getByLabel('Task title', { exact: true }).fill('buy milk .tomo !!!');
    await page
      .getByLabel('Description, Markdown supported', { exact: true })
      .fill('Updated **description**');
    await expect(page.getByText('Interpret task notation', { exact: false })).toHaveCount(
      0,
    );
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(
      page.getByRole('dialog', { name: 'Review task notation' }),
    ).toContainText('Priority: 0');
    await page.getByRole('button', { name: 'Save interpretation', exact: true }).click();
    await expect(page.locator('[data-edit-title]')).toHaveText('buy milk');
    await expect(page.locator('[data-saved-description] strong')).toHaveText(
      'description',
    );
    const result = await page.request.get(`/api/v1/tasks/${task.id}/schedule`);
    expect(result.ok()).toBe(true);
    const saved = await result.json();
    expect(saved.schedule.priority).toBe(0);
    expect(saved.schedule.planned.date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    await page.reload();
    await expect(page.locator('[data-edit-title]')).toHaveText('buy milk');
    await expect(page.locator('[data-saved-description] strong')).toHaveText(
      'description',
    );
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
  });
}
