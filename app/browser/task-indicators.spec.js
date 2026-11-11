import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

for (const width of [390, 1280]) {
  test(`comment and subtask indicators follow tasks and subtasks at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
    const post = async (url, data) => {
      const response = await page.request.post(`/api/v1/${url}`, {
        headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
        data,
      });
      expect(response.ok(), await response.text()).toBeTruthy();
      return response.json();
    };
    const space = await post('workspaces', { title: `Comments ${randomUUID()}` });
    const list = await post('lists', { workspace_id: space.id, title: 'Comment counts' });
    const capture = async (title, parent_id = null) =>
      (await post('capture', { list_id: list.id, tasks: [{ title, parent_id }] }))[0];
    const parent = await capture('Parent with comments');
    const child = await capture('Child with comments', parent.id);
    const grandchild = await capture('Grandchild', child.id);
    const quiet = await capture('No comments');
    const comment = (task) =>
      post(`tasks/${task.id}/comments`, { body: 'A saved comment' });
    await comment(parent);
    await comment(parent);
    const firstChildComment = await comment(child);
    const row = (task) => page.locator(`.task[data-id="${task.id}"]`);
    const count = (target, number) =>
      target.getByRole('img', {
        name: `${number} ${number === 1 ? 'comment' : 'comments'}`,
        exact: true,
      });
    const discussionReads = [];
    page.on('request', (request) => {
      if (
        request.method() === 'GET' &&
        /\/(comments|discussion)(\?|$)/.test(request.url())
      )
        discussionReads.push(request.url());
    });
    await page.goto(`/lists/${list.id}`);
    await expect(count(row(parent), 2)).toBeVisible();
    await expect(row(child)).toHaveCount(0);
    await expect(
      row(parent).getByRole('img', { name: '1 subtask', exact: true }),
    ).toBeVisible();

    await expect(row(quiet).locator('.task-comment-count')).toHaveCount(0);
    expect(discussionReads).toEqual([]);
    const assertPosition = async (target, menuSelector) => {
      const badge = await target.locator('.task-comment-count').boundingBox();
      const menu = await target.locator(menuSelector).boundingBox();
      if (!menu) {
        const row = await target.boundingBox();
        expect(badge.x + badge.width).toBeLessThanOrEqual(row.x + row.width + 1);
        return;
      }
      expect(badge.x + badge.width).toBeLessThanOrEqual(menu.x + 1);
      if (width === 390) {
        const group = target.locator('.task-indicators').first();
        await expect(group).toHaveCSS('flex-direction', 'column');
        const groupBox = await group.boundingBox();
        expect(groupBox.y).toBeLessThanOrEqual(menu.y + menu.height);
      } else {
        expect(
          Math.abs(badge.y + badge.height / 2 - menu.y - menu.height / 2),
        ).toBeLessThan(5);
      }
    };
    await assertPosition(row(parent), '.task-menu');
    // The indicator is part of the existing row hit area.
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    await count(row(parent), 2).locator('svg').click();
    const panel = page.getByLabel('Task details', { exact: true });
    const subtask = panel.locator(`[data-child-id="${child.id}"]`);
    await expect(count(subtask, 1)).toBeVisible();
    await expect(
      subtask.getByRole('img', { name: '1 subtask', exact: true }),
    ).toBeVisible();
    await expect(
      panel.locator(
        '.panel-heading-tools .task-indicators, .task-page-heading .task-indicators',
      ),
    ).toHaveCount(0);
    await assertPosition(
      subtask,
      ':scope > .task-page-child-line > .task-child-menu > summary',
    );
    const secondChildComment = await comment(child);
    await expect(count(subtask, 2)).toBeVisible();
    await expect(count(row(parent), 2)).toBeAttached();
    await post(`comments/${firstChildComment.id}/delete`, {
      revision: firstChildComment.revision,
    });
    await expect(count(subtask, 1)).toBeVisible();
    await page.getByRole('button', { name: 'Close task details', exact: true }).click();
    await page.goto(`/tasks/${parent.id}`);
    const pageChild = page.locator(`[data-child-id="${child.id}"]`);
    await expect(count(pageChild, 1)).toBeVisible();
    await expect(
      pageChild.getByRole('img', { name: '1 subtask', exact: true }),
    ).toBeVisible();
    await expect(page.locator('.task-page-heading .task-indicators')).toHaveCount(0);
    await assertPosition(
      pageChild,
      ':scope > .task-page-child-line > .task-child-menu > summary',
    );
    await post(`comments/${secondChildComment.id}/delete`, {
      revision: secondChildComment.revision,
    });
    await expect(pageChild.locator('.task-comment-count')).toHaveCount(0);
    await post(`cli/lifecycle/subtask/${grandchild.id}`, { archive: true });
    await expect(pageChild.locator('.task-indicators')).toHaveCount(0);
    await page.goto(`/lists/${list.id}`);
    await expect(count(row(parent), 2)).toBeVisible();
    await expect(row(child).locator('.task-comment-count')).toHaveCount(0);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBeTruthy();
  });
}

test('Inbox rows include their comment count in the existing page request', async ({
  page,
}) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const post = async (url, data) => {
    const response = await page.request.post(`/api/v1/${url}`, {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  };
  const [task] = await post('capture', {
    list_id: inbox.list_id,
    tasks: [{ title: `Inbox comments ${randomUUID()}` }],
  });
  await post(`tasks/${task.id}/comments`, { body: 'Saved inbox comment' });
  await page.goto('/inbox');
  await expect(
    page
      .locator(`[data-task-id="${task.id}"]`)
      .getByRole('img', { name: '1 comment', exact: true }),
  ).toBeVisible();
});

test('both indicators appear in Inbox, Today, calendar views and search', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const inbox = await (await page.request.get('/api/v1/inbox')).json();
  const post = async (path, data) => {
    const response = await page.request.post(`/api/v1/${path}`, {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  };
  const title = `Breakdown ${randomUUID().slice(0, 8)}`;
  const [parent] = await post('capture', { list_id: inbox.list_id, tasks: [{ title }] });
  const [child] = await post('capture', {
    list_id: inbox.list_id,
    tasks: [{ title: `${title} child`, parent_id: parent.id }],
  });
  await post('capture', {
    list_id: inbox.list_id,
    tasks: [{ title: `${title} grandchild`, parent_id: child.id }],
  });
  for (const task of [parent, child])
    await post(`tasks/${task.id}/comments`, { body: 'A saved comment' });
  const indicators = async (target) => {
    await expect(target.locator('.task-comment-count')).toHaveAttribute(
      'aria-label',
      '1 comment',
    );
    await expect(target.locator('.task-subtask-count')).toHaveAttribute(
      'aria-label',
      '1 subtask',
    );
  };
  await page.goto('/inbox');
  await indicators(page.locator(`.inbox-task[data-task-id="${parent.id}"]`));
  await page.goto('/today');
  const date = await page
    .locator('main[data-planning-today]')
    .getAttribute('data-planning-today');
  for (const task of [parent, child]) {
    const current = await (
      await page.request.get(`/api/v1/tasks/${task.id}/schedule`)
    ).json();
    const response = await page.request.put(`/api/v1/tasks/${task.id}/schedule`, {
      headers: { 'x-csrf-token': csrf },
      data: {
        revision: current.task_revision,
        schedule: { ...current.schedule, planned: { kind: 'date', date } },
      },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
  }
  await page.reload();
  for (const task of [parent, child])
    await indicators(page.locator(`[data-planning-task="${task.id}"]`));
  // Isolate the day across projects and repeated runs on the same synthetic DB;
  // month cells only show their first three tasks before the overflow action.
  const daySeed = Number.parseInt(randomUUID().slice(0, 7), 16);
  const calendarDate = new Date(
    Date.UTC(2060 + (daySeed % 40), (daySeed >>> 6) % 12, 1 + ((daySeed >>> 10) % 28)),
  )
    .toISOString()
    .slice(0, 10);
  for (const task of [parent, child]) {
    const current = await (
      await page.request.get(`/api/v1/tasks/${task.id}/schedule`)
    ).json();
    const response = await page.request.put(`/api/v1/tasks/${task.id}/schedule`, {
      headers: { 'x-csrf-token': csrf },
      data: {
        revision: current.task_revision,
        schedule: { ...current.schedule, planned: { kind: 'date', date: calendarDate } },
      },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
  }
  await page.goto(`/plan?view=week&date=${calendarDate}`);
  for (const task of [parent, child])
    await indicators(page.locator(`.calendar-event[data-task-id="${task.id}"]`));
  const current = await (await page.request.get(`/api/v1/tasks/${child.id}`)).json();
  const placed = await page.request.put(`/api/v1/tasks/${child.id}/placement`, {
    headers: { 'x-csrf-token': csrf },
    data: {
      revision: current.revision,
      date: calendarDate,
      time: '10:00',
      timezone: 'UTC',
      duration_minutes: 30,
    },
  });
  expect(placed.ok(), await placed.text()).toBeTruthy();
  await page.reload();
  await indicators(page.locator(`.calendar-timed-event[data-task-id="${child.id}"]`));
  await page.goto(`/plan?view=month&date=${calendarDate}`);
  await indicators(
    page.locator('.calendar-month-task').filter({ hasText: title }).first(),
  );
  await page.setViewportSize({ width: 390, height: 844 });
  const monthTask = page
    .locator('.calendar-month-task')
    .filter({ hasText: title })
    .first();
  await indicators(monthTask);
  const monthBox = await monthTask.boundingBox();
  const countBox = await monthTask.locator('.task-indicators').boundingBox();
  expect(countBox.x + countBox.width).toBeLessThanOrEqual(
    monthBox.x + monthBox.width + 1,
  );
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(`/search?query=${encodeURIComponent(title)}`);
  for (const task of [parent, child])
    await indicators(page.locator(`[data-planning-task="${task.id}"]`).first());
  await page.keyboard.press('ControlOrMeta+k');
  const search = page.getByRole('dialog', { name: 'Quick search' });
  await search.getByRole('combobox').fill(title);
  await indicators(
    search
      .locator('.omnibar-result')
      .filter({ has: page.getByText(title, { exact: true }) }),
  );
});
