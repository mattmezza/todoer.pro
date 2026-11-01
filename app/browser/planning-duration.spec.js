import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('task duration is shared between the planning dialog and task settings', async ({
  page,
}) => {
  await page.clock.install({ time: new Date('2026-11-01T12:00:00Z') });
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const title = `Duration ${randomUUID()} ${'longtasktitle'.repeat(8)}`;
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: { list_id: inbox.list_id, tasks: [{ title }] },
  });
  expect(response.ok()).toBeTruthy();
  const [task] = await response.json();
  await page.reload();
  const row = page.locator('.inbox-task').filter({ hasText: title });
  await row.getByText('Plan…').click();
  await row.getByText('Choose date…').click();
  const dialog = page.getByRole('dialog', { name: 'Plan this task' });
  await expect(
    dialog.getByRole('heading', { name: `When will you get “${title}” done?` }),
  ).toBeVisible();
  await expect(dialog.locator('h2 use')).toHaveAttribute(
    'href',
    '/assets/icons.svg#clock',
  );
  const desktopFields = await dialog
    .locator('.workbench-date-fields input')
    .evaluateAll((inputs) =>
      inputs.map((input) => input.getBoundingClientRect().toJSON()),
    );
  expect(desktopFields).toHaveLength(3);
  expect(
    Math.max(...desktopFields.map((rect) => rect.top)) -
      Math.min(...desktopFields.map((rect) => rect.top)),
  ).toBeLessThan(2);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(dialog.getByRole('group', { name: 'Time shortcuts' })).toBeVisible();
  expect(
    await dialog.evaluate((element) => element.scrollWidth <= element.clientWidth),
  ).toBe(true);
  await page.clock.setFixedTime(new Date('2026-11-01T23:50:00Z'));
  const expected = await page.evaluate(() => {
    const zone = document.querySelector('[data-workbench]').dataset.timezone;
    const parts = Object.fromEntries(
      new Intl.DateTimeFormat('en-CA', {
        timeZone: zone,
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        hourCycle: 'h23',
      })
        .formatToParts(new Date(Date.now() + 30 * 60000))
        .map(({ type, value }) => [type, value]),
    );
    return {
      date: `${parts.year}-${parts.month}-${parts.day}`,
      time: `${parts.hour}:${parts.minute}`,
    };
  });
  await dialog.getByRole('button', { name: '+30m' }).click();
  const date = await dialog.getByLabel('Day').inputValue();
  expect(date).toBe(expected.date);
  await expect(dialog.getByLabel('Time (optional)')).toHaveValue(expected.time);
  const currentDay = await page.evaluate(() => {
    const zone = document.querySelector('[data-workbench]').dataset.timezone;
    return new Intl.DateTimeFormat('en-CA', {
      timeZone: zone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
    }).format(new Date());
  });
  const nextWeek = new Date(`${currentDay}T12:00:00Z`);
  nextWeek.setUTCDate(nextWeek.getUTCDate() + 7);
  await dialog
    .getByRole('group', { name: 'Date shortcuts' })
    .getByRole('button', { name: '+1w' })
    .click();
  await expect(dialog.getByLabel('Day')).toHaveValue(nextWeek.toISOString().slice(0, 10));
  await expect(dialog.getByLabel('Time (optional)')).toHaveValue(expected.time);
  const durationShortcuts = dialog.getByRole('group', { name: 'Duration shortcuts' });
  await durationShortcuts.getByRole('button', { name: '1d' }).click();
  await expect(dialog.getByLabel('Duration (minutes, optional)')).toHaveValue('1440');
  await durationShortcuts.getByRole('button', { name: '0m', exact: true }).click();
  await expect(dialog.getByLabel('Duration (minutes, optional)')).toHaveValue('0');
  await dialog
    .getByRole('group', { name: 'Duration shortcuts' })
    .getByRole('button', { name: '2h' })
    .click();
  await expect(dialog.getByLabel('Duration (minutes, optional)')).toHaveValue('120');
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/v1/tasks/${task.id}/schedule`)).json())
          .duration_minutes,
    )
    .toBe(120);
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto(`/tasks/${task.id}`);
  await page.locator('.task-context-menu > summary').click();
  await page.locator('[data-open-advanced]').click();
  const panel = page.getByRole('dialog', { name: 'Task details' });
  await expect(panel.getByText('Duration · 2h')).toBeVisible();
  await panel.getByText('Duration · 2h').click();
  await panel
    .getByRole('group', { name: 'Duration shortcuts' })
    .getByRole('button', { name: '30m' })
    .click();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/v1/tasks/${task.id}/schedule`)).json())
          .duration_minutes,
    )
    .toBe(30);
  const current = await (
    await page.request.get(`/api/v1/tasks/${task.id}/schedule`)
  ).json();
  const unplan = await page.request.put(`/api/v1/tasks/${task.id}/placement`, {
    headers: { 'x-csrf-token': csrf },
    data: { revision: current.task_revision, date: null, time: null, timezone: 'UTC' },
  });
  expect(unplan.ok()).toBeTruthy();
  expect(
    (await (await page.request.get(`/api/v1/tasks/${task.id}/schedule`)).json())
      .duration_minutes,
  ).toBe(30);
  expect(date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  const childResponse = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: inbox.list_id,
      tasks: [{ title: 'Duration subtask', parent_id: task.id }],
    },
  });
  expect(childResponse.ok()).toBeTruthy();
  const [child] = await childResponse.json();
  await page.goto(`/tasks/${child.id}`);
  await page.locator('.task-context-menu > summary').click();
  await page.locator('[data-open-advanced]').click();
  const childPanel = page.getByRole('dialog', { name: 'Task details' });
  await childPanel.getByText('Set duration').click();
  await childPanel
    .getByRole('group', { name: 'Duration shortcuts' })
    .getByRole('button', { name: '4h' })
    .click();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/v1/tasks/${child.id}/schedule`)).json())
          .duration_minutes,
    )
    .toBe(240);
});

test('calendar lays overlapping durations in separate lanes', async ({ page }) => {
  await page.clock.install({ time: new Date('2026-11-01T12:00:00Z') });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const prefix = `Overlap ${randomUUID()}`;
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: inbox.list_id,
      tasks: [
        { title: `${prefix} A` },
        { title: `${prefix} B` },
        { title: `${prefix} C` },
      ],
    },
  });
  const tasks = await response.json();
  for (const [i, task] of tasks.entries()) {
    const saved = await page.request.put(`/api/v1/tasks/${task.id}/placement`, {
      headers: { 'x-csrf-token': csrf },
      data: {
        revision: task.revision,
        date: '2026-11-01',
        time: i ? '10:30' : '10:00',
        timezone: 'UTC',
        duration_minutes: i ? 30 : 120,
        priority: i === 1 ? 0 : 3,
      },
    });
    expect(saved.ok()).toBeTruthy();
  }
  await page.goto('/plan?view=day&date=2026-11-01');
  const cards = page.locator('.calendar-timed-event').filter({ hasText: prefix });
  await expect(cards).toHaveCount(3);
  const boxes = await cards.evaluateAll((nodes) =>
    nodes.map((node) => ({
      title: node.textContent,
      rect: node.getBoundingClientRect().toJSON(),
    })),
  );
  const a = boxes.find((box) => box.title.includes(`${prefix} A`)).rect;
  const b = boxes.find((box) => box.title.includes(`${prefix} B`)).rect;
  const c = boxes.find((box) => box.title.includes(`${prefix} C`)).rect;
  expect(a.height).toBeGreaterThan(b.height * 3);
  expect(a.right).toBeLessThanOrEqual(b.left);
  expect(b.right).toBeLessThanOrEqual(c.left);
});
