import { test, expect } from '@playwright/test';
test('smart capture saves the reviewed priority, date and tag', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/capture?list=groceries');
  const title = `Smart capture ${Date.now()}`;
  await page.getByLabel('Task', { exact: true }).fill(`${title} today p0 +shopping`);
  await expect(page.locator('.capture-result').getByText(/P0/)).toBeVisible();
  await page.getByRole('button', { name: 'Add task', exact: true }).click();
  await expect(page).toHaveURL(/\/lists\//);
  const task = page.getByRole('link', { name: title, exact: true });
  await expect(task).toBeVisible();
  const id = (await task.getAttribute('href')).split('/').pop();
  const schedule = await (await page.request.get(`/api/v1/tasks/${id}/schedule`)).json();
  const tags = await (await page.request.get(`/api/v1/tasks/${id}/tags`)).json();
  expect(schedule.schedule.priority).toBe(0);
  expect(schedule.schedule.due.kind).toBe('date');
  expect(tags.map((t) => t.name)).toContain('shopping');
});

test('quick capture parses without losing the task when AI is disabled', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  await page.keyboard.press('n');
  const dialog = page.getByRole('dialog', { name: 'What’s on your mind?' });
  await expect(dialog).toBeVisible();
  const title = `Quick smart ${Date.now()}`;
  await dialog.getByLabel('Task', { exact: true }).fill(`${title} tomorrow p1`);
  await expect(dialog.locator('.capture-result').getByText(/P1/)).toBeVisible();
  await dialog.getByRole('button', { name: 'Add task', exact: true }).click();
  await expect(dialog).not.toBeVisible();
});

test('Inbox capture opens the shared omnibar and refreshes after saving', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const launcher = page.getByRole('link', { name: 'Capture a task…' });
  await launcher.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: 'What’s on your mind?' });
  const title = `Inbox omnibar ${Date.now()}`;
  await dialog.getByLabel('Task', { exact: true }).fill(title);
  await expect(dialog.locator('.capture-result')).toContainText('Inbox');
  await dialog.getByRole('button', { name: 'Add task', exact: true }).click();
  await expect(page.getByRole('link', { name: title, exact: true })).toBeVisible();
});

test('breakdown remains available with a disabled provider and manual capture is preserved', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/capture');
  const title = `Breakdown ${Date.now()}`;
  await page.getByLabel('Task', { exact: true }).fill(title);
  await page.getByRole('button', { name: 'Add task', exact: true }).click();
  await page.getByRole('link', { name: title, exact: true }).click();
  await page.getByLabel('Task actions').click();
  await page.getByRole('button', { name: 'Break down…' }).click();
  await expect(
    page.getByText('AI assistance is unavailable. You can still add sub-tasks manually.'),
  ).toBeVisible();
  await expect(page.getByText('Add sub-task', { exact: true }).first()).toBeVisible();
});

test('offline queue retains interpreted metadata for retry', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/capture');
  const title = `Offline smart ${Date.now()}`;
  await page.getByLabel('Task', { exact: true }).fill(`${title} tomorrow p0 +offline`);
  await expect(page.locator('.capture-result').getByText(/P0/)).toBeVisible();
  await page.route('**/api/v1/capture', (route) => route.abort());
  await page.getByRole('button', { name: 'Add task', exact: true }).click();
  await expect(
    page.getByText('Saved on this device. Open Inbox to retry syncing.'),
  ).toBeVisible();
  const record = await page.evaluate(async (name) => {
    const db = await new Promise((resolve, reject) => {
      const r = indexedDB.open('todoer-captures-v1');
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
    return await new Promise((resolve, reject) => {
      const r = db.transaction('captures').objectStore('captures').getAll();
      r.onsuccess = () =>
        resolve(r.result.find((item) => item.payload.tasks[0].title === name));
      r.onerror = () => reject(r.error);
    });
  }, title);
  expect(record.payload.metadata.priority).toBe(0);
  expect(record.payload.metadata.tags).toEqual(['offline']);
  expect(record.payload.metadata.due_date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  await page.unroute('**/api/v1/capture');
  await page.goto(`/lists/${encodeURIComponent(record.payload.list_id)}`);
  await expect(page.getByRole('link', { name: title, exact: true })).toBeVisible();
});

test('breakdown review sends only selected edited steps in the chosen order', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/capture');
  const name = `Review breakdown ${Date.now()}`;
  await page.getByLabel('Task', { exact: true }).fill(name);
  await page.getByRole('button', { name: 'Add task', exact: true }).click();
  await page.getByRole('link', { name, exact: true }).click();
  let reviewed;
  await page.route('**/api/v1/assist/drafts', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 300));
    return route.fulfill({
      json: {
        status: 'draft',
        preview: {
          id: 'draft',
          revision: 1,
          expected_hash: 'first',
          payload: {
            target: { kind: 'existing_task', task_id: 'test' },
            tasks: [
              { title: 'First', description: 'A', parent_index: null },
              { title: 'Second', description: 'B', parent_index: null },
              { title: 'Third', description: 'C', parent_index: null },
            ],
          },
        },
      },
    });
  });
  await page.route('**/api/v1/assist/drafts/draft/review', (route) => {
    reviewed = route.request().postDataJSON();
    return route.fulfill({
      json: {
        id: 'draft',
        revision: 2,
        expected_hash: 'reviewed',
        payload: reviewed.payload,
      },
    });
  });
  await page.route('**/api/v1/assist/apply', (route) =>
    route.fulfill({ json: { tasks: [], list_id: 'inbox' } }),
  );
  await page.getByLabel('Task actions').click();
  await page.getByRole('button', { name: 'Break down…' }).click();
  const modal = page.getByRole('dialog', { name: 'Break down task' });
  await expect(modal).toBeVisible();
  await expect(modal.getByText('Generating suggestions…')).toBeVisible();
  const first = modal.getByRole('group', { name: 'Suggestion 1' });
  await first.getByLabel('Title').fill('Edited first');
  const third = page.getByRole('group', { name: 'Suggestion 3' });
  await third.getByLabel('Include suggestion').uncheck();
  await page
    .getByRole('group', { name: 'Suggestion 2' })
    .getByRole('button', { name: 'Move up' })
    .click();
  await page.getByRole('button', { name: 'Add selected sub-tasks' }).click();
  await expect
    .poll(() => reviewed?.payload.tasks.map((t) => t.title))
    .toEqual(['Second', 'Edited first']);
});

test('capture settings persist and can disable parsing', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/settings');
  const checkbox = page.getByLabel('Recognize shortcuts');
  await expect(checkbox).toBeChecked();
  await checkbox.uncheck();
  await page.getByLabel('Global AI instructions (optional)').fill('Use concise steps');
  await page.getByRole('button', { name: 'Save capture settings' }).click();
  await expect(page.getByText('Settings saved.')).toBeVisible();
  await page.goto('/capture');
  await page.getByLabel('Task', { exact: true }).fill('Literal tomorrow p0');
  await expect(page.locator('.smart-capture')).not.toContainText('P0');
  await page.goto('/settings');
  await expect(page.getByLabel('Recognize shortcuts')).not.toBeChecked();
  await page.getByLabel('Recognize shortcuts').check();
  await page.getByLabel('Global AI instructions (optional)').fill('');
  await page.getByRole('button', { name: 'Save capture settings' }).click();
  await expect(page.getByText('Settings saved.')).toBeVisible();
});

test('space and list autocomplete work with Tab and arrow keys', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  await page.keyboard.press('n');
  const dialog = page.getByRole('dialog', { name: 'What’s on your mind?' });
  const task = dialog.getByLabel('Task', { exact: true });
  await task.fill('Buy milk @per');
  await expect(dialog.getByRole('option', { name: '@personal' })).toBeVisible();
  await task.press('Tab');
  await expect(task).toHaveValue('Buy milk @personal');
  await task.fill('Buy milk @personal#in');
  await expect(dialog.getByRole('option', { name: '#Inbox' })).toBeVisible();
  await task.press('ArrowDown');
  await task.press('Tab');
  await expect(task).toHaveValue('Buy milk @personal#Inbox');
  await expect(dialog.locator('.capture-result')).toContainText('@personal / #Inbox');
  await expect(dialog.getByLabel('Destination', { exact: true })).toBeHidden();
  await dialog.getByText('Add a description', { exact: true }).click();
  const description = dialog.getByLabel('Description, Markdown supported');
  await expect(description).toBeVisible();
  await description.fill('One line\nTwo lines\nThree lines');
  expect(await description.evaluate((el) => el.scrollHeight <= el.clientHeight)).toBe(
    true,
  );
});

test('suggestions support Ctrl-N, Ctrl-P, arrows and Tab', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.route('**/api/v1/capture/options', (route) =>
    route.fulfill({
      json: {
        preferences: { smart_enabled: true, date_enabled: true, repeat_capture: false },
        destinations: [
          {
            space: 'Personal',
            space_id: 'a',
            list: 'Inbox',
            list_id: 'inbox',
            default_list: true,
          },
          {
            space: 'People',
            space_id: 'b',
            list: 'Todo',
            list_id: 'inbox',
            default_list: true,
          },
        ],
        tags: [],
      },
    }),
  );
  await page.goto('/today');
  await page.keyboard.press('n');
  const task = page
    .getByRole('dialog', { name: 'What’s on your mind?' })
    .getByLabel('Task', { exact: true });
  await task.fill('buy @pe');
  await expect(page.getByRole('option', { name: '@Personal' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await task.press('Control+n');
  await expect(page.getByRole('option', { name: '@People' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await task.press('Control+p');
  await expect(page.getByRole('option', { name: '@Personal' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await task.press('ArrowDown');
  await task.press('Tab');
  await expect(task).toHaveValue('buy @People');
});

test('priority is previewed without a selector and literal text can be restored', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  await page.keyboard.press('n');
  const dialog = page.getByRole('dialog', { name: 'What’s on your mind?' });
  await dialog.getByLabel('Task', { exact: true }).fill('buy milk p0');
  await expect(dialog.locator('.capture-result')).toContainText('P0');
  await expect(dialog.getByRole('combobox', { name: 'Priority' })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Use text as typed' }).click();
  await expect(dialog.locator('.capture-result strong')).toHaveText('buy milk p0');
  await dialog.getByRole('button', { name: 'Interpret shortcuts' }).click();
  await expect(dialog.locator('.capture-result strong')).toHaveText('buy milk');
});

test('quick capture reveals an auto-growing description and can keep adding', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  await page.keyboard.press('n');
  const dialog = page.getByRole('dialog', { name: 'What’s on your mind?' });
  await expect(dialog.getByRole('button', { name: 'Shortcuts' })).toBeVisible();
  const addDescription = dialog.getByText('Add a description', { exact: true });
  await expect(addDescription).toBeVisible();
  await addDescription.click();
  await expect(addDescription).toBeHidden();
  const description = dialog.getByLabel('Description, Markdown supported');
  await description.fill('Line one\nLine two\nLine three');
  expect(await description.evaluate((el) => el.scrollHeight <= el.clientHeight)).toBe(
    true,
  );
  const repeat = dialog.getByLabel('Keep adding');
  const checkboxBox = await repeat.boundingBox();
  const buttonBox = await dialog
    .getByRole('button', { name: 'Add task', exact: true })
    .boundingBox();
  expect(
    Math.abs(checkboxBox.y + checkboxBox.height / 2 - buttonBox.y - buttonBox.height / 2),
  ).toBeLessThan(2);
  await repeat.check();
  const title = dialog.getByLabel('Task', { exact: true });
  await title.fill(`Repeat capture ${Date.now()}`);
  await dialog.getByRole('button', { name: 'Add task', exact: true }).click();
  await expect(dialog).toBeVisible();
  await expect(title).toHaveValue('');
  await expect(title).toBeFocused();
  await expect(addDescription).toBeVisible();
});

test('shortcut help expires after 30 seconds but stays visible while hovered', async ({
  page,
}) => {
  await page.clock.install();
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  await page.keyboard.press('n');
  const dialog = page.getByRole('dialog', { name: 'What’s on your mind?' });
  const button = dialog.getByRole('button', { name: 'Shortcuts' });
  await button.click();
  const hints = dialog.getByText('@space · #list · +tag · p0–p3 · ! / !! / !!! · today');
  await expect(hints).toBeVisible();
  await expect(button).toBeHidden();
  await page.mouse.move(0, 0);
  await page.clock.fastForward(30001);
  await expect(button).toBeVisible();
  await expect(hints).toBeHidden();
  await button.click();
  await hints.hover();
  await page.clock.fastForward(30001);
  await expect(hints).toBeVisible();
  await page.mouse.move(0, 0);
  await expect(button).toBeVisible();
});
