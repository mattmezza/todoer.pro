import { expect, test } from '@playwright/test';
import { randomUUID } from 'node:crypto';

const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };

async function openRecurrence(page, request) {
  const response = await request.post('/api/v1/capture', {
    headers: { ...auth, 'Idempotency-Key': randomUUID() },
    data: { list_id: 'inbox', tasks: [{ title: `Recurring task ${randomUUID()}` }] },
  });
  expect(response.ok(), await response.text()).toBe(true);
  const [task] = await response.json();
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto(`/tasks/${task.id}`);
  await page.locator('[data-task-due]').click();
  const panel = page.getByRole('dialog', { name: 'Task details' });
  await panel.getByText('Repeat this task', { exact: true }).click();
  return { task, panel, recurrence: panel.locator('[data-recurrence-form]') };
}

test('Start recurrence explains missing fields and accepts weekday rules', async ({
  page,
  request,
}) => {
  const { task, recurrence } = await openRecurrence(page, request);
  const button = recurrence.getByRole('button', {
    name: 'Start recurrence',
    exact: true,
  });
  const notice = recurrence.getByRole('status');
  await button.click();
  await expect(notice).toHaveText('Choose a first due date, time and timezone.');
  await recurrence.getByLabel('First due local date and time').fill('2030-06-17T10:00');
  await recurrence.getByLabel('Recurrence IANA timezone').fill('Europe/Zurich');
  await recurrence.getByLabel('Interval in days').fill('0');
  await recurrence.getByLabel('Repeat rule').selectOption('weekdays');
  await button.click();
  await expect(notice).toHaveText('Choose at least one weekday.');
  await recurrence.getByLabel('Monday').check();
  await recurrence.getByLabel('Friday').check();
  const saved = page.waitForResponse(
    (r) =>
      r.url().endsWith(`/tasks/${task.id}/series`) && r.request().method() === 'POST',
  );
  await button.click();
  const response = await saved;
  expect(response.status()).toBe(201);
  expect((await response.json()).spec.rule).toEqual({ kind: 'weekdays', days: [1, 5] });
  await expect(notice).toContainText('Recurring series created');
  await expect(button).toBeDisabled();
  await expect(page.locator('[data-task-due]')).toContainText('Due');
});

test('Start recurrence uses a planning save made while drafting the series', async ({
  page,
  request,
}) => {
  const { task, panel, recurrence } = await openRecurrence(page, request);
  await recurrence.getByLabel('First due local date and time').fill('2030-06-17T10:00');
  await recurrence.getByLabel('Recurrence IANA timezone').fill('Europe/Zurich');
  const planningSaved = page.waitForResponse(
    (r) =>
      r.url().endsWith(`/tasks/${task.id}/schedule`) && r.request().method() === 'PUT',
  );
  await panel.getByRole('radio', { name: 'P1 priority' }).check();
  expect((await planningSaved).status()).toBe(200);
  const saved = page.waitForResponse(
    (r) =>
      r.url().endsWith(`/tasks/${task.id}/series`) && r.request().method() === 'POST',
  );
  await recurrence.getByRole('button', { name: 'Start recurrence', exact: true }).click();
  expect((await saved).status()).toBe(201);
  const { schedule } = await (
    await request.get(`/api/v1/tasks/${task.id}/schedule`, { headers: auth })
  ).json();
  expect(schedule.priority).toBe(1);
  expect(schedule.due).toEqual({
    kind: 'timed',
    local: '2030-06-17T10:00:00',
    timezone: 'Europe/Zurich',
  });
});

test('a remote write preserves the recurrence draft until it is discarded', async ({
  page,
  request,
}) => {
  const { task, recurrence } = await openRecurrence(page, request);
  const start = recurrence.getByLabel('First due local date and time');
  await start.fill('2030-06-17T10:00');
  const updated = await request.patch(`/api/v1/tasks/${task.id}`, {
    headers: auth,
    data: { revision: task.revision, title: 'Title changed in another session' },
  });
  expect(updated.ok(), await updated.text()).toBe(true);
  await recurrence.getByRole('button', { name: 'Start recurrence', exact: true }).click();
  await expect(recurrence.getByRole('status')).toContainText('Your draft is preserved');
  await expect(start).toHaveValue('2030-06-17T10:00');
  await expect(page.locator('[data-edit-title]')).toHaveText(
    'Title changed in another session',
  );
  await recurrence.getByRole('button', { name: 'Discard recurrence draft' }).click();
  await page
    .getByRole('dialog', { name: 'Confirm change' })
    .getByRole('button', {
      name: 'Discard',
      exact: true,
    })
    .click();
  await expect(start).toBeEmpty();
  await start.fill('2030-06-18T09:00');
  await recurrence.getByRole('button', { name: 'Start recurrence', exact: true }).click();
  await expect(recurrence.getByRole('status')).toContainText('Recurring series created');
});
