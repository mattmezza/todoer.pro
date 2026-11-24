import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('task reminders support dates, times and custom offsets', async ({ page }) => {
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
  await page.goto(`/tasks/${task.id}`);
  const openPlanning = async () => {
    await page.locator('[data-task-due]').click();
    await expect(page.getByRole('dialog', { name: 'Task details' })).toBeVisible();
  };
  await openPlanning();
  const form = page.locator('[data-schedule-form]');
  const addReminder = form.getByRole('button', { name: 'Add reminder', exact: true });
  const timings = form.getByLabel('When', { exact: true });
  const planned = form.locator('[name=planned_date]');
  const waitForSave = async (change) => {
    const saved = page.waitForResponse(
      (r) =>
        r.url().endsWith(`/tasks/${task.id}/schedule`) && r.request().method() === 'PUT',
    );
    await change();
    const response = await saved;
    expect(response.ok(), await response.text()).toBeTruthy();
    return (await response.json()).schedule;
  };

  await form.getByText('Reminders', { exact: true }).click();
  await addReminder.click();
  await expect(timings.first().locator('optgroup')).toHaveAttribute('disabled', '');
  await form.locator('[name=reminder_timezone]').fill('UTC');
  const fixed = await waitForSave(() =>
    form.locator('[name=reminder_local]').fill('2030-06-15T10:00'),
  );
  expect(fixed.reminders[0].at).toEqual({
    kind: 'timed',
    local: '2030-06-15T10:00:00',
    timezone: 'UTC',
  });

  await form.getByText('Set planned date', { exact: true }).click();
  await waitForSave(() => planned.fill('2030-06-17'));
  for (const [label, days] of [
    ['At event time', 0],
    ['1 day before', 1],
    ['2 days before', 2],
  ]) {
    await addReminder.click();
    const schedule = await waitForSave(() => timings.last().selectOption({ label }));
    expect(schedule.reminders.at(-1).at).toMatchObject({
      kind: 'relative',
      days,
      hours: 0,
    });
  }
  await addReminder.click();
  await timings.last().selectOption({ label: 'Custom offset' });
  await waitForSave(() => form.getByLabel('Days before').fill('1'));
  const custom = await waitForSave(() => form.getByLabel('Hours before').fill('5'));
  expect(custom.reminders.at(-1).at).toMatchObject({
    kind: 'relative',
    days: 1,
    hours: 5,
  });

  await page.reload();
  await openPlanning();
  await expect(timings).toHaveCount(5);
  await expect(timings.last()).toHaveValue('custom');
  await expect(form.getByLabel('Days before')).toHaveValue('1');
  await expect(form.getByLabel('Hours before')).toHaveValue('5');
  await form.getByText(/^Planned · /).click();
  const cleared = await waitForSave(() => planned.fill(''));
  expect(cleared.planned).toBeNull();
  expect(cleared.reminders).toEqual(custom.reminders);
  await expect(
    form.getByText('Set a date to activate this reminder.').filter({ visible: true }),
  ).toHaveCount(4);
});
