import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('Today and all-day calendar entries follow priority and keep actions aligned', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const prefix = `Priority ${randomUUID().slice(0, 8)}`;
  const response = await page.request.post('/api/v1/capture', {
    headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
    data: {
      list_id: inbox.list_id,
      tasks: [3, 1, 0, 2].map((p) => ({ title: `${prefix} P${p}` })),
    },
  });
  expect(response.ok()).toBeTruthy();
  const tasks = await response.json();
  await page.goto('/today');
  const date = await page
    .locator('main[data-planning-today]')
    .getAttribute('data-planning-today');
  for (const [index, priority] of [3, 1, 0, 2].entries()) {
    const task = tasks[index];
    const schedule = await (
      await page.request.get(`/api/v1/tasks/${task.id}/schedule`)
    ).json();
    const saved = await page.request.put(`/api/v1/tasks/${task.id}/schedule`, {
      headers: { 'x-csrf-token': csrf },
      data: {
        revision: schedule.task_revision,
        schedule: { ...schedule.schedule, priority, planned: { kind: 'date', date } },
      },
    });
    expect(saved.ok()).toBeTruthy();
  }
  await page.reload();
  const rows = page.locator('.planning-task').filter({ hasText: prefix });
  await expect(rows.locator('.task-title')).toHaveText(
    [0, 1, 2, 3].map((p) => `${prefix} P${p}`),
  );
  await rows.first().locator('.task-title').click();
  await expect(page.getByLabel('Task details', { exact: true })).toBeVisible();
  await expect
    .poll(() =>
      rows.first().evaluate((row) => {
        const title = row.querySelector('.task-title').getBoundingClientRect();
        const priority = row
          .querySelector('.task-priority-summary')
          .getBoundingClientRect();
        return priority.left >= title.right && priority.top < title.bottom;
      }),
    )
    .toBe(true);
  await page.goto('/plan');
  const cards = page
    .locator('.calendar-all-day .calendar-event')
    .filter({ hasText: prefix });
  await expect(cards).toHaveCount(4);
  await expect(cards.locator('.workbench-task-copy a')).toHaveText([
    `!!!${prefix} P0`,
    `!!${prefix} P1`,
    `!${prefix} P2`,
    `${prefix} P3`,
  ]);
  const clock = cards
    .first()
    .getByRole('button', { name: `Plan date and time for ${prefix} P0` });
  await expect(clock.locator('use')).toHaveAttribute('href', '/assets/icons.svg#clock');
  await clock.click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.getByRole('dialog').press('Escape');
  for (const task of tasks.slice(0, 3)) {
    const current = await (await page.request.get(`/api/v1/tasks/${task.id}`)).json();
    const placed = await page.request.put(`/api/v1/tasks/${task.id}/placement`, {
      headers: { 'x-csrf-token': csrf },
      data: { revision: current.revision, date, time: '10:00', timezone: 'UTC' },
    });
    expect(placed.ok()).toBeTruthy();
  }
  await page.reload();
  const timed = page.locator('.calendar-timed-event').filter({ hasText: prefix });
  await expect(timed).toHaveCount(3);
  const positions = await timed.evaluateAll((cards) =>
    cards.map((card) => ({
      text: card.querySelector('.workbench-task-copy a').textContent,
      x: card.getBoundingClientRect().x,
    })),
  );
  positions.sort((a, b) => a.x - b.x);
  expect(positions.map((card) => card.text.match(/P[0-3]$/)?.[0])).toEqual([
    'P0',
    'P1',
    'P3',
  ]);
});

test('Today filters update tasks in place and support browser history', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  await page.evaluate(() => {
    window.filterPageMarker = 'still here';
  });
  let release;
  const hold = new Promise((resolve) => {
    release = resolve;
  });
  let captured;
  const requested = new Promise((resolve) => {
    captured = resolve;
  });
  await page.route('**/today?*', async (route) => {
    if (new URL(route.request().url()).searchParams.get('range') === 'tomorrow') {
      captured();
      await hold;
    }
    await route.continue();
  });
  await page
    .getByRole('navigation', { name: 'Planning' })
    .getByRole('link', { name: 'Tomorrow' })
    .click();
  await requested;
  await expect(
    page.locator('[data-planning-rows] .skeleton-planning-list .skeleton-row'),
  ).toHaveCount(3);
  await expect(page.locator('[data-planning-rows]')).toHaveAttribute('aria-busy', 'true');
  release();
  await expect(page).toHaveURL(/range=tomorrow/);
  await expect(
    page
      .getByRole('navigation', { name: 'Planning' })
      .getByRole('link', { name: 'Tomorrow' }),
  ).toHaveAttribute('aria-current', 'page');
  expect(await page.evaluate(() => window.filterPageMarker)).toBe('still here');
  await page.getByRole('button', { name: 'P0', exact: true }).click();
  await expect(page).toHaveURL(/prio=0/);
  await page.goBack();
  await expect(page.getByRole('button', { name: 'All priorities' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await page.goBack();
  await expect(
    page
      .getByRole('navigation', { name: 'Planning' })
      .getByRole('link', { name: 'Today' }),
  ).toHaveAttribute('aria-current', 'page');
  expect(await page.evaluate(() => window.filterPageMarker)).toBe('still here');
});

test('Today filter failures preserve the list and offer a retry', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/today');
  await page.route('**/today?*', (route) => route.abort());
  await page
    .getByRole('navigation', { name: 'Planning' })
    .getByRole('link', { name: 'Tomorrow' })
    .click();
  await expect(page.getByRole('alert')).toContainText('Could not load');
  await expect(page.locator('[data-planning-rows]')).not.toHaveAttribute(
    'aria-busy',
    'true',
  );
  await page.unroute('**/today?*');
  await page.getByRole('alert').getByRole('button', { name: 'Try again' }).click();
  await expect(page).toHaveURL(/range=tomorrow/);
  await expect(page.getByRole('alert')).toHaveCount(0);
});
