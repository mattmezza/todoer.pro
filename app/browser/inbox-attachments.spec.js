import { test, expect } from '@playwright/test';
import { randomUUID } from 'node:crypto';

for (const mode of ['drop', 'paste', 'comment-paste', 'existing-comment-paste']) {
  test(`${mode} uploads a file and preserves the comment draft`, async ({ page }) => {
    let finalized = false;
    let uploaded = false;
    await page.route('**/api/v1/tasks/*/uploads', async (route) => {
      const upload = route.request().postDataJSON();
      if (mode === 'existing-comment-paste') expect(upload.comment_id).toBeTruthy();
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
      await route.fulfill({ json: { id: 'attached2345' } });
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
        const event = new ClipboardEvent('paste', { bubbles: true, cancelable: true });
        // Firefox ignores clipboardData in the synthetic ClipboardEvent constructor.
        Object.defineProperty(event, 'clipboardData', { value: clipboardData });
        element.dispatchEvent(event);
      }, data);
    }
    await expect(panel.locator('.attachment-card')).toContainText('notes.txt');
    await expect(panel.getByLabel('Add a comment')).toHaveValue(
      mode === 'comment-paste'
        ? 'Draft stays here[notes.txt](/api/v1/attachments/attached2345/download)'
        : 'Draft stays here',
    );
    if (mode === 'existing-comment-paste')
      await expect(panel.getByLabel('Edit comment', { exact: true })).toHaveValue(
        'Unsaved existing comment edit[notes.txt](/api/v1/attachments/attached2345/download)',
      );
    expect(uploaded).toBe(true);
    expect(finalized).toBe(true);
    await expect(panel.getByRole('link', { name: 'Download notes.txt' })).toBeVisible();
    await expect(panel.getByRole('button', { name: 'Remove notes.txt' })).toBeVisible();
  });
}

for (const width of [390, 1280]) {
  test(`clipboard images upload immediately into Markdown and task files at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    let finalizeUpload;
    let uploading = false;
    let finalized = false;
    await page.route('**/api/v1/tasks/*/uploads', (route) => {
      expect(route.request().postDataJSON().comment_id).toBeNull();
      return route.fulfill({ json: { intent: { id: 'image-upload', revision: 1 } } });
    });
    await page.route('**/api/v1/uploads/image-upload/content?*', (route) => {
      uploading = true;
      return route.fulfill({ json: {} });
    });
    await page.route('**/api/v1/uploads/image-upload/finalize', async (route) => {
      if (!finalized)
        await new Promise((resolve) => {
          finalizeUpload = resolve;
        });
      finalized = true;
      await route.fulfill({ json: { id: 'image2345' } });
    });
    await page.route('**/api/v1/attachments/image2345/preview', (route) =>
      route.fulfill({
        contentType: 'image/png',
        body: Buffer.from(
          'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=',
          'base64',
        ),
      }),
    );
    await page.route('**/fragments/tasks/*/discussion', async (route) => {
      const response = await route.fetch();
      let html = await response.text();
      if (!html.includes('data-file-form'))
        html = html.replace(
          '</section><h3',
          '<form data-file-form><input type="file" data-file-input hidden><button type="button" data-file-choose>Attach file</button><p data-file-selection hidden></p><progress data-upload-progress hidden></progress><button data-file-upload hidden></button><button data-file-abandon hidden></button></form></section><h3',
        );
      if (finalized)
        html = html.replace(
          '<div class="attachment-grid">',
          '<div class="attachment-grid"><div data-file-row class="attachment-card"><img class="attachment-thumbnail" src="/api/v1/attachments/image2345/preview" alt=""><span>image.png</span></div>',
        );
      await route.fulfill({ response, body: html });
    });
    await page.goto('/signin');
    await page.getByRole('button', { name: 'Enter development workspace' }).click();
    await page.goto('/inbox');
    const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
    const inbox = await (await page.request.get('/api/v1/inbox')).json();
    const created = await page.request.post('/api/v1/capture', {
      headers: { 'x-csrf-token': csrf, 'Idempotency-Key': randomUUID() },
      data: { list_id: inbox.list_id, tasks: [{ title: `Image paste ${randomUUID()}` }] },
    });
    const [task] = await created.json();
    await page.goto(`/tasks/${task.id}`);
    await expect(page.locator('[data-file-form]')).toBeVisible();
    await page.locator('[data-comment-composer] > summary').click();
    const input = page.getByLabel('Add a comment');
    await input.fill('Before image ');
    const paste = async (target) =>
      target.evaluate((element) => {
        const transfer = new DataTransfer();
        transfer.items.add(new File(['image'], 'image.png', { type: 'image/png' }));
        const event = new ClipboardEvent('paste', { bubbles: true, cancelable: true });
        Object.defineProperty(event, 'clipboardData', { value: transfer });
        element.dispatchEvent(event);
      });
    await paste(input);
    await expect.poll(() => uploading).toBe(true);
    await expect(input).toHaveValue(/Before image \[Uploading image.png/);
    await input.press('End');
    await input.pressSequentially(' after image');
    await expect.poll(() => Boolean(finalizeUpload)).toBe(true);
    finalizeUpload();
    await expect(input).toHaveValue(
      'Before image ![image.png](/api/v1/attachments/image2345/preview) after image',
    );
    await page.getByRole('button', { name: 'Post', exact: true }).click();
    await expect(page.locator('[data-comment-rendered] img')).toHaveAttribute(
      'src',
      '/api/v1/attachments/image2345/preview',
    );
    await expect(page.locator('.attachment-thumbnail')).toBeVisible();
    uploading = false;
    await paste(page.locator('body'));
    await expect.poll(() => uploading).toBe(true);
    await expect(page.locator('[data-file-selection]')).toBeHidden();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBe(true);
  });
}
