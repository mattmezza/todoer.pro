import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('a tag captured with a task appears in the Inbox and its task panel', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  await page.getByText('Capture a task…').click();
  const dialog = page.getByRole('dialog', { name: 'What’s on your mind?' });
  const title = `Tagged ${randomUUID()}`;
  await dialog.getByRole('combobox', { name: /task/i }).fill(`${title} +work`);
  await expect(dialog.locator('.capture-tag')).toHaveText('+work');
  await dialog.getByRole('button', { name: 'Add task', exact: true }).click();
  const row = page.locator('.inbox-task').filter({ hasText: title });
  await expect(row.locator('.inbox-tag')).toHaveText('+work');
  await row.locator('.inbox-location').click();
  const panel = page.getByLabel('Task details', { exact: true });
  await expect(panel.locator('.panel-task-tags')).toContainText('+work');
  await page.reload();
  await expect(
    page.locator('.inbox-task').filter({ hasText: title }).locator('.inbox-tag'),
  ).toHaveText('+work');
});
