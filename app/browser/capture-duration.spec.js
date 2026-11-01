import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('capture saves zero and multi-day durations on tasks and subtasks', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  await page.keyboard.press('n');
  const quick = page.getByRole('dialog', { name: 'What’s on your mind?' });
  const title = `Immediate ${randomUUID()}`;
  await quick.getByLabel('Task', { exact: true }).fill(`${title} :0`);
  await expect(quick.locator('[data-duration]')).toHaveText('Duration 0m');
  await quick.getByRole('button', { name: 'Add task', exact: true }).click();
  const row = page.locator('[data-planning-task]').filter({ hasText: title });
  await expect(row).toBeVisible();
  const id = await row.getAttribute('data-planning-task');
  expect(
    (await (await page.request.get(`/api/v1/tasks/${id}/schedule`)).json())
      .duration_minutes,
  ).toBe(0);
  await page.goto(`/tasks/${id}`);
  await page.locator('.task-context-menu > summary').click();
  await page.locator('[data-open-advanced]').click();
  const settings = page.getByRole('dialog', { name: 'Task details' });
  await expect(settings.getByText('Duration · 0m')).toBeVisible();
  await settings.getByRole('button', { name: 'Back', exact: false }).click();
  await page.locator('[data-child-composer] > summary').click();
  const childForm = page.locator('[data-child-form]');
  const childTitle = `Long subtask ${randomUUID()}`;
  await childForm.locator('[name="title"]').fill(`${childTitle} :2days`);
  await expect(childForm.locator('[data-duration]')).toHaveText('Duration 2d');
  await childForm.getByRole('button', { name: 'Add sub-task' }).click();
  const child = page.locator('[data-children] li').filter({ hasText: childTitle });
  await expect(child).toBeVisible();
  const childId = await child.getAttribute('data-child-id');
  expect(
    (await (await page.request.get(`/api/v1/tasks/${childId}/schedule`)).json())
      .duration_minutes,
  ).toBe(2880);
});
