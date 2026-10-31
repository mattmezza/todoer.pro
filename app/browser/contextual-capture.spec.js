import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

test.beforeEach(async ({ page }) => {
  await page.route('https://capture.test/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.startsWith('/assets/'))
      return route.fulfill({
        contentType: path.endsWith('.css') ? 'text/css' : 'text/javascript',
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
    const priority = form.locator('.capture-badges > .capture-priority');
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
      await expect(form.locator('.capture-tags .tag-pill')).toHaveText([
        '+home',
        '+errands',
      ]);
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

test('capture tags stack beneath priority without separating title and description', async ({
  page,
}) => {
  await page.route('https://fonts.bunny.net/**', (route) => route.abort());
  await page.addStyleTag({
    content: readFileSync('app/assets/app.css', 'utf8').replace(/@import[^;]+;/g, ''),
  });
  await page.evaluate(async () => {
    const { mountSmartCapture } = await import('/assets/smart-capture.js');
    document.body.className = 'reference-list-page';
    const main = document.querySelector('main');
    main.className = 'task-page';
    main.style.cssText = 'margin:24px;padding:0;max-width:none;min-width:0';
    for (const kind of ['main', 'section', 'child', 'quick']) {
      const form = document.createElement('form');
      form.dataset.kind = kind;
      if (kind === 'child') form.dataset.childForm = '';
      else if (kind === 'quick') form.className = 'quick-capture-form';
      else form.className = 'inline-capture';
      if (kind === 'main') form.id = 'capture-form';
      form.innerHTML =
        '<input name="title" value="Buy milk p1 +family +weekly-shopping"><textarea name="description" placeholder="Description"></textarea><div class="inline-actions"></div>';
      main.append(form);
      await mountSmartCapture(form).ready;
      if (kind === 'quick')
        form.querySelector('.capture-entry').classList.add('quick-capture-fields');
    }
    const settings = document.createElement('div');
    settings.className = 'settings-content';
    settings.innerHTML = '<span class="tag-pill">+family</span>';
    main.append(settings);
  });
  for (const width of [1100, 375]) {
    await page.setViewportSize({ width, height: 900 });
    const geometry = await page.evaluate(() => {
      const reference = getComputedStyle(
        document.querySelector('.settings-content .tag-pill'),
      );
      return [...document.querySelectorAll('form')].map((form) => {
        const input = form.querySelector('input').getBoundingClientRect();
        const description = form.querySelector('textarea').getBoundingClientRect();
        const priority = form.querySelector('.capture-priority').getBoundingClientRect();
        const tag = form.querySelector('.capture-tag');
        const pill = tag.getBoundingClientRect();
        const style = getComputedStyle(tag);
        return {
          kind: form.dataset.kind,
          tagsBelow: pill.top >= priority.bottom,
          tagsRight: pill.left >= input.right,
          descriptionGap: description.top - input.bottom,
          matchingStyle: ['borderRadius', 'fontSize', 'color', 'backgroundColor'].every(
            (key) => style[key] === reference[key],
          ),
          overflow: document.documentElement.scrollWidth > window.innerWidth,
        };
      });
    });
    for (const item of geometry) {
      expect(item.tagsBelow).toBe(true);
      expect(item.tagsRight).toBe(true);
      expect(item.descriptionGap).toBeGreaterThanOrEqual(0);
      expect(item.descriptionGap).toBeLessThanOrEqual(12);
      expect(item.matchingStyle).toBe(true);
      expect(item.overflow).toBe(false);
    }
  }
});

test('quick capture close button stays clear of parsed metadata', async ({ page }) => {
  await page.addStyleTag({
    content: readFileSync('app/assets/app.css', 'utf8').replace(/@import[^;]+;/g, ''),
  });
  await page.evaluate(async () => {
    const { mountSmartCapture } = await import('/assets/smart-capture.js');
    const dialog = document.createElement('dialog');
    dialog.className = 'capture-omnibar';
    dialog.innerHTML =
      '<button class="capture-close" aria-label="Close capture">×</button><form class="quick-capture-form"><input name="title" value="buy milk p0 +family"><textarea name="description" placeholder="Add description"></textarea></form>';
    document.body.append(dialog);
    dialog.showModal();
    await mountSmartCapture(dialog.querySelector('form')).ready;
    dialog.querySelector('.capture-entry').classList.add('quick-capture-fields');
  });
  for (const width of [375, 1100]) {
    await page.setViewportSize({ width, height: 600 });
    const close = await page.getByRole('button', { name: 'Close capture' }).boundingBox();
    for (const selector of [
      '.capture-priority',
      '.capture-tag',
      '.smart-capture-input',
    ]) {
      const content = await page.locator(selector).boundingBox();
      expect(
        content.x + content.width <= close.x || content.y >= close.y + close.height,
      ).toBe(true);
    }
  }
});

test('contextual dates parse separately, shortcuts navigate, and long titles wrap', async ({
  page,
}) => {
  await page.addStyleTag({ content: readFileSync('app/assets/app.css', 'utf8') });
  await page.evaluate(async () => {
    document.querySelector('main').innerHTML =
      '<form class="inline-capture" id="capture-form" style="width:360px"><textarea name="title" rows="1"></textarea><textarea name="description"></textarea><div class="inline-actions"></div></form>';
    const { mountSmartCapture } = await import('/assets/smart-capture.js');
    window.capture = mountSmartCapture(document.querySelector('form'));
    await window.capture.ready;
  });
  const title = page.locator('textarea[name=title]');
  await title.fill('Plan vegetables .today -in3d +home p1');
  const metadata = await page.evaluate(() => window.capture.get().metadata);
  expect(metadata.planned_date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  expect(metadata.due_date).not.toBe(metadata.planned_date);
  await expect(page.locator('.capture-date')).toHaveCount(2);
  await expect(page.locator('input[type=date]')).toHaveCount(0);
  await expect(page.locator('.capture-date[data-date-kind=due]')).toHaveCSS(
    'color',
    'rgb(255, 59, 59)',
  );
  expect(
    await page
      .locator('.capture-date[data-date-kind=planned]')
      .evaluate((el) => getComputedStyle(el).color),
  ).not.toBe('rgb(255, 59, 59)');
  await page.getByRole('button', { name: 'Shortcuts', exact: true }).click();
  await page.getByRole('button', { name: 'Planned date →', exact: true }).click();
  await page.getByRole('button', { name: 'Everyday →', exact: true }).click();
  await page.getByRole('button', { name: '.tomo', exact: true }).click();
  await expect(title).toBeFocused();
  await page.getByRole('button', { name: 'Shortcuts', exact: true }).click();
  await page.getByRole('button', { name: 'Due date →', exact: true }).click();
  await page.getByRole('button', { name: 'Back to previous shortcuts' }).click();
  await expect(
    page.getByRole('button', { name: 'Planned date →', exact: true }),
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await title.fill(
    'A long task title with enough words to wrap across several lines while remaining fully visible without horizontal scrolling '.repeat(
      3,
    ),
  );
  await expect.poll(() => title.evaluate((el) => el.offsetHeight)).toBeGreaterThan(60);
  expect(await title.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBeTruthy();
});
