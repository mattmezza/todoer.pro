import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

test('list preference filters the response and sections can be reordered and searched', async ({
  page,
  browserName,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const post = async (path, data) => {
    const response = await page.request.post(path, {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data,
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return response.json();
  };
  const space = await post('/api/v1/workspaces', { title: `Review ${randomUUID()}` });
  const list = await post('/api/v1/lists', {
    workspace_id: space.id,
    title: 'Review source',
  });
  const destinationTitle = `Needle ${randomUUID()}`;
  await post('/api/v1/lists', { workspace_id: space.id, title: destinationTitle });
  const first = await post(`/api/v1/lists/${list.id}/sections`, {
    title: 'First section',
    description: 'A description hidden on mobile',
  });
  const second = await post(`/api/v1/lists/${list.id}/sections`, {
    title: 'Second section',
  });
  const tasks = await post('/api/v1/capture', {
    list_id: list.id,
    tasks: [{ title: 'Finished unique review task' }, { title: 'Open review task' }],
  });
  const complete = await page.request.patch(`/api/v1/tasks/${tasks[0].id}`, {
    headers: { 'x-csrf-token': csrf },
    data: { revision: tasks[0].revision, complete: true },
  });
  expect(complete.ok()).toBeTruthy();
  // An older running server may still render the form without a description field.
  const legacyPage = `**/lists/${list.id}`;
  await page.route(legacyPage, async (route) => {
    const response = await route.fetch();
    const body = (await response.text()).replace(
      /<label class="sr-only" for="new-section-description">.*?<\/textarea>/,
      '',
    );
    await route.fulfill({ response, body });
  });
  await page.goto(`/lists/${list.id}`);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  await expect(page.locator('#new-section-description')).toHaveCount(1);
  await page.unroute(legacyPage);
  await expect(page.locator('main > .breadcrumbs')).toBeHidden();
  await expect(
    page.locator(`[data-section-id="${first.id}"] .section-description`),
  ).toBeHidden();
  await expect(
    page.locator(`[data-section-id="${first.id}"] [data-section-drag]`),
  ).toBeHidden();
  const account = await page.locator('meta[name="account-id"]').getAttribute('content');
  await page.evaluate(
    (id) => localStorage.setItem(`todoer:${id}:hide-completed`, 'true'),
    account,
  );
  await page.reload();
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  // Once migrated, both the document and its fragment omit the completed title.
  const html = await page.request.get(`/lists/${list.id}`);
  expect(await html.text()).not.toContain('Finished unique review task');
  const fragment = await page.request.get(`/fragments/lists/${list.id}/tasks`);
  expect(await fragment.text()).not.toContain('Finished unique review task');
  await page.locator(`[data-section-id="${first.id}"] .entity-menu summary`).click();
  await page.locator(`[data-section-id="${first.id}"] [data-section-move-open]`).click();
  const dialog = page.getByRole('dialog', { name: 'Move section' });
  await dialog.getByLabel('Search destination lists').fill(destinationTitle);
  await expect(dialog.getByLabel('Destination list').locator('option')).toHaveCount(1);
  await expect(dialog.getByLabel('Destination list').locator('option')).toHaveText(
    destinationTitle,
  );
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await page.locator(`[data-section-id="${second.id}"] .entity-menu summary`).click();
  await page.locator(`[data-section-id="${second.id}"] [data-section-up]`).click();
  const ids = () =>
    page
      .locator('.task-section[data-section-id]:not([data-section-id=""])')
      .evaluateAll((nodes) => nodes.map((node) => node.dataset.sectionId));
  await expect.poll(ids).toEqual([second.id, first.id]);
  if (browserName === 'chromium') {
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true });
    const source = await page
      .locator(`[data-section-id="${second.id}"] .section-heading h2`)
      .boundingBox();
    const target = await page
      .locator(`[data-section-id="${first.id}"] .section-heading`)
      .boundingBox();
    const x = source.x + source.width / 2,
      y = source.y + source.height / 2;
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [{ x, y }],
    });
    await page.waitForTimeout(450);
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ x: target.x + target.width / 2, y: target.y + target.height - 3 }],
    });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await expect.poll(ids).toEqual([first.id, second.id]);
  }
  await page.locator('[data-section-add-open]').click();
  await page.locator('#new-section-title').fill('New section');
  await page.locator('#new-section-description').fill('Section context');
  await page.locator('[data-section-create] button[type="submit"]').click();
  await expect(
    page.locator('.section-heading-copy h2').filter({ hasText: 'new-section' }),
  ).toBeVisible();
  const response = await page.request.get(`/api/v1/lists/${list.id}/sections`);
  expect(response.ok()).toBeTruthy();
  expect(await response.json()).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ title: 'New section', description: 'Section context' }),
    ]),
  );
});
