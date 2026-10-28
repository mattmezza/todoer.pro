import { test, expect } from '@playwright/test';
test.setTimeout(60000);
// Isolated browser presentation tests. Core tests cover production data,
// deduplication, leases, permissions and unread ownership.
test('bell, animated arrival, panel, pagination and failed acknowledgements', async ({
  page,
}) => {
  let unread = 1,
    latest = 1,
    failRead = false;
  const item = (id) => ({
    id,
    kind: 'due',
    title: `Notification task ${id}`,
    task_reference: 'T-example',
    list_title: 'Inbox',
    created_at: 1793200000,
    unread: true,
  });
  await page.route('**/api/v1/notifications**', async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/read')) {
      if (failRead) return route.fulfill({ status: 503, body: 'unavailable' });
      unread = 0;
      return route.fulfill({ status: 204 });
    }
    const items = url.searchParams.has('before') ? [item(1)] : [item(latest)];
    return route.fulfill({
      json: {
        items: items.map((n) => ({ ...n, unread: unread > 0 })),
        unread,
        latest_id: latest,
        next_cursor:
          url.searchParams.get('limit') === '25' && !url.searchParams.has('before')
            ? 1
            : null,
      },
    });
  });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const toggle = page.locator('[data-notification-toggle]');
  await expect(toggle).toHaveAttribute('aria-label', 'Notifications, 1 unread');
  await toggle.click();
  await expect(page.locator('.notification-popover')).toBeVisible();
  await expect(page.locator('[data-notification-recent]')).toContainText(
    'Notification task 1',
  );
  await page.keyboard.press('Escape');
  await expect(page.locator('.notification-popover')).toBeHidden();
  latest = 2;
  unread = 2;
  await page.evaluate(() => window.dispatchEvent(new Event('online')));
  await expect(toggle).toHaveClass(/notification-arrived/);
  await expect(page.locator('[data-notification-badge]')).toHaveText('2');
  await page.goto('/notifications');
  await expect(page.locator('[data-notification-items]')).toContainText(
    'Notification task 2',
  );
  await page.locator('[data-notification-more]').click();
  await expect(page.locator('[data-notification-items] .notification-item')).toHaveCount(
    2,
  );
  failRead = true;
  await page.locator('[data-notification-center] [data-notification-read-all]').click();
  await expect(
    page.locator('[data-notification-center] [data-notification-error]'),
  ).toBeVisible();
  await expect(page.locator('[data-notification-badge]')).toHaveText('2');
  failRead = false;
  await page.locator('[data-notification-center] [data-notification-read-all]').click();
  await expect(page.locator('[data-notification-badge]')).toBeHidden();
  await page.setViewportSize({ width: 360, height: 800 });
  await toggle.click();
  await expect(page.locator('.notification-popover')).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
});

test('real overdue task reaches the inbox without push and opens its task page', async ({
  page,
  request,
}) => {
  const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
  const title = `Notification acceptance ${Date.now()}`;
  const capture = await request.post('/api/v1/capture', {
    headers: { ...auth, 'Idempotency-Key': title },
    data: {
      list_id: 'inbox',
      tasks: [{ title, description: 'Notification integration check' }],
    },
  });
  expect(capture.ok(), await capture.text()).toBe(true);
  const tasks = await capture.json();
  const task = Array.isArray(tasks) ? tasks[0] : tasks.tasks[0];
  const schedule = await request.put(`/api/v1/tasks/${task.id}/schedule`, {
    headers: auth,
    data: {
      revision: task.revision,
      schedule: {
        due: {
          kind: 'timed',
          local: new Date(Date.now() - 60000).toISOString().slice(0, 19),
          timezone: 'UTC',
        },
        reminders: [],
      },
    },
  });
  expect(schedule.ok(), await schedule.text()).toBe(true);
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await expect(page.locator('[data-notification-badge]')).toHaveText('1', {
    timeout: 35000,
  });
  await page.locator('[data-notification-toggle]').click();
  const notice = page.locator('[data-notification-recent] a').filter({ hasText: title });
  await expect(notice).toBeVisible();
  await notice.click();
  await expect(page.locator('h1')).toContainText(title);
  await page.goto('/notifications');
  await expect(page.locator('[data-notification-badge]')).toBeHidden();
  await page.locator('[data-notification-filter="unread"]').click();
  await expect(page.locator('[data-notification-items]')).toContainText(
    'You’re all caught up.',
  );
});
