import { test, expect } from '@playwright/test';
import crypto from 'node:crypto';
const auth = { Authorization: 'Bearer local-integration-token-not-for-production' };
async function post(request, path, data) {
  const response = await request.post(`/api/v1${path}`, {
    headers: { ...auth, 'Idempotency-Key': crypto.randomUUID() },
    data,
  });
  expect(response.ok(), await response.text()).toBe(true);
  return response.json();
}
async function fixture(request) {
  const unique = crypto.randomUUID().slice(0, 8);
  const workspace = await post(request, '/workspaces', { title: `Metadata ${unique}` });
  const list = await post(request, '/lists', {
    workspace_id: workspace.id,
    title: `Metadata ${unique}`,
  });
  const section = await post(request, `/lists/${list.id}/sections`, {
    title: 'Next actions',
  });
  const [parent, task] = await post(request, '/capture', {
    list_id: list.id,
    tasks: [
      { title: `Parent ${unique}`, description: '', parent_id: null },
      {
        title: `Metadata task ${unique}`,
        description: 'Saved **context** <img src=x onerror=alert(1)>',
        parent_id: null,
      },
    ],
  });
  return { workspace, list, section, parent, task, unique };
}
async function open(page, list, task) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto(`/lists/${list.id}?task=${task.id}`);
  await expect(page.locator('#task-editor')).toBeVisible();
  await page.locator('[data-task-metadata] > details > summary').click();
  await expect(page.locator('[data-tags-form] [name=new_tags]')).toBeEnabled();
  return page.locator('[data-task-metadata]');
}
for (const [name, width, height] of [
  ['phone', 390, 844],
  ['desktop', 1440, 1000],
]) {
  test(`${name}: stable tag IDs, independent pending metadata, section and safe parenting`, async ({
    page,
    request,
  }) => {
    await page.setViewportSize({ width, height });
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    const f = await fixture(request);
    let response = await request.put(`/api/v1/tasks/${f.task.id}/tags`, {
      headers: auth,
      data: { revision: f.task.revision, tags: ['Old Name'] },
    });
    expect(response.ok()).toBe(true);
    f.task = await response.json();
    const [oldTag] = await (
      await request.get(`/api/v1/tasks/${f.task.id}/tags`, { headers: auth })
    ).json();
    let root = await open(page, f.list, f.task);
    await expect(root.locator('[data-description-preview] strong')).toHaveText('context');
    await expect(
      root.locator('[data-description-preview] img, [data-description-preview] script'),
    ).toHaveCount(0);
    await expect(root.locator(`[data-tag-id="${oldTag.id}"]`)).toHaveText('+Old Name');
    await root.locator('[name=section_id]').selectOption(f.section.id);
    await root.locator('[name=parent_search]').fill(f.parent.title);
    await root.locator(`[data-parent-choice="${f.parent.id}"]`).click();
    await expect(root.locator('[data-parent-selection]')).toHaveText(f.parent.title);
    await root.locator('[name=new_tags]').fill('+home +urgent');
    response = await request.patch(`/api/v1/tags/${oldTag.id}`, {
      headers: auth,
      data: { revision: oldTag.revision, name: 'Renamed tag' },
    });
    expect(response.ok()).toBe(true);
    const saved = page.waitForResponse(
      (r) =>
        r.url().endsWith(`/tasks/${f.task.id}/tags`) && r.request().method() === 'PUT',
    );
    await root.getByRole('button', { name: 'Save tags', exact: true }).click();
    const saveResponse = await saved;
    expect(saveResponse.status()).toBe(200);
    expect(saveResponse.request().postDataJSON().tags).toContainEqual({ id: oldTag.id });
    await expect(root.locator(`[data-tag-id="${oldTag.id}"]`)).toHaveText('+Renamed tag');
    await page.keyboard.press('Escape');
    await expect(page.locator('#task-editor')).not.toBeVisible();
    await page.goto(`/lists/${f.list.id}?task=${f.task.id}`);
    root = page.locator('[data-task-metadata]');
    await expect(root.locator('[name=section_id]')).toHaveValue(f.section.id);
    await expect(root.locator('[name=parent_id]')).toHaveValue(f.parent.id);
    await expect(root.locator('[name=new_tags]')).toHaveValue('');
    await root.getByRole('button', { name: 'Save section', exact: true }).click();
    await expect(root.locator('[data-metadata-notice]')).toContainText('Section saved.');
    await root.getByRole('button', { name: 'Change parent', exact: true }).click();
    await expect(root.locator('[data-metadata-notice]')).toHaveText('Parent updated.');
    const [task] = await (
      await request.get(`/api/v1/tasks/${f.task.id}/subtree`, { headers: auth })
    ).json();
    expect(task.parent_id).toBe(f.parent.id);
    await expect(
      root.getByRole('button', { name: 'Save section', exact: true }),
    ).toBeHidden();
    await root.getByText('Advanced: exact task ID', { exact: true }).click();
    await root.locator('[name=parent_id]').fill(f.task.id);
    await root.getByRole('button', { name: 'Change parent', exact: true }).click();
    await expect(root.locator('[data-metadata-notice]')).toContainText('cycle');
    await expect(root.locator('[name=parent_id]')).toHaveValue(f.task.id);
    await root.getByRole('button', { name: 'Top-level task', exact: true }).click();
    await expect(root.locator('[data-parent-selection]')).toHaveText('Top-level task');
    await root.getByRole('button', { name: 'Change parent', exact: true }).click();
    await expect(root.locator('[data-metadata-notice]')).toHaveText('Parent updated.');
    expect(
      (
        await (
          await request.get(`/api/v1/tasks/${f.task.id}/subtree`, { headers: auth })
        ).json()
      )[0].parent_id,
    ).toBeNull();
    expect(
      await page.locator('#task-editor').evaluate((d) => d.scrollWidth <= d.clientWidth),
    ).toBe(true);
    for (const control of await root
      .locator('button:visible,select:visible,input:visible')
      .all()) {
      expect(Math.round((await control.boundingBox()).height)).toBeGreaterThanOrEqual(44);
    }
    if (name === 'phone')
      await page.screenshot({
        path: `.local/rebuild/metadata-phone-${test.info().project.name}.png`,
      });
    expect(errors).toEqual([]);
  });
}
test('metadata draft survives close and stale task write without changing tags', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  let root = await open(page, f.list, f.task);
  await root.locator('[name=new_tags]').fill('+preserved');
  await page.keyboard.press('Escape');
  await page.goto(`/lists/${f.list.id}?task=${f.task.id}`);
  root = page.locator('[data-task-metadata]');
  await expect(root.locator('[name=new_tags]')).toHaveValue('+preserved');
  const response = await request.patch(`/api/v1/tasks/${f.task.id}`, {
    headers: auth,
    data: { revision: f.task.revision, title: 'Concurrent metadata edit' },
  });
  expect(response.ok()).toBe(true);
  await root.getByRole('button', { name: 'Save tags', exact: true }).click();
  await expect(root.locator('[data-metadata-notice]')).toContainText(
    /draft is preserved|draft and original revision/,
  );
  await expect(root.locator('[name=new_tags]')).toHaveValue('+preserved');
  expect(
    await (
      await request.get(`/api/v1/tasks/${f.task.id}/tags`, { headers: auth })
    ).json(),
  ).toEqual([]);
});
test('reference autocomplete, saved ambiguous binding guard and authorized backlinks', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  const duplicate = `Duplicate${f.unique}`;
  const first = await post(request, '/lists', {
    workspace_id: f.workspace.id,
    title: duplicate,
  });
  const second = await post(request, '/lists', {
    workspace_id: f.workspace.id,
    title: duplicate,
  });
  const [source] = await post(request, '/capture', {
    list_id: f.list.id,
    tasks: [
      {
        title: 'Reference source',
        description: `Ambiguous #${duplicate} and target %${f.task.reference}`,
        parent_id: null,
      },
    ],
  });
  let root = await open(page, f.list, source);
  await root.locator('[data-reference-details] summary').click();
  await expect(root.locator('[data-unresolved-references]')).toContainText(
    `#${duplicate}`,
  );
  await root.locator('[name=reference_query]').fill(`%${f.task.reference}`);
  await root
    .locator('[data-reference-choices]')
    .getByRole('button', { name: new RegExp(f.task.title) })
    .click();
  await expect(root.locator('[name=selected_reference]')).toHaveValue(`%${f.task.id}`);
  await page.locator('#edit-description').fill('Unsaved source text must not be bound');
  const bind = root
    .locator('[data-unresolved-references]')
    .getByRole('button', { name: new RegExp(`Bind ${duplicate}`) })
    .first();
  await bind.click();
  await expect(root.locator('[data-metadata-notice]')).toContainText(
    'Save or discard the task brief draft',
  );
  await page.locator('#edit-description').fill(source.description);
  const saved = page.waitForResponse(
    (r) =>
      r.url().endsWith(`/references/task/${source.id}/bind`) &&
      r.request().method() === 'POST',
  );
  await bind.click();
  const response = await saved;
  expect(response.status()).toBe(200);
  const bound = response.request().postDataJSON();
  expect(bound.target_kind).toBe('list');
  expect([first.id, second.id]).toContain(bound.target_id);
  await expect(root.locator('[data-unresolved-references]')).toHaveText(
    'No unresolved references.',
  );
  await page.goto(`/lists/${f.list.id}?task=${f.task.id}`);
  root = page.locator('[data-task-metadata]');
  await root.locator(':scope > details > summary').click();
  await root.locator('[data-reference-details] summary').click();
  await expect(
    root.locator(`[data-backlinks] a[href="/tasks/${source.id}"]`),
  ).toBeVisible();
});
test('native favorites and filtered search open the actual canonical task', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/favorites');
  await page.getByText('Add a favorite', { exact: true }).click();
  await page.locator('#favorite-list').selectOption(f.list.id);
  await page.getByRole('button', { name: 'Favorite list', exact: true }).click();
  await expect(
    page.locator(`[aria-label="Current favorites"] a[href="/lists/${f.list.id}"]`),
  ).toBeVisible();
  await page.goto(`/lists/${f.list.id}`);
  await expect(
    page.locator(`nav[aria-label="Favorites"] a[href="/lists/${f.list.id}"]`),
  ).toBeVisible();
  await page.goto('/search');
  await page.locator('#query').fill('Metadata');
  await page.getByText('Filters', { exact: true }).click();
  await page.locator('select[name=list]').selectOption(f.list.id);
  await page.getByRole('button', { name: 'Search', exact: true }).click();
  const link = page.locator(
    `[aria-label="Search results"] a[href="/tasks/${f.task.id}"]`,
  );
  await expect(link).toHaveText(f.task.title);
  await link.click();
  await expect(page.locator('#task-editor')).toBeVisible();
  await expect(page.locator('#edit-title')).toHaveValue(f.task.title);
});
test('parent search is bounded to the current list and shows duplicate-title context', async ({
  page,
  request,
}) => {
  const f = await fixture(request);
  const [duplicate] = await post(request, '/capture', {
    list_id: f.list.id,
    tasks: [{ title: f.parent.title, description: '', parent_id: f.parent.id }],
  });
  const elsewhere = await post(request, '/lists', {
    workspace_id: f.workspace.id,
    title: 'Elsewhere',
  });
  const [outside] = await post(request, '/capture', {
    list_id: elsewhere.id,
    tasks: [{ title: f.parent.title, description: '', parent_id: null }],
  });
  const root = await open(page, f.list, f.task);
  const searched = page.waitForResponse(
    (r) => r.url().endsWith('/api/v1/query') && r.request().method() === 'POST',
  );
  await root.locator('[name=parent_search]').fill(f.parent.title);
  const response = await searched;
  expect(response.request().postDataJSON()).toMatchObject({
    list_id: f.list.id,
    entities: ['task'],
    limit: 20,
  });
  const entered = page.waitForResponse(
    (r) => r.url().endsWith('/api/v1/query') && r.request().method() === 'POST',
  );
  await root.locator('[name=parent_search]').press('Enter');
  await entered;
  await expect(root.locator('[data-metadata-notice]')).not.toContainText(
    'Parent updated.',
  );
  await expect(root.locator('[data-parent-choice]')).toHaveCount(2);
  await expect(root.locator(`[data-parent-choice="${outside.id}"]`)).toHaveCount(0);
  const first = await root.locator(`[data-parent-choice="${f.parent.id}"]`).textContent();
  const nested = await root
    .locator(`[data-parent-choice="${duplicate.id}"]`)
    .textContent();
  expect(first).not.toBe(nested);
  await root.locator(`[data-parent-choice="${duplicate.id}"]`).click();
  await expect(root.locator('[name=parent_id]')).toHaveValue(duplicate.id);
  await expect(root.locator('[data-parent-selection]')).toHaveText(f.parent.title);
});
