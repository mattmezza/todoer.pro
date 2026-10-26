import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

async function setup(page) {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.goto('/lists/inbox');
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  const api = async (path, method = 'GET', data, extra = {}) => {
    const response = await page.request.fetch(`/api/v1${path}`, {
      method,
      data,
      headers: { 'x-csrf-token': csrf, ...extra },
    });
    expect(response.ok(), `${method} ${path}: ${response.status()}`).toBe(true);
    return response.json();
  };
  const capture = async (title, description = '', parent = null, list = 'inbox') =>
    (
      await api(
        '/capture',
        'POST',
        { list_id: list, tasks: [{ title, description, parent_id: parent }] },
        { 'Idempotency-Key': `task-page-${randomUUID()}` },
      )
    )[0];
  const current = async (id) =>
    (
      await api(`/tasks/${id}/subtree/query`, 'POST', {
        limit: 1,
        fields: ['title', 'description', 'revision', 'complete', 'list_id'],
      })
    ).items[0];
  return { api, capture, current };
}
async function ready(page, id) {
  await page.goto(`/tasks/${id}`);
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  await expect(page.locator('[data-task-page]')).toBeVisible();
  await expect(page.locator('#task-editor')).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`/tasks/${id}$`));
}

test('dedicated task preserves drafts across reload, remote CAS conflict and explicit rebase', async ({
  page,
}) => {
  const { capture, api, current } = await setup(page);
  const task = await capture(
    `Task page ${randomUUID().slice(0, 8)}`,
    '**Saved description**',
  );
  await ready(page, task.id);
  await expect(page.locator('[data-saved-description] strong')).toHaveText(
    'Saved description',
  );
  await page.locator('[data-edit-title]').click();
  await page.getByLabel('Task title', { exact: true }).fill('My preserved local title');
  await page
    .getByLabel('Description, Markdown supported', { exact: true })
    .fill('My **local draft** stays intact.');
  await page.getByLabel('Task title', { exact: true }).press('Escape');
  await expect(page.locator('[data-brief-form]')).toBeHidden();
  await page.reload();
  await expect(page.getByLabel('Task title', { exact: true })).toHaveValue(
    'My preserved local title',
  );
  const before = await current(task.id);
  await api(`/tasks/${task.id}`, 'PATCH', {
    revision: before.revision,
    title: 'Remote saved title',
    description: 'Remote saved description',
  });
  await expect(page.locator('[data-edit-title]')).toHaveText('Remote saved title');
  await expect(
    page.getByLabel('Description, Markdown supported', { exact: true }),
  ).toHaveValue('My **local draft** stays intact.');
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  await expect(page.locator('[data-conflict]')).not.toBeEmpty();
  expect((await current(task.id)).description).toBe('Remote saved description');
  page.on('dialog', (dialog) => dialog.accept());
  await page
    .getByRole('button', { name: 'Use latest revision with my draft', exact: true })
    .click();
  await page.getByLabel('Task title', { exact: true }).press('Control+Enter');
  await expect(page.locator('[data-edit-title]')).toHaveText('My preserved local title');
  await expect(page.locator('[data-brief-form]')).toBeHidden();
  await expect(page.locator('[data-saved-description] strong')).toHaveText('local draft');
  expect((await current(task.id)).description).toBe('My **local draft** stays intact.');
});

test('task page shares metadata planning and discussion while keeping the brief draft', async ({
  page,
}) => {
  const { capture, current, api } = await setup(page);
  const task = await capture(
    `Task modules ${randomUUID().slice(0, 8)}`,
    'Original context',
  );
  await ready(page, task.id);
  await page.locator('[data-edit-title]').click();
  await page
    .getByLabel('Task title', { exact: true })
    .fill('Brief draft across module saves');
  await page.locator('[data-task-advanced] > summary').click();
  await page.getByText('Planning and recurrence', { exact: true }).click();
  const planned = page.getByLabel('Planned date', { exact: true });
  await expect(planned).toBeEnabled();
  await planned.fill('2026-11-02');
  await page.getByRole('button', { name: 'Save schedule', exact: true }).click();
  await expect(page.getByLabel('Task title', { exact: true })).toHaveValue(
    'Brief draft across module saves',
  );
  await page.getByText('Tags, structure and references', { exact: true }).click();
  const tags = page.getByLabel('Add tags (space-separated +names)', { exact: true });
  await expect(tags).toBeEnabled();
  const tagName = `page-${randomUUID().slice(0, 8)}`;
  await tags.fill(`+${tagName}`);
  await page.getByRole('button', { name: 'Save tags', exact: true }).click();
  await expect
    .poll(async () =>
      (await api(`/tasks/${task.id}/tags`)).some((tag) => tag.name === tagName),
    )
    .toBe(true);
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  await expect(page.locator('[data-edit-title]')).toHaveText(
    'Brief draft across module saves',
  );
  expect((await current(task.id)).title).toBe('Brief draft across module saves');
  await page.locator('[data-comment-composer]>summary').click();
  const comment = page.getByLabel('Add a comment', { exact: true });
  await comment.fill('Dedicated task comment with **context**');
  await comment.press('Control+Enter');
  await expect(page.locator('[data-comment-rendered] strong')).toHaveText('context');
  await page.getByText('Add subtask', { exact: true }).click();
  const childForm = page.locator('[data-child-form]');
  await childForm
    .getByLabel('New checklist item', { exact: true })
    .fill('Linked child task');
  await childForm.getByText('Add a description', { exact: true }).click();
  await childForm
    .getByLabel('Checklist item description', { exact: true })
    .fill('Child original description');
  await childForm
    .getByLabel('Checklist item description', { exact: true })
    .press('Control+Enter');
  const link = page
    .locator('[data-children]')
    .getByRole('link', { name: 'Linked child task', exact: true });
  await expect(link).toBeVisible();
  const path = await link.getAttribute('href');
  await link.click();
  await expect(page).toHaveURL(new RegExp(`${path}$`));
  await expect(page.locator('[data-saved-description]')).toHaveText(
    'Child original description',
  );
});

test('whole checklist move requires audience acknowledgement and atomically assigns destination section', async ({
  page,
}) => {
  const { api, capture, current } = await setup(page);
  const source = await api('/workspaces', 'POST', {
    title: `Page source ${randomUUID().slice(0, 8)}`,
  });
  const destination = await api('/workspaces', 'POST', {
    title: `Page destination ${randomUUID().slice(0, 8)}`,
  });
  await api(`/workspaces/${destination.id}/members`, 'POST', {
    user_id: 'other',
    role: 'editor',
    revision: destination.revision,
    acknowledge_access_change: true,
  });
  const sourceList = await api('/lists', 'POST', {
    workspace_id: source.id,
    title: 'Page source list',
    restricted: false,
  });
  const destinationList = await api('/lists', 'POST', {
    workspace_id: destination.id,
    title: 'Page destination list',
    restricted: false,
  });
  const section = await api(`/lists/${destinationList.id}/sections`, 'POST', {
    title: 'Destination section',
  });
  const task = await capture('Move page root', 'Preserve root', null, sourceList.id);
  const child = await capture(
    'Move page child',
    'Preserve child',
    task.id,
    sourceList.id,
  );
  await ready(page, task.id);
  await page.locator('.task-context-menu summary').click();
  await page
    .getByRole('button', { name: 'Move task and checklist', exact: true })
    .click();
  await page
    .getByLabel('Destination list', { exact: true })
    .selectOption(destinationList.id);
  await expect(page.locator('[data-move-audience]')).toContainText('adds access');
  await page.getByLabel('Destination section', { exact: true }).selectOption(section.id);
  await page.getByRole('button', { name: 'Review move', exact: true }).click();
  await expect(page.locator('[data-move-error]')).toContainText('acknowledge');
  expect((await current(task.id)).list_id).toBe(sourceList.id);
  await page.locator('[data-move-ack]').check();
  await page.getByRole('button', { name: 'Review move', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Apply reviewed move', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Apply reviewed move', exact: true }).click();
  await expect(page.locator('[data-connection]')).toHaveText('Up to date');
  await expect
    .poll(async () => (await current(task.id)).list_id)
    .toBe(destinationList.id);
  expect((await current(child.id)).list_id).toBe(destinationList.id);
  expect((await api(`/tasks/${task.id}/section`)).section_id).toBe(section.id);
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}$`));
});

test('mobile task page has a private read-only guest view and working shared navigation', async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 800 });
  const { api, capture } = await setup(page);
  const hiddenTitle = `Hidden workspace ${randomUUID().slice(0, 8)}`;
  const workspace = await api('/workspaces', 'POST', { title: hiddenTitle });
  const list = await api('/lists', 'POST', {
    workspace_id: workspace.id,
    title: 'Read-only task list',
    restricted: true,
  });
  const task = await capture(
    'Read-only dedicated task',
    'Visible task context',
    null,
    list.id,
  );
  await api(`/lists/${list.id}/grants`, 'POST', {
    user_id: 'dev',
    role: 'viewer',
    revision: list.revision,
    acknowledge_access_change: true,
  });
  const latest = await api(`/workspaces/${workspace.id}`);
  await api(`/workspaces/${workspace.id}/members`, 'POST', {
    user_id: 'other',
    role: 'owner',
    revision: latest.revision,
    acknowledge_access_change: true,
  });
  const ownerState = await api(`/workspaces/${workspace.id}`);
  await api(`/workspaces/${workspace.id}/members/revoke`, 'POST', {
    user_id: 'dev',
    revision: ownerState.revision,
  });
  await ready(page, task.id);
  await expect(page.locator('[data-root-complete]')).toBeDisabled();
  await expect(page.locator('[data-edit-title]')).toBeDisabled();
  await expect(page.locator('body')).not.toContainText(hiddenTitle);
  await expect(page.getByText('Read-only discussion.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Toggle navigation', exact: true }).click();
  await expect(page.locator('#navigation')).toHaveClass(/open/);
  await page.keyboard.press('Escape');
  await expect(page.locator('#navigation')).not.toHaveClass(/open/);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
});

test('task access revocation clears saved content while keeping account-scoped local draft', async ({
  page,
}) => {
  const { api, capture } = await setup(page);
  const workspace = await api('/workspaces', 'POST', {
    title: `Revocation ${randomUUID().slice(0, 8)}`,
  });
  const list = await api('/lists', 'POST', {
    workspace_id: workspace.id,
    title: 'Revocable task list',
    restricted: false,
  });
  const task = await capture(
    'Revocable saved title',
    'Revocable saved description',
    null,
    list.id,
  );
  const ownerState = await api(`/workspaces/${workspace.id}`);
  await api(`/workspaces/${workspace.id}/members`, 'POST', {
    user_id: 'other',
    role: 'owner',
    revision: ownerState.revision,
    acknowledge_access_change: true,
  });
  await ready(page, task.id);
  await page.locator('[data-edit-title]').click();
  await page
    .getByLabel('Task title', { exact: true })
    .fill('Private local draft survives revocation');
  const latest = await api(`/workspaces/${workspace.id}`);
  await api(`/workspaces/${workspace.id}/members/revoke`, 'POST', {
    user_id: 'dev',
    revision: latest.revision,
  });
  await expect(
    page.getByRole('heading', { name: 'Task unavailable', exact: true }),
  ).toBeVisible();
  await expect(page.locator('body')).not.toContainText('Revocable saved description');
  await expect(page.locator('body')).not.toContainText('Revocable saved title');
  const saved = await page.evaluate(
    (id) => JSON.parse(sessionStorage.getItem(`todoer:dev:edit:${id}`)),
    task.id,
  );
  expect(saved.title).toBe('Private local draft survives revocation');
  await page.keyboard.press('n'); // Revoked-page shortcuts do not revive hidden editors.
  await expect(
    page.getByRole('heading', { name: 'Task unavailable', exact: true }),
  ).toBeVisible();
});

test('bounded checklist continuation retains brief and subtask drafts and reports leaf progress', async ({
  page,
}) => {
  const { api, capture } = await setup(page);
  const workspace = await api('/workspaces', 'POST', {
    title: `Pagination ${randomUUID().slice(0, 8)}`,
  });
  const list = await api('/lists', 'POST', {
    workspace_id: workspace.id,
    title: 'Bounded task list',
    restricted: true,
  });
  const task = await capture('Bounded task page', 'Preserved root brief', null, list.id);
  await api(
    '/capture',
    'POST',
    {
      list_id: list.id,
      tasks: Array.from({ length: 100 }, (_, i) => ({
        title: `Checklist item ${i + 1}`,
        description: '',
        parent_id: task.id,
      })),
    },
    { 'Idempotency-Key': `task-page-many-${randomUUID()}` },
  );
  await ready(page, task.id);
  await page.getByText('Add subtask', { exact: true }).click();
  await page
    .getByLabel('New checklist item', { exact: true })
    .fill('Unsubmitted child draft');
  await page.getByLabel('New checklist item', { exact: true }).press('Escape');
  await expect(page.locator('[data-child-composer]')).not.toHaveAttribute('open', '');
  await page.locator('[data-edit-title]').click();
  await page.getByLabel('Task title', { exact: true }).fill('Unsubmitted root draft');
  await expect(page.locator('[data-child-progress]')).toContainText('loaded items');
  await page
    .getByRole('button', { name: 'Load more checklist items', exact: true })
    .click();
  await expect(page.locator('[data-children] > li')).toHaveCount(100);
  await expect(page.locator('[data-child-progress]')).toHaveText(
    '0/100 leaf items complete',
  );
  await expect(page.getByLabel('Task title', { exact: true })).toHaveValue(
    'Unsubmitted root draft',
  );
  await page.reload();
  await expect(page.getByLabel('Task title', { exact: true })).toHaveValue(
    'Unsubmitted root draft',
  );
  await expect(page.getByLabel('New checklist item', { exact: true })).toHaveValue(
    'Unsubmitted child draft',
  );
  await expect(page.locator('[data-child-composer]')).toHaveAttribute('open', '');
});

test('task-page logout revokes the session and does not recreate cleared drafts on pagehide', async ({
  page,
}) => {
  const { capture } = await setup(page);
  const task = await capture('Logout task fixture', 'Preserved until explicit logout');
  await ready(page, task.id);
  await page.locator('[data-edit-title]').click();
  await page
    .getByLabel('Task title', { exact: true })
    .fill('Draft discarded on explicit logout');
  await page.locator('.account-menu summary').click();
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await expect(page).toHaveURL(/\/signin/);
  expect(
    await page.evaluate(() =>
      Object.keys(sessionStorage).filter((key) => key.startsWith('todoer:')),
    ),
  ).toEqual([]);
  await page.goto(`/tasks/${task.id}`);
  await expect(page).toHaveURL(/\/signin/);
  await expect(page.locator('body')).not.toContainText('Logout task fixture');
});

test('task composers and contextual actions retain the reference layout', async ({
  page,
}) => {
  const { capture, api } = await setup(page);
  const task = await capture(`Composer layout ${randomUUID().slice(0, 8)}`);
  await ready(page, task.id);
  await expect(page.getByText('No checklist items yet')).toHaveCount(0);
  await page.locator('[data-child-composer] > summary').click();
  await expect(page.locator('[data-child-composer] > summary')).toBeHidden();
  await page.locator('[data-child-cancel]').click();
  await expect(page.locator('[data-child-composer] > summary')).toBeVisible();
  await page.locator('[data-comment-composer] > summary').click();
  const comment = page.locator('[data-new-comment-input]');
  const initial = await comment.evaluate((el) => el.clientHeight);
  await comment.fill(Array(20).fill('A longer comment line').join('\n'));
  expect(await comment.evaluate((el) => el.clientHeight)).toBeGreaterThan(initial);
  await comment.press('Escape');
  await expect(page.locator('[data-comment-composer] > summary')).toBeVisible();
  await page.locator('.task-context-menu > summary').click();
  await expect(page.locator('[data-context-edit]')).toHaveCSS('display', 'flex');
  await page.locator('[data-open-move]').click();
  await expect(page.locator('[data-move-dialog]')).toBeVisible();
  expect(
    await page
      .locator('#task-move-heading')
      .evaluate((el) => getComputedStyle(el).fontFamily),
  ).not.toContain('monospace');
  await page.getByRole('button', { name: 'Close move dialog' }).click();
  const workspace = await api('/workspaces', 'POST', {
    title: `Modal ${randomUUID().slice(0, 8)}`,
  });
  const list = await api('/lists', 'POST', {
    workspace_id: workspace.id,
    title: 'Modal list',
  });
  await page.goto(`/lists/${list.id}`);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  await page.locator('[data-list-create-open]').first().click();
  const create = page.locator('#list-create-dialog');
  await expect(create).toBeVisible();
  await expect(create.locator('select')).toHaveCount(0);
  await expect(create.locator('[name=workspace_id]')).not.toHaveValue('');
  await expect(create.locator('.inline-actions')).toHaveCSS('border-top-width', '0px');
  await expect(create.locator('[name=title]')).toHaveCSS('font-size', '16px');
});
