import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

for (const width of [390, 1280]) {
  test(`entity editors share compact fields and primary actions at ${width}px`, async ({
    page,
  }) => {
    test.setTimeout(45000);
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
    const space = await post('workspaces', { title: `Forms ${randomUUID()}` });
    const list = await post('lists', { workspace_id: space.id, title: 'Forms' });
    const [task] = await post('capture', {
      list_id: list.id,
      tasks: [{ title: 'Form parent', description: 'One-line brief' }],
    });
    const [child] = await post('capture', {
      list_id: list.id,
      tasks: [{ title: 'Form child', description: 'Child brief', parent_id: task.id }],
    });
    const comment = await post(`tasks/${task.id}/comments`, { body: 'Existing comment' });
    const section = await post(`lists/${list.id}/sections`, {
      title: 'Existing section',
    });
    const check = async (form, { cancel = 'Cancel' } = {}) => {
      await expect(form).toBeVisible();
      const fields = form
        .locator('input:not([type]), input[type=text], textarea')
        .filter({ visible: true });
      expect(await fields.count()).toBeGreaterThan(0);
      for (const field of await fields.all()) {
        await expect(field).toHaveCSS('border-radius', '6px');
        await expect(field).toHaveCSS('border-top-style', 'solid');
        await expect(field).toHaveCSS('border-top-width', '1px');
        await expect(field).toHaveCSS('font-size', width === 390 ? '16px' : '15px');
        const box = await field.boundingBox();
        expect(box.height).toBeGreaterThanOrEqual(38);
        expect(box.x + box.width).toBeLessThanOrEqual(width + 1);
      }
      const save = form
        .locator('button.primary[type=submit],button.primary:not([type])')
        .first();
      const secondary = form.getByRole('button', { name: cancel, exact: true });
      await expect(save).toBeVisible();
      await expect(secondary).toBeVisible();
      await expect(save).toHaveCSS('background-color', 'rgb(79, 70, 229)');
      expect(
        await secondary.evaluate((el) => getComputedStyle(el).backgroundColor),
      ).not.toBe('rgb(79, 70, 229)');
      expect((await save.boundingBox()).y).toBeGreaterThanOrEqual(
        (await fields.last().boundingBox()).y +
          (await fields.last().boundingBox()).height,
      );
      expect((await save.boundingBox()).x).toBeLessThan(
        (await secondary.boundingBox()).x,
      );
    };
    await page.goto(`/lists/${list.id}`);
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    const row = page.locator(`.task[data-id="${task.id}"]`);
    await row.locator('.task-title').click({ button: 'right' });
    await check(row.locator('form'));
    await row.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.locator('#capture-open').click();
    await check(page.locator('#capture-form'));
    await page.locator('#capture-cancel').click();
    await page.locator('[data-section-add-open]').click();
    await check(page.locator('[data-section-create]'));
    await page.locator('[data-section-add-cancel]').click();
    await page
      .locator(`[data-section-id="${section.id}"] h2 [data-section-edit-open]`)
      .click();
    const sectionDialog = page.getByRole('dialog', { name: 'Edit section', exact: true });
    await check(sectionDialog.locator('form'));
    await sectionDialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.locator('[data-list-display] h1 button').click();
    await check(page.locator('[data-list-edit]'));
    await page.locator('[data-list-edit-cancel]').click();
    if (width === 390)
      await page.getByRole('button', { name: 'Toggle navigation' }).click();
    await page
      .locator('[data-list-create-open]')
      .filter({ visible: true })
      .first()
      .click();
    const listDialog = page.locator('#list-create-dialog');
    await check(listDialog.locator('form'));
    await listDialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.goto(`/space/${space.id}`);
    await page.locator('[data-space-edit="description"]').click();
    await check(page.locator('[data-space-edit-form]'));
    await page.locator('[data-space-edit-cancel]').click();
    await page.goto(`/tasks/${task.id}`);
    const exerciseTask = async (surface) => {
      await surface.locator('[data-edit-title]').click();
      await check(surface.locator('[data-brief-form]'));
      await surface.locator('[data-cancel-edit]').click();
      const childRow = surface.locator(`[data-child-id="${child.id}"]`);
      await childRow
        .locator(':scope > .task-page-child-line > a')
        .click({ button: 'right' });
      await check(childRow.locator('form'));
      await childRow.getByRole('button', { name: 'Cancel', exact: true }).click();
      await surface.locator('[data-child-composer] > summary').click();
      await check(surface.locator('[data-child-form]'));
      await surface.locator('[data-child-cancel]').click();
      await surface.locator('[data-comment-composer] > summary').click();
      await check(surface.locator('[data-new-comment]'));
      await surface.locator('[data-comment-composer-close]').click();
      const commentRow = surface.locator(`[data-comment-id="${comment.id}"]`);
      await commentRow.locator('.comment-actions > summary').click();
      await commentRow.locator('[data-comment-edit]').click();
      await check(commentRow.locator('[data-comment-form]'));
      await commentRow.locator('[data-comment-cancel]').click();
    };
    await exerciseTask(page.locator('main[data-task-page]'));
    await page.goto(`/lists/${list.id}`);
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    await row.locator('.task-description').click();
    const panel = page.getByRole('complementary', { name: 'Task details', exact: true });
    await expect(panel.locator('.planning-detail-content')).not.toHaveAttribute(
      'aria-busy',
      'true',
    );
    await exerciseTask(panel);
    await page.getByRole('button', { name: 'Close task details', exact: true }).click();
    await expect(panel).toHaveCount(0);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
  });
}
