import { test, expect } from '@playwright/test';
import crypto from 'node:crypto';
const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
async function post(request, path, data) {
  const r = await request.post(`/api/v1${path}`, {
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data,
  });
  expect(r.ok(), await r.text()).toBe(true);
  return r.json();
}
async function fixture(request) {
  const suffix = crypto.randomUUID().slice(0, 8);
  const workspace = await post(request, '/workspaces', { title: `Order ${suffix}` });
  const list = await post(request, '/lists', {
    workspace_id: workspace.id,
    title: `Order ${suffix}`,
  });
  const roots = await post(request, '/capture', {
    list_id: list.id,
    tasks: ['Alpha', 'Bravo', 'Charlie'].map((name) => ({
      title: `${name} ${suffix}`,
      description: '',
      parent_id: null,
    })),
  });
  const children = await post(request, '/capture', {
    list_id: list.id,
    tasks: ['Child one', 'Child two'].map((name) => ({
      title: `${name} ${suffix}`,
      description: '',
      parent_id: roots[0].id,
    })),
  });
  const complete = await request.patch(`/api/v1/tasks/${roots[1].id}`, {
    headers: auth,
    data: { revision: roots[1].revision, complete: true },
  });
  expect(complete.ok()).toBe(true);
  return { workspace, list, roots, children };
}
async function open(page, list) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto(`/lists/${list.id}`);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
}
const row = (page, id) => page.locator(`#task-list .task[data-id="${id}"]`);
async function ids(request, list, parent = null) {
  return (
    await (await request.get(`/api/v1/lists/${list.id}/tasks`, { headers: auth })).json()
  )
    .filter((t) => t.parent_id === parent)
    .map((t) => t.id);
}
async function menu(page, id) {
  await row(page, id).locator('summary.task-menu').click();
}
for (const [name, width, height] of [
  ['phone', 390, 844],
  ['desktop', 1440, 1000],
]) {
  test(`${name}: Up and Down submit every sibling including completed tasks`, async ({
    page,
    request,
  }) => {
    await page.setViewportSize({ width, height });
    const f = await fixture(request);
    await open(page, f.list);
    const original = f.roots.map((t) => t.id);
    await menu(page, f.roots[2].id);
    const up = row(page, f.roots[2].id).locator('[data-order-up]');
    expect(Math.round((await up.boundingBox()).height)).toBeGreaterThanOrEqual(44);
    if (name === 'phone')
      await expect(row(page, f.roots[2].id).locator('[data-order-handle]')).toBeHidden();
    const sent = page.waitForResponse(
      (r) =>
        r.url().endsWith(`/lists/${f.list.id}/task-order`) &&
        r.request().method() === 'POST',
    );
    await up.click();
    const response = await sent;
    expect(response.status()).toBe(200);
    expect(response.request().postDataJSON().ids).toEqual([
      original[0],
      original[2],
      original[1],
    ]);
    expect(await ids(request, f.list)).toEqual([original[0], original[2], original[1]]);
    await expect
      .poll(() =>
        page
          .locator('#task-list .task[data-parent-id=""]')
          .evaluateAll((rows) => rows.map((r) => r.dataset.id)),
      )
      .toEqual([original[0], original[2], original[1]]);
    await menu(page, f.roots[2].id);
    const down = page.waitForResponse(
      (r) =>
        r.url().endsWith(`/lists/${f.list.id}/task-order`) &&
        r.request().method() === 'POST',
    );
    await row(page, f.roots[2].id).locator('[data-order-down]').click();
    expect((await down).status()).toBe(200);
    expect(await ids(request, f.list)).toEqual(original);
    await expect(row(page, f.roots[0].id).locator('[data-order-handle]')).toBeHidden();
  });
}
test('desktop native drag handle reorders siblings and nested Up keeps parents separate', async ({
  page,
  request,
}) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  const f = await fixture(request);
  await open(page, f.list);
  await menu(page, f.roots[2].id);
  const handle = row(page, f.roots[2].id).locator('[data-order-handle]');
  const supportsDrag = await page.evaluate(
    () => matchMedia('(min-width: 640px) and (hover: hover) and (pointer: fine)').matches,
  );
  expect(await row(page, f.roots[2].id).getAttribute('draggable')).toBeNull();
  if (supportsDrag) {
    await expect(handle).toBeVisible();
    const moved = page.waitForResponse(
      (r) =>
        r.url().endsWith(`/lists/${f.list.id}/task-order`) &&
        r.request().method() === 'POST',
    );
    await handle.dragTo(row(page, f.roots[0].id), { targetPosition: { x: 20, y: 3 } });
    expect((await moved).status()).toBe(200);
  } else {
    // Headless Firefox reports hover:false and pointer:none. It truthfully gets
    // the accessible button fallback rather than a forced fake capability.
    await expect(handle).toBeHidden();
    for (const expected of [
      [f.roots[0].id, f.roots[2].id, f.roots[1].id],
      [f.roots[2].id, f.roots[0].id, f.roots[1].id],
    ]) {
      if (
        !(await row(page, f.roots[2].id)
          .locator('details')
          .evaluate((d) => d.open))
      )
        await menu(page, f.roots[2].id);
      const moved = page.waitForResponse(
        (r) =>
          r.url().endsWith(`/lists/${f.list.id}/task-order`) &&
          r.request().method() === 'POST',
      );
      await row(page, f.roots[2].id).locator('[data-order-up]').click();
      expect((await moved).status()).toBe(200);
      await expect
        .poll(() =>
          page
            .locator('#task-list .task[data-parent-id=""]')
            .evaluateAll((rows) => rows.map((r) => r.dataset.id)),
        )
        .toEqual(expected);
    }
  }
  expect(await ids(request, f.list)).toEqual([
    f.roots[2].id,
    f.roots[0].id,
    f.roots[1].id,
  ]);
  await expect
    .poll(() =>
      page
        .locator('#task-list .task[data-parent-id=""]')
        .evaluateAll((rows) => rows.map((r) => r.dataset.id)),
    )
    .toEqual([f.roots[2].id, f.roots[0].id, f.roots[1].id]);
  await menu(page, f.children[1].id);
  const nested = page.waitForResponse(
    (r) =>
      r.url().endsWith(`/lists/${f.list.id}/task-order`) &&
      r.request().method() === 'POST',
  );
  await row(page, f.children[1].id).locator('[data-order-up]').click();
  const response = await nested;
  expect(response.status()).toBe(200);
  expect(response.request().postDataJSON().parent_id).toBe(f.roots[0].id);
  expect(response.request().postDataJSON().ids).toEqual([
    f.children[1].id,
    f.children[0].id,
  ]);
  expect(await ids(request, f.list)).toEqual([
    f.roots[2].id,
    f.roots[0].id,
    f.roots[1].id,
  ]);
  await expect
    .poll(() =>
      page
        .locator('#task-list .task')
        .evaluateAll((rows) => rows.map((r) => r.dataset.id)),
    )
    .toEqual([
      f.roots[2].id,
      f.roots[0].id,
      f.children[1].id,
      f.children[0].id,
      f.roots[1].id,
    ]);
});
test('actual concurrent reorder returns409 without replacing a newly opened capture draft', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  await open(page, f.list);
  let notify, release;
  const reached = new Promise((r) => (notify = r)),
    held = new Promise((r) => (release = r));
  await page.route(`**/api/v1/lists/${f.list.id}/task-order`, async (route) => {
    const body = route.request().postDataJSON();
    notify(body);
    await held;
    await route.continue();
  });
  await menu(page, f.roots[2].id);
  await row(page, f.roots[2].id).locator('[data-order-up]').click();
  const body = await reached;
  const external = [f.roots[2].id, f.roots[1].id, f.roots[0].id];
  const concurrent = await request.post(`/api/v1/lists/${f.list.id}/task-order`, {
    headers: auth,
    data: { ...body, ids: external },
  });
  expect(concurrent.status()).toBe(200);
  await page
    .locator('#capture-title')
    .fill('New draft must survive an ordering conflict');
  const conflict = page.waitForResponse(
    (r) => r.url().endsWith(`/lists/${f.list.id}/task-order`) && r.status() === 409,
  );
  release();
  await conflict;
  await expect(page.locator('#capture-title')).toHaveValue(
    'New draft must survive an ordering conflict',
  );
  await expect(page.locator('#refresh-notice')).toBeVisible();
  expect(
    await page
      .locator('#task-list .task[data-parent-id=""]')
      .evaluateAll((rows) => rows.map((r) => r.dataset.id)),
  ).toEqual(f.roots.map((t) => t.id));
  expect(await ids(request, f.list)).toEqual(external);
  await page.locator('#capture-title').fill('');
  await page.locator('#refresh-now').click();
  await expect
    .poll(() =>
      page
        .locator('#task-list .task[data-parent-id=""]')
        .evaluateAll((rows) => rows.map((r) => r.dataset.id)),
    )
    .toEqual(external);
});
test('an existing capture draft blocks ordering without sending a write', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  await open(page, f.list);
  let writes = 0;
  page.on('request', (r) => {
    if (r.method() === 'POST' && r.url().endsWith('/task-order')) writes++;
  });
  await page.locator('#capture-title').fill('Existing capture draft');
  await menu(page, f.roots[2].id);
  await row(page, f.roots[2].id).locator('[data-order-up]').click();
  await expect(page.locator('#capture-title')).toHaveValue('Existing capture draft');
  expect(writes).toBe(0);
  expect(await ids(request, f.list)).toEqual(f.roots.map((t) => t.id));
});
test('real viewer membership has no ordering controls and the server refuses writes', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  const workspace = await (
    await request.get(`/api/v1/workspaces/${f.workspace.id}`, { headers: auth })
  ).json();
  const owner = await post(request, `/workspaces/${f.workspace.id}/members`, {
    revision: workspace.revision,
    user_id: 'other',
    role: 'owner',
    acknowledge_access_change: true,
  });
  await post(request, `/workspaces/${f.workspace.id}/members`, {
    revision: owner.revision,
    user_id: 'dev',
    role: 'viewer',
    acknowledge_access_change: true,
  });
  await open(page, f.list);
  expect(
    await page.locator('[data-order-up],[data-order-down],[data-order-handle]').count(),
  ).toBe(0);
  const info = await (
    await request.get(`/api/v1/lists/${f.list.id}`, { headers: auth })
  ).json();
  const response = await request.post(`/api/v1/lists/${f.list.id}/task-order`, {
    headers: auth,
    data: {
      revision: info.revision,
      parent_id: null,
      ids: f.roots.map((t) => t.id).reverse(),
    },
  });
  expect(response.status()).toBe(403);
  expect(await ids(request, f.list)).toEqual(f.roots.map((t) => t.id));
});
