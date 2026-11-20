import { expect, test } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test.use({ serviceWorkers: 'block' });

async function setup(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name=csrf-token]').getAttribute('content');
  const api = async (path, method = 'GET', data) => {
    const response = await page.request.fetch(`/api/v1${path}`, {
      method,
      data,
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
    });
    expect(response.ok(), await response.text()).toBe(true);
    return response.json();
  };
  const space = await api('/workspaces', 'POST', {
    title: `Capture test ${randomUUID()}`,
  });
  const list = await api('/lists', 'POST', { workspace_id: space.id, title: 'Feedback' });
  return { api, list };
}

for (const eventTiming of ['before', 'after']) {
  test(`a slow list capture settles quietly with a live update ${eventTiming} its response`, async ({
    page,
  }) => {
    const { api, list } = await setup(page);
    const section = await api(`/lists/${list.id}/sections`, 'POST', {
      title: 'Destination',
    });
    await page.addInitScript(
      ({ listId, delayUpdate }) => {
        const listen = EventSource.prototype.addEventListener;
        EventSource.prototype.addEventListener = function (type, listener, options) {
          if (
            type !== 'changes' ||
            new URL(this.url).searchParams.get('list_id') !== listId
          )
            return listen.call(this, type, listener, options);
          listen.call(
            this,
            type,
            (event) => {
              if (JSON.parse(event.data).count > 0 && !window.captureLiveUpdate) {
                window.captureLiveUpdate = true;
                if (delayUpdate) {
                  window.deliverCaptureLiveUpdate = () => listener.call(this, event);
                  return;
                }
              }
              listener.call(this, event);
            },
            options,
          );
        };
      },
      { listId: list.id, delayUpdate: eventTiming === 'after' },
    );
    await page.goto(`/lists/${list.id}`);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
    await page.evaluate(() => {
      window.captureSignals = [];
      new MutationObserver(() => {
        if (!document.querySelector('#refresh-notice').hidden)
          window.captureSignals.push('Refresh tasks');
        for (const element of document.querySelectorAll('.pending-task-state,#notice')) {
          const label = element.textContent.trim();
          if (label && label !== 'Saving…') window.captureSignals.push(label);
        }
      }).observe(document.body, {
        subtree: true,
        childList: true,
        attributes: true,
        attributeFilter: ['hidden'],
      });
    });
    const save = Promise.withResolvers();
    await page.route('**/api/v1/capture', async (route) => {
      const result = await route.fetch();
      await save.promise;
      await route.fulfill({ response: result });
    });
    const group = page.locator(`[data-section-id="${section.id}"]`);
    await group.getByRole('button', { name: 'Add task', exact: true }).click();
    const form = group.locator('.section-capture');
    await form.getByLabel('Task name').fill('Optimistic list item');
    await form.getByRole('button', { name: 'Add task' }).click();
    try {
      await expect(group.locator('.pending-task')).toContainText('Optimistic list item');
      await expect(group.locator('.pending-task')).toContainText('Saving');
      await expect(group.getByRole('link', { name: 'Optimistic list item' })).toHaveCount(
        0,
      );
      await page.locator('#capture-open').click();
      await page.locator('#capture-title').fill('Keep this unrelated draft');
      if (eventTiming === 'before')
        await page.waitForFunction(() => window.captureLiveUpdate);
      await expect(page.locator('#refresh-notice')).toBeHidden();
    } finally {
      save.resolve();
    }
    await expect(group.locator('.pending-task')).toHaveCount(0);
    await expect(group.getByRole('link', { name: 'Optimistic list item' })).toHaveCount(
      1,
    );
    await expect(page.locator('#capture-title')).toHaveValue('Keep this unrelated draft');
    if (eventTiming === 'after') {
      await page.waitForFunction(() => window.captureLiveUpdate);
      const checkedChanges = page.waitForResponse(
        (response) => new URL(response.url()).pathname === '/api/v1/changes',
      );
      await page.evaluate(() => window.deliverCaptureLiveUpdate());
      await checkedChanges;
    }
    await expect(page.locator('#refresh-notice')).toBeHidden();
    expect(await page.evaluate(() => window.captureSignals)).toEqual([]);
    if (eventTiming === 'after') {
      await api('/capture', 'POST', {
        list_id: list.id,
        tasks: [{ title: 'External task' }],
      });
      await expect(page.locator('#refresh-notice')).toBeVisible();
      await expect(page.locator('#capture-title')).toHaveValue(
        'Keep this unrelated draft',
      );
    }
  });
}

test('an offline capture remains visibly queued and settles when the connection returns', async ({
  page,
  context,
}) => {
  const { list } = await setup(page);
  await page.goto(`/lists/${list.id}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  await page.locator('#capture-open').click();
  await context.setOffline(true);
  await page.locator('#capture-title').fill('Offline test item');
  await page.locator('#capture-form button[type=submit]').click();
  await expect(page.locator('#task-list .pending-task')).toContainText('Pending sync');
  await expect(page.locator('#pending-captures')).toContainText('Offline test item');
  await context.setOffline(false);
  await expect(page.locator('#task-list .pending-task')).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Offline test item' })).toHaveCount(1);
  await expect(page.locator('#pending-captures')).toBeEmpty();
});

test('a rejected subtask stops showing as pending and keeps its draft for a successful retry', async ({
  page,
}) => {
  const { api, list } = await setup(page);
  const [task] = await api('/capture', 'POST', {
    list_id: list.id,
    tasks: [{ title: 'Parent task' }],
  });
  await page.goto(`/tasks/${task.id}`);
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  const save = Promise.withResolvers();
  await page.route(
    '**/api/v1/capture',
    async (route) => {
      await save.promise;
      await route.fulfill({
        status: 422,
        json: { error: { message: 'Test rejection' } },
      });
    },
    { times: 1 },
  );
  await page.locator('[data-child-composer] > summary').click();
  const form = page.locator('[data-child-form]');
  const title = form.getByLabel('New sub-task');
  const submit = form.getByRole('button', { name: 'Add sub-task' });
  await title.fill('Optimistic child');
  await submit.click();
  try {
    await expect(page.locator('[data-children] .pending-task')).toContainText(
      'Optimistic child',
    );
  } finally {
    save.resolve();
  }
  await expect(page.locator('[data-children] .pending-task')).toHaveCount(0);
  await expect(page.locator('[data-task-notice]')).toContainText('Test rejection');
  await expect(title).toHaveValue('Optimistic child');
  await submit.click();
  await expect(
    page.locator('[data-children]').getByRole('link', { name: 'Optimistic child' }),
  ).toHaveCount(1);
  await expect(title).toBeEmpty();
});
