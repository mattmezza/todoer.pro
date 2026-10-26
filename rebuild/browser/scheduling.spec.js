import { test, expect } from '@playwright/test';
import crypto from 'node:crypto';
const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
async function openNew(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  const title = `Planning ${crypto.randomUUID()}`;
  await page.locator('#capture-title').fill(title);
  await page.locator('#capture-form button').click();
  const row = page
    .locator('.task')
    .filter({ has: page.getByRole('button', { name: title, exact: true }) });
  await expect(row).toHaveCount(1);
  const id = await row.getAttribute('data-id');
  await row.locator('.task-title').click();
  await page.locator('[data-scheduling] summary').click();
  await expect(page.locator('[data-schedule-form] [name=planned_date]')).toBeEnabled();
  return {
    id,
    title,
    row,
    form: page.locator('[data-schedule-form]'),
    recurrence: page.locator('[data-recurrence-form]'),
  };
}
for (const [name, width, height] of [
  ['phone', 390, 844],
  ['desktop', 1440, 1000],
]) {
  test(`${name}: mounted schedule saves date semantics, reminders and independent brief draft`, async ({
    page,
    request,
  }) => {
    await page.setViewportSize({ width, height });
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    const { id, form } = await openNew(page);
    await page.locator('#edit-title').fill('Brief draft survives schedule save');
    await form.locator('[name=planned_date]').fill('2030-06-12');
    await form.locator('[name=due_kind]').selectOption('date');
    await form.locator('[name=due_date]').fill('2030-06-15');
    await form.locator('[name=priority]').selectOption('1');
    await form.getByRole('button', { name: 'Add reminder', exact: true }).click();
    await form.locator('[name=channel]').selectOption('push');
    await form.locator('[name=reminder_local]').fill('2030-06-14T09:30');
    await form.locator('[name=reminder_timezone]').fill('Europe/Zurich');
    expect(
      await page.locator('#task-editor').evaluate((d) => d.scrollWidth <= d.clientWidth),
    ).toBe(true);
    for (const control of await form
      .locator('button:visible,select:visible,input:visible')
      .all()) {
      expect(Math.round((await control.boundingBox()).height)).toBeGreaterThanOrEqual(44);
    }
    if (name === 'phone')
      await page.screenshot({
        path: `.local/rebuild/scheduling-phone-${test.info().project.name}.png`,
      });
    const saved = page.waitForResponse(
      (r) => r.url().endsWith(`/tasks/${id}/schedule`) && r.request().method() === 'PUT',
    );
    await form.getByRole('button', { name: 'Save schedule', exact: true }).click();
    expect((await saved).status()).toBe(200);
    await expect(page.locator('[data-schedule-notice]')).toHaveText('Schedule saved.');
    await expect(page.locator('#edit-title')).toHaveValue(
      'Brief draft survives schedule save',
    );
    const schedule = await (
      await request.get(`/api/v1/tasks/${id}/schedule`, { headers: auth })
    ).json();
    expect(schedule.schedule.planned).toEqual({ kind: 'date', date: '2030-06-12' });
    expect(schedule.schedule.due).toEqual({ kind: 'date', date: '2030-06-15' });
    expect(schedule.schedule.priority).toBe(1);
    expect(schedule.schedule.reminders).toEqual([
      {
        channel: 'push',
        at: { kind: 'timed', local: '2030-06-14T09:30:00', timezone: 'Europe/Zurich' },
      },
    ]);
    await page.locator('#edit-form button[type=submit]').click();
    await expect(page.locator('#task-editor')).not.toBeVisible();
    const [task] = await (
      await request.get(`/api/v1/tasks/${id}/subtree`, { headers: auth })
    ).json();
    expect(task.title).toBe('Brief draft survives schedule save');
    expect(errors).toEqual([]);
  });
}
test('planning draft keeps original revision across close, reopen and actual concurrent write', async ({
  page,
  request,
}) => {
  const { id, title, form } = await openNew(page);
  await form.locator('[name=planned_date]').fill('2031-08-04');
  await page.keyboard.press('Escape');
  await expect(page.locator('#task-editor')).not.toBeVisible();
  await page.getByRole('button', { name: title, exact: true }).click();
  await expect(page.locator('[data-schedule-form] [name=planned_date]')).toHaveValue(
    '2031-08-04',
  );
  const [task] = await (
    await request.get(`/api/v1/tasks/${id}/subtree`, { headers: auth })
  ).json();
  expect(
    (
      await request.patch(`/api/v1/tasks/${id}`, {
        headers: auth,
        data: { revision: task.revision, title: 'Concurrent planning update' },
      })
    ).status(),
  ).toBe(200);
  await page.waitForTimeout(2200);
  await page
    .locator('[data-schedule-form]')
    .getByRole('button', { name: 'Save schedule', exact: true })
    .click();
  await expect(page.locator('[data-schedule-notice]')).toContainText(
    /draft is preserved|draft and original revision|draft began/i,
  );
  await expect(page.locator('[data-schedule-form] [name=planned_date]')).toHaveValue(
    '2031-08-04',
  );
  const saved = await (
    await request.get(`/api/v1/tasks/${id}/schedule`, { headers: auth })
  ).json();
  expect(saved.schedule.planned).toBeNull();
  await page.reload();
  await expect(page.locator('[data-schedule-form] [name=planned_date]')).toHaveValue(
    '2031-08-04',
  );
});
test('mounted recurrence sends weekday limits and sets a zoned first deadline', async ({
  page,
  request,
}) => {
  const { id, recurrence } = await openNew(page);
  await recurrence.locator('[name=rule]').selectOption('weekdays');
  await recurrence.locator('[name=weekday][value="1"]').check();
  await recurrence.locator('[name=weekday][value="5"]').check();
  await recurrence.locator('[name=start_local]').fill('2030-06-17T10:00');
  await recurrence.locator('[name=recurrence_timezone]').fill('Europe/Zurich');
  await recurrence.locator('[name=end_date]').fill('2030-12-31');
  await recurrence.locator('[name=count]').fill('12');
  const saved = page.waitForResponse(
    (r) => r.url().endsWith(`/tasks/${id}/series`) && r.request().method() === 'POST',
  );
  await recurrence.getByRole('button', { name: 'Start recurrence', exact: true }).click();
  const response = await saved;
  expect(response.status()).toBe(201);
  const series = await response.json();
  expect(series.spec.rule).toEqual({ kind: 'weekdays', days: [1, 5] });
  expect(series.spec.count).toBe(12);
  expect(series.spec.end_date).toBe('2030-12-31');
  await expect(page.locator('[data-schedule-notice]')).toContainText(
    'Recurring series created',
  );
  await expect(
    recurrence.getByRole('button', { name: 'Start recurrence', exact: true }),
  ).toBeDisabled();
  const schedule = await (
    await request.get(`/api/v1/tasks/${id}/schedule`, { headers: auth })
  ).json();
  expect(schedule.schedule.due).toEqual({
    kind: 'timed',
    local: '2030-06-17T10:00:00',
    timezone: 'Europe/Zurich',
  });
});
test('invalid DST time keeps the timed schedule draft and does not mutate the task', async ({
  page,
  request,
}) => {
  const { id, form } = await openNew(page);
  await form.locator('[name=due_kind]').selectOption('timed');
  await form.locator('[name=due_local]').fill('2030-03-31T02:30');
  await form.locator('[name=due_timezone]').fill('Europe/Zurich');
  await form.getByRole('button', { name: 'Save schedule', exact: true }).click();
  await expect(page.locator('[data-schedule-notice]')).toContainText(
    'nonexistent or ambiguous',
  );
  await expect(form.locator('[name=due_local]')).toHaveValue('2030-03-31T02:30');
  const schedule = await (
    await request.get(`/api/v1/tasks/${id}/schedule`, { headers: auth })
  ).json();
  expect(schedule.schedule.due).toBeNull();
});
for (const [kind, days] of [
  ['daily', 2],
  ['completion_relative', 3],
]) {
  test(`mounted ${kind} recurrence preserves interval and occurrence cap`, async ({
    page,
  }) => {
    const { id, recurrence } = await openNew(page);
    await recurrence.locator('[name=rule]').selectOption(kind);
    await recurrence.locator('[name=interval_days]').fill(String(days));
    await recurrence.locator('[name=start_local]').fill('2030-07-01T08:00');
    await recurrence.locator('[name=recurrence_timezone]').fill('UTC');
    await recurrence.locator('[name=count]').fill('2');
    const saved = page.waitForResponse(
      (r) => r.url().endsWith(`/tasks/${id}/series`) && r.request().method() === 'POST',
    );
    await recurrence
      .getByRole('button', { name: 'Start recurrence', exact: true })
      .click();
    const response = await saved;
    expect(response.status()).toBe(201);
    const series = await response.json();
    expect(series.spec.rule).toEqual(
      kind === 'daily' ? { kind, interval_days: days } : { kind, days },
    );
    expect(series.spec.count).toBe(2);
  });
}
