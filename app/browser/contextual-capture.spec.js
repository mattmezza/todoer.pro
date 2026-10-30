import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

test.beforeEach(async ({ page }) => {
  await page.route('https://capture.test/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.startsWith('/assets/'))
      return route.fulfill({
        contentType: 'text/javascript',
        body: readFileSync(`app${path}`, 'utf8'),
      });
    if (path === '/api/v1/capture/options')
      return route.fulfill({
        json: {
          preferences: { smart_enabled: true, date_enabled: true },
          destinations: [
            { space: 'personal', space_id: 's', list: 'inbox', list_id: 'l' },
          ],
          tags: ['home'],
        },
      });
    return route.fulfill({ contentType: 'text/html', body: '<main></main>' });
  });
  await page.goto('https://capture.test/');
});

test('contextual forms preserve locations and dates while parsing allowed metadata', async ({
  page,
}) => {
  const result = await page.evaluate(async () => {
    const { mountSmartCapture } = await import('/assets/smart-capture.js');
    document.querySelector('main').innerHTML =
      '<section class="task-section" data-section-id="veg" data-section-title="veggies"></section>';
    const results = [];
    for (const kind of ['main', 'section', 'child']) {
      const form = document.createElement('form');
      if (kind === 'child') form.dataset.childForm = '';
      else form.className = 'inline-capture';
      if (kind === 'main') form.id = 'capture-form';
      form.innerHTML = '<input name="title"><div class="inline-actions"></div>';
      document.querySelector('main').append(form);
      const capture = mountSmartCapture(form);
      await capture.ready;
      const input = form.querySelector('input');
      input.value = 'Buy @personal #inbox $veggies tomorrow p1 +home';
      input.dispatchEvent(new Event('input'));
      results.push(capture.get());
      input.value = '@';
      input.dispatchEvent(new Event('input'));
      if (!form.querySelector('[role=listbox]').hidden)
        throw Error('Location suggestions visible');
      if (!form.querySelector('.inline-actions .capture-help'))
        throw Error('Help misplaced');
      form.remove();
    }
    return results;
  });
  expect(result[0].title).toBe('Buy @personal #inbox tomorrow');
  expect(result[0].section_id).toBe('veg');
  for (const item of result.slice(1))
    expect(item.title).toBe('Buy @personal #inbox $veggies tomorrow');
  for (const item of result) {
    expect(item.list_id).toBeUndefined();
    expect(item.metadata).toEqual({ priority: 1, tags: ['home'], due_date: null });
  }
});

test('all contextual composers show parsed priority and tag badges without destination prose', async ({
  page,
}) => {
  await page.evaluate(async () => {
    const { mountSmartCapture } = await import('/assets/smart-capture.js');
    for (const kind of ['main', 'section', 'child']) {
      const form = document.createElement('form');
      form.dataset.kind = kind;
      if (kind === 'child') form.dataset.childForm = '';
      else form.className = 'inline-capture';
      if (kind === 'main') form.id = 'capture-form';
      form.innerHTML = '<input name="title"><div class="inline-actions"></div>';
      document.querySelector('main').append(form);
      await mountSmartCapture(form).ready;
    }
  });
  for (const kind of ['main', 'section', 'child']) {
    const form = page.locator(`[data-kind="${kind}"]`);
    const input = form.locator('input');
    const preview = form.locator('.capture-result');
    const priority = form.locator('.smart-capture-input > .capture-priority');
    await input.fill('Buy milk');
    await expect(preview).toBeHidden();
    await expect(priority).toBeHidden();
    for (const [shortcut, value] of [
      ['p0', 0],
      ['!!', 1],
      ['!', 2],
      ['!3', 3],
    ]) {
      await input.fill(`Buy milk ${shortcut} +home +errands`);
      await expect(priority).toHaveText(`P${value}`);
      await expect(priority).toHaveAttribute('data-priority', String(value));
      await expect(preview.locator('.capture-tag')).toHaveText(['+home', '+errands']);
      await expect(preview).not.toContainText('Will add');
      await expect(preview).not.toContainText('To ');
    }
    await form.getByRole('button', { name: 'Use text as typed' }).click();
    await expect(priority).toBeHidden();
    await expect(preview).toBeHidden();
    await form.getByRole('button', { name: 'Interpret shortcuts' }).click();
    await expect(priority).toHaveText('P3');
    await input.fill('Buy milk p0 p1 +home');
    await expect(priority).toBeHidden();
    await input.fill('');
    await expect(priority).toBeHidden();
    await expect(preview).toBeHidden();
  }
});

test('refresh appends rows without remounting existing tasks or capture forms', async ({
  page,
}) => {
  expect(
    await page.evaluate(async () => {
      const { reconcileTasks } = await import('/assets/task-list-refresh.js');
      const current = document.querySelector('main');
      const section = (rows) =>
        `<section class="task-section" data-section-id=""><div class="section-tasks">${rows.map((id) => `<article class="task" data-id="${id}">${id}</article>`).join('')}</div><form><input value="draft"></form></section>`;
      current.innerHTML = section(['a']);
      const row = current.querySelector('.task');
      const form = current.querySelector('form');
      const incoming = document.createElement('div');
      incoming.innerHTML = section(['a', 'b']);
      reconcileTasks(current, incoming);
      return (
        current.querySelector('.task') === row &&
        current.querySelector('form') === form &&
        current.querySelectorAll('.task').length === 2
      );
    }),
  ).toBe(true);
});
