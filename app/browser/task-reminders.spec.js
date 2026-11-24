import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({ timezoneId: 'UTC' });

async function createTask(page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const created = await page.request.post('/api/v1/capture', {
    headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
    data: { list_id: 'inbox', tasks: [{ title: `Reminder ${randomUUID()}` }] },
  });
  expect(created.ok()).toBeTruthy();
  const [task] = await created.json();
  return task;
}

async function openPlanning(page) {
  await page.locator('[data-task-due]').click();
  await expect(page.getByRole('dialog', { name: 'Task details' })).toBeVisible();
  return page.locator('[data-schedule-form]');
}

async function waitForSave(page, task, change) {
  const [response] = await Promise.all([
    page.waitForResponse(
      (r) =>
        r.url().endsWith(`/tasks/${task.id}/schedule`) && r.request().method() === 'PUT',
    ),
    change(),
  ]);
  expect(response.ok(), await response.text()).toBeTruthy();
  return (await response.json()).schedule;
}

test('task reminders support dates, times and custom offsets', async ({ page }) => {
  const task = await createTask(page);
  await page.goto(`/tasks/${task.id}`);
  const form = await openPlanning(page);
  const addReminder = form.getByRole('button', { name: 'Add reminder', exact: true });
  const timings = form.getByRole('combobox', { name: 'When', exact: true });
  const planned = form.locator('[name=planned_date]');
  const daysBefore = form.getByRole('spinbutton', { name: 'Days before' });
  const hoursBefore = form.getByRole('spinbutton', { name: 'Hours before' });
  const save = (change) => waitForSave(page, task, change);

  await form.getByText('Reminders', { exact: true }).click();
  await addReminder.click();
  await expect(timings.first().locator('optgroup')).toHaveAttribute('disabled', '');
  await form.locator('[name=reminder_timezone]').fill('UTC');
  const fixed = await save(() =>
    form.locator('[name=reminder_local]').fill('2030-06-15T10:00'),
  );
  expect(fixed.reminders[0].at).toEqual({
    kind: 'timed',
    local: '2030-06-15T10:00:00',
    timezone: 'UTC',
  });

  await form.getByText('Set planned date', { exact: true }).click();
  await save(() => planned.fill('2030-06-17'));
  for (const [label, days] of [
    ['At event time', 0],
    ['1 day before', 1],
    ['2 days before', 2],
  ]) {
    await addReminder.click();
    const schedule = await save(() => timings.last().selectOption({ label }));
    expect(schedule.reminders.at(-1).at).toMatchObject({
      kind: 'relative',
      days,
      hours: 0,
    });
  }
  await addReminder.click();
  await timings.last().selectOption({ label: 'Custom offset' });
  await save(() => daysBefore.fill('1'));
  const custom = await save(() => hoursBefore.fill('5'));
  expect(custom.reminders.at(-1).at).toMatchObject({
    kind: 'relative',
    days: 1,
    hours: 5,
  });

  await page.reload();
  await openPlanning(page);
  await expect(timings).toHaveCount(5);
  await expect(timings.last()).toHaveValue('custom');
  await expect(daysBefore).toHaveValue('1');
  await expect(hoursBefore).toHaveValue('5');
  await form.getByText(/^Planned · /).click();
  const cleared = await save(() => planned.fill(''));
  expect(cleared.planned).toBeNull();
  expect(cleared.reminders).toEqual(custom.reminders);
  await expect(
    form.getByText('Set a date to activate this reminder.').filter({ visible: true }),
  ).toHaveCount(4);
});

test('quick reminder times save without a planned or due date', async ({ page }) => {
  const task = await createTask(page);
  await page.clock.setFixedTime(new Date('2030-03-30T23:30:00Z'));
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const preferences = '/api/v1/notification-preferences';
  const original = await (await page.request.get(preferences)).json();
  const savePreferences = async (data) => {
    const response = await page.request.put(preferences, {
      headers: { 'X-CSRF-Token': csrf },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
  };

  await savePreferences({ ...original, timezone: 'Europe/Zurich', week_start: 7 });
  try {
    await page.goto(`/tasks/${task.id}`);
    const form = await openPlanning(page);
    await form.getByText('Reminders', { exact: true }).click();
    const addReminder = form.getByRole('button', { name: 'Add reminder', exact: true });
    const shortcuts = form.getByRole('group', { name: 'Quick reminder times' });
    const reminders = [];
    // It is already Sunday in Zurich. Next week starts the following Sunday.
    for (const [name, date] of [
      ['Tomorrow at 9 AM', '2030-04-01'],
      ['Next week at 9 AM', '2030-04-07'],
    ]) {
      await addReminder.click();
      const schedule = await waitForSave(page, task, () =>
        shortcuts.last().getByRole('button', { name }).click(),
      );
      reminders.push({
        channel: 'email',
        at: { kind: 'timed', local: `${date}T09:00:00`, timezone: 'Europe/Zurich' },
      });
      expect(schedule.planned).toBeNull();
      expect(schedule.due).toBeNull();
      expect(schedule.reminders).toEqual(reminders);
    }

    await page.reload();
    await openPlanning(page);
    const times = form.locator('[name=reminder_local]');
    await expect(times).toHaveCount(2);
    for (const [index, reminder] of reminders.entries()) {
      await expect(times.nth(index)).toHaveValue(reminder.at.local.slice(0, 16));
    }
  } finally {
    await savePreferences(original);
  }
});
