import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

for (const mode of ['drop', 'paste', 'comment-paste', 'existing-comment-paste']) {
  test(`${mode} uploads a file and preserves the comment draft`, async ({ page }) => {
    let finalized = false;
    let uploaded = false;
    await page.route('**/api/v1/tasks/*/uploads', async (route) => {
      const upload = route.request().postDataJSON();
      if (mode.includes('comment-paste')) expect(upload.comment_id).toBeTruthy();
      else expect(upload.comment_id).toBeNull();
      expect(upload).toMatchObject({
        filename: 'notes.txt',
        size: 5,
      });
      await route.fulfill({ json: { intent: { id: 'test-upload', revision: 1 } } });
    });
    await page.route('**/api/v1/uploads/test-upload/content?*', async (route) => {
      expect(route.request().postDataBuffer().toString()).toBe('hello');
      uploaded = true;
      await route.fulfill({ json: { uploaded: true } });
    });
    await page.route('**/api/v1/uploads/test-upload/finalize', async (route) => {
      finalized = true;
      await route.fulfill({ json: { ok: true } });
    });
    await page.route('**/fragments/tasks/*/discussion', async (route) => {
      const response = await route.fetch();
      let html = await response.text();
      if (!html.includes('data-file-form'))
        html = html.replace(
          '</section><h3',
          '<form data-file-form class="attachment-form"><input type="file" data-file-input hidden><button type="button" data-file-choose>Attach file</button><p data-file-selection hidden></p><progress data-upload-progress hidden></progress><button data-file-upload hidden></button><button data-file-abandon hidden></button></form></section><h3',
        );
      if (finalized)
        html = html.replace(
          '<div class="attachment-grid">',
          '<div class="attachment-grid"><div data-file-row class="attachment-card"><a class="attachment-card-main" href="/test"><span class="attachment-type">TXT</span><span class="attachment-card-info"><strong>notes.txt</strong><span>TXT · 5 B · Download</span></span></a><div class="attachment-card-actions"><a href="/test" aria-label="Download notes.txt">Download</a><button type="button" data-file-delete="test" aria-label="Remove notes.txt">Remove</button></div></div>',
        );
      await route.fulfill({ response, body: html });
    });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/inbox');
    const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
    const inbox = await (await page.request.get('/api/v1/inbox')).json();
    const title = `File drop ${randomUUID()}`;
    const created = await page.request.post('/api/v1/capture', {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data: { list_id: inbox.list_id, tasks: [{ title }] },
    });
    expect(created.ok()).toBeTruthy();
    if (mode === 'existing-comment-paste') {
      const [task] = await created.json();
      const comment = await page.request.post(`/api/v1/tasks/${task.id}/comments`, {
        headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
        data: { body: 'Existing comment' },
      });
      expect(comment.ok()).toBeTruthy();
    }
    await page.reload();
    await page
      .locator('.inbox-task')
      .filter({ hasText: title })
      .locator('.inbox-location')
      .click();
    const panel = page.getByLabel('Task details', { exact: true });
    await expect(panel.locator('[data-file-choose]')).toBeVisible();
    await panel.locator('[data-comment-composer] > summary').click();
    await panel.getByLabel('Add a comment').fill('Draft stays here');
    if (mode === 'existing-comment-paste') {
      await panel.locator('.comment-actions > summary').click();
      await panel.getByRole('button', { name: 'Edit comment', exact: true }).click();
      await panel
        .getByLabel('Edit comment', { exact: true })
        .fill('Unsaved existing comment edit');
    }
    const data = await page.evaluateHandle(() => {
      const transfer = new DataTransfer();
      transfer.items.add(new File(['hello'], 'notes.txt', { type: 'text/plain' }));
      return transfer;
    });
    if (mode === 'drop') {
      await page.locator('h1').first().dispatchEvent('dragenter', { dataTransfer: data });
      await expect(page.locator('.attachment-drop-zone-panel')).toBeHidden();
      await panel.dispatchEvent('dragenter', { dataTransfer: data });
      await expect(page.locator('.attachment-drop-zone-panel')).toBeVisible();
      await panel.dispatchEvent('dragover', { dataTransfer: data });
      await panel.dispatchEvent('drop', { dataTransfer: data });
      await expect(page.locator('.attachment-drop-zone-panel')).toBeHidden();
    } else {
      const target =
        mode === 'existing-comment-paste'
          ? panel.getByLabel('Edit comment', { exact: true })
          : mode === 'comment-paste'
            ? panel.getByLabel('Add a comment')
            : panel;
      await target.evaluate((element, clipboardData) => {
        element.dispatchEvent(
          new ClipboardEvent('paste', { clipboardData, bubbles: true, cancelable: true }),
        );
      }, data);
      if (mode === 'comment-paste') {
        await expect(panel.locator('[data-file-selection]')).toContainText(
          'attached when you post',
        );
        expect(uploaded).toBe(false);
        await panel.getByRole('button', { name: 'Post', exact: true }).click();
      }
    }
    await expect(panel.locator('.attachment-card')).toContainText('notes.txt');
    await expect(panel.getByLabel('Add a comment')).toHaveValue(
      mode === 'comment-paste' ? '' : 'Draft stays here',
    );
    if (mode === 'existing-comment-paste')
      await expect(panel.getByLabel('Edit comment', { exact: true })).toHaveValue(
        'Unsaved existing comment edit',
      );
    expect(uploaded).toBe(true);
    expect(finalized).toBe(true);
    await expect(panel.getByRole('link', { name: 'Download notes.txt' })).toBeVisible();
    await expect(panel.getByRole('button', { name: 'Remove notes.txt' })).toBeVisible();
  });
}
