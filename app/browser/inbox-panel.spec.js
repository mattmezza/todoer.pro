import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('Inbox row opens an editable, compact task panel', async ({ page }) => {
  await page.route('**/api/v1/assist/status', (route) =>
    route.fulfill({ json: { enabled: true } }),
  );
  await page.route('**/api/v1/assist/drafts', (route) =>
    route.fulfill({
      status: 503,
      json: { error: { message: 'Unavailable in this test' } },
    }),
  );
  await page.route('**/fragments/tasks/*/discussion', async (route) => {
    const response = await route.fetch();
    let html = await response.text();
    if (!html.includes('data-file-form'))
      html = html.replace(
        '</section><h3',
        '<form data-file-form class="attachment-form"><input type="file" data-file-input hidden><button type="button" data-file-choose>Attach file</button><p data-file-selection hidden></p><progress data-upload-progress hidden></progress><button data-file-upload hidden></button><button data-file-abandon hidden></button></form></section><h3',
      );
    await route.fulfill({ response, body: html });
  });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const title = `Panel ${randomUUID()}`;
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: inbox.list_id,
      tasks: [{ title, description: 'First description' }],
    },
  });
  expect(response.ok()).toBeTruthy();
  await page.reload();
  const row = page.locator('.inbox-task').filter({ hasText: title });
  await row.locator('.inbox-location').click();
  const panel = page.getByLabel('Task details', { exact: true });
  await expect(panel.locator('[data-task-page]')).toBeVisible();
  await expect
    .poll(async () => {
      const bell = await page
        .getByRole('button', { name: 'Notifications' })
        .boundingBox();
      const exportButton = await page
        .getByRole('button', { name: 'Export Inbox' })
        .boundingBox();
      return Math.abs(
        bell.y + bell.height / 2 - exportButton.y - exportButton.height / 2,
      );
    })
    .toBeLessThan(2);
  expect(
    (await page.getByRole('button', { name: 'Notifications' }).boundingBox()).x,
  ).toBeGreaterThan(
    (await page.getByRole('button', { name: 'Export Inbox' }).boundingBox()).x,
  );
  const checkbox = await panel
    .getByRole('button', { name: 'Complete task' })
    .boundingBox();
  expect(checkbox.x - (await panel.boundingBox()).x).toBeGreaterThanOrEqual(12);
  await expect(panel.getByText('Replan', { exact: true })).toHaveCount(0);
  const task = panel.locator('[data-task-page]');
  await panel.getByRole('button', { name: title, exact: true }).click();
  await panel.getByLabel('Task title', { exact: true }).fill('Edited panel task');
  await panel.getByLabel('Description, Markdown supported').fill('Edited description');
  await panel.getByLabel('Task title', { exact: true }).press('Enter');
  await expect(panel.getByRole('button', { name: 'Edited panel task' })).toBeVisible();
  await expect(panel.locator('[data-saved-description]')).toContainText(
    'Edited description',
  );
  const child = panel.locator('[data-child-composer] > summary');
  const comment = panel.locator('[data-comment-composer] > summary');
  await expect(comment).toBeVisible();
  const titleBox = await panel
    .getByRole('button', { name: 'Edited panel task' })
    .boundingBox();
  const checkboxBox = await panel
    .getByRole('button', { name: 'Complete task' })
    .boundingBox();
  const descriptionBox = await panel.locator('[data-saved-description]').boundingBox();
  expect(titleBox.x - (checkboxBox.x + checkboxBox.width)).toBeGreaterThanOrEqual(6);
  expect(
    (await child.boundingBox()).y - (descriptionBox.y + descriptionBox.height),
  ).toBeGreaterThanOrEqual(20);
  expect(
    Math.abs((await child.boundingBox()).height - (await comment.boundingBox()).height),
  ).toBeLessThan(2);
  await child.click();
  await expect(panel.getByLabel('New sub-task')).toBeVisible();
  await expect(panel.getByLabel('New sub-task')).toHaveCSS('font-size', '16px');
  await expect(panel.getByLabel('Sub-task description')).toHaveCSS('font-size', '13px');
  const childTitle = await panel.getByLabel('New sub-task').boundingBox();
  const childDescription = await panel.getByLabel('Sub-task description').boundingBox();
  expect(childDescription.y - (childTitle.y + childTitle.height)).toBeLessThan(10);
  await panel.locator('[data-child-cancel]').click();
  await comment.click();
  await expect(panel.getByLabel('Add a comment')).toBeVisible();
  await expect(panel.getByLabel('Add a comment')).toHaveCSS(
    'background-color',
    'rgba(0, 0, 0, 0)',
  );
  await expect(panel.getByRole('button', { name: 'AI breakdown…' })).toBeVisible();
  await panel.getByRole('button', { name: 'AI breakdown…' }).click();
  await expect(panel.getByRole('dialog', { name: 'Break down task' })).toBeVisible();
  await panel.getByRole('button', { name: 'Close breakdown' }).click();
  const chooser = page.waitForEvent('filechooser');
  await panel.getByRole('button', { name: 'Attach file' }).click();
  await chooser;
  await panel.getByRole('button', { name: 'Complete task' }).click();
  await expect(task).toHaveClass(/is-complete/);
  await expect(panel.getByRole('button', { name: 'Edited panel task' })).toHaveCSS(
    'text-decoration-line',
    'line-through',
  );
  await expect(panel.locator('[data-copy-task-reference]')).toBeVisible();
  await panel.getByLabel('Task actions').click();
  await expect(panel.getByRole('button', { name: 'Move to trash…' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(panel).toBeVisible();
  await panel.getByLabel('Change priority').click();
  await page.keyboard.press('Escape');
  await expect(panel).toBeVisible();
  await panel.getByLabel('Change priority').click();
  await expect(panel.locator('.panel-priority-options [data-priority="1"]')).toHaveCSS(
    'color',
    'rgb(231, 169, 63)',
  );
  await panel.locator('.panel-priority-options [data-priority="1"]').click();
  await expect(panel.getByLabel('Change priority')).toHaveText('P1');
  await panel.getByRole('link', { name: 'Open task page' }).click();
  await expect(page).toHaveURL(/\/tasks\//);
  await expect(page.locator('main[data-task-page]')).toBeVisible();
});
