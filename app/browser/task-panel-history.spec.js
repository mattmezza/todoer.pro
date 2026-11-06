import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

for (const width of [390, 1280]) {
  test(`task panels use one history entry and subtask rows share the flow at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
    const post = async (path, data) => {
      const response = await page.request.post(`/api/v1/${path}`, {
        headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
        data,
      });
      expect(response.ok(), await response.text()).toBeTruthy();
      return response.json();
    };
    const space = await post('workspaces', { title: `Panels ${randomUUID()}` });
    const list = await post('lists', {
      workspace_id: space.id,
      title: 'Panel navigation',
    });
    const capture = async (title, parent_id = null) =>
      (
        await post('capture', {
          list_id: list.id,
          tasks: [{ title, parent_id, description: `${title} brief` }],
        })
      )[0];
    const parent = await capture('Parent');
    const child = await capture('Child', parent.id);
    const grandchild = await capture('Grandchild', child.id);
    await page.goto('/today');
    await page.goto(`/lists/${list.id}`);
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    await page.evaluate(() => {
      window.panelBackground = document.querySelector('#main');
    });
    const listURL = page.url();
    const length = await page.evaluate(() => history.length);
    const panel = page.getByRole('complementary', { name: 'Task details', exact: true });
    const assertPanel = async (id) => {
      await expect(panel.locator('[data-task-page]')).toHaveAttribute('data-task-id', id);
      await expect(panel.locator('.planning-detail-content')).not.toHaveAttribute(
        'aria-busy',
        'true',
      );
    };
    await page.locator(`.task[data-id="${parent.id}"] .task-description`).click();
    await assertPanel(parent.id);
    expect(await page.evaluate(() => history.length)).toBe(length + 1);
    await panel
      .locator(`[data-child-id="${child.id}"] > [data-child-description]`)
      .click();
    await assertPanel(child.id);
    expect(await page.evaluate(() => history.length)).toBe(length + 1);
    await page.goBack();
    await expect(panel).toHaveCount(0);
    await expect(page).toHaveURL(listURL);
    expect(
      await page.evaluate(
        () => window.panelBackground === document.querySelector('#main'),
      ),
    ).toBe(true);
    await page.goForward();
    await assertPanel(child.id);
    await page.getByRole('button', { name: 'Close task details', exact: true }).click();
    await expect(panel).toHaveCount(0);
    await page.goForward();
    await assertPanel(child.id);
    // Titles retain their normal full-page links, including inside a panel.
    await panel
      .locator(`[data-child-id="${grandchild.id}"] > .task-page-child-line > a`)
      .click();
    await expect(page.locator('main[data-task-page]')).toHaveAttribute(
      'data-task-id',
      grandchild.id,
    );
    await expect(panel).toHaveCount(0);
    await page.goBack();
    await assertPanel(child.id);
    await page.goBack();
    await expect(panel).toHaveCount(0);
    await expect(page).toHaveURL(listURL);
    // The same row hit area opens a panel on the dedicated task page.
    await page.goto(`/tasks/${parent.id}`);
    const taskURL = page.url();
    await page
      .locator(`main [data-child-id="${child.id}"] > [data-child-description]`)
      .click();
    await assertPanel(child.id);
    await panel.locator('[data-child-composer] > summary').click();
    await panel
      .getByLabel('New sub-task', { exact: true })
      .fill('Panel-only child draft');
    await expect(page.locator('body > main [data-child-form] [name=title]')).toHaveValue(
      '',
    );
    const duplicateIds = await page.evaluate(() => {
      const ids = [...document.querySelectorAll('[id]')].map((el) => el.id);
      return ids.filter((id, index) => ids.indexOf(id) !== index);
    });
    expect(duplicateIds).toEqual([]);
    await page.goBack();
    await expect(panel).toHaveCount(0);
    await expect(page).toHaveURL(taskURL);
    await expect(page.locator('main[data-task-page]')).toHaveAttribute(
      'data-task-id',
      parent.id,
    );
  });
}

test('Inbox, Today and calendar panel history keeps the rendered background', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const api = async (path, method = 'GET', data) => {
    const response = await page.request.fetch(`/api/v1/${path}`, {
      method,
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  };
  const inbox = await api('inbox');
  const title = `History ${randomUUID()}`;
  const [task] = await api('capture', 'POST', {
    list_id: inbox.list_id,
    tasks: [{ title }],
  });
  await page.goto('/today');
  const date = await page
    .locator('[data-planning-today]')
    .getAttribute('data-planning-today');
  const schedule = await api(`tasks/${task.id}/schedule`);
  await api(`tasks/${task.id}/schedule`, 'PUT', {
    revision: schedule.task_revision,
    schedule: { ...schedule.schedule, planned: { kind: 'date', date } },
  });
  const panel = page.getByRole('complementary', { name: 'Task details', exact: true });
  for (const path of ['/inbox', '/today', '/plan']) {
    await page.goto(path);
    const link = page
      .locator('#main a[href^="/tasks/"]')
      .filter({ hasText: title })
      .first();
    await expect(link).toBeVisible();
    await page.evaluate(() => {
      window.panelBackground = document.querySelector('#main');
    });
    const backgroundURL = page.url();
    await link.click();
    await expect(panel.locator('[data-task-page]')).toHaveAttribute(
      'data-task-id',
      task.id,
    );
    await page.goBack();
    await expect(panel).toHaveCount(0);
    await expect(page).toHaveURL(backgroundURL);
    // Wait through HTMX's asynchronous history hook before checking identity.
    await expect(page.locator('#main')).not.toHaveAttribute('aria-busy', 'true');
    expect(
      await page.evaluate(
        () => window.panelBackground === document.querySelector('#main'),
      ),
    ).toBe(true);
    await page.goForward();
    await expect(panel.locator('[data-task-page]')).toHaveAttribute(
      'data-task-id',
      task.id,
    );
    await page.getByRole('button', { name: 'Close task details', exact: true }).click();
    await expect(panel).toHaveCount(0);
  }
});
