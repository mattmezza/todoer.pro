import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

test.beforeEach(async ({ page }) => {
  await page.route('https://names.test/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/assets/entity-name.js')
      return route.fulfill({
        contentType: 'text/javascript',
        body: readFileSync(`app${path}`, 'utf8'),
      });
    return route.fulfill({
      contentType: 'text/html',
      body: `<!doctype html><main>
      <form><label>Space<input data-reference-prefix="@" value="Personal Plans" aria-describedby="name-help"></label><span id="name-help">Name</span></form>
      <form><label>List<textarea aria-label="List" data-reference-prefix="#">Shopping List</textarea></label></form>
      <form><label>Section<input data-reference-prefix="$" value="This Week"></label></form>
      </main><script type="module" src="/assets/entity-name.js"></script>`,
    });
  });
  await page.goto('https://names.test/');
});

test('live references follow server slug rules without altering typed names or the caret', async ({
  page,
}) => {
  const examples = [
    ['  #My  NEXT_Project! ', 'my-next-project'],
    ['CAFÉ 東京 ÉTÉ', 'café-東京-été'],
    ['💡 / ___', 'untitled'],
    ['X'.repeat(500), 'x'.repeat(64)],
    ['a'.repeat(63) + ' words', 'a'.repeat(63)],
  ];
  for (const [label, prefix] of [
    ['Space', '@'],
    ['List', '#'],
    ['Section', '$'],
  ]) {
    const input = page.getByLabel(label, { exact: true });
    const output = input
      .locator('xpath=ancestor::form[1]')
      .locator('.entity-name-reference');
    for (const [name, slug] of examples) {
      await input.fill(name);
      await expect(input).toHaveValue(name);
      await expect(output).toHaveText(`Reference preview: ${prefix}${slug}`);
    }
    await input.fill('Personal Plans');
    await input.evaluate((field) => field.setSelectionRange(8, 8));
    await input.pressSequentially('Weekly ');
    await expect(input).toHaveValue('PersonalWeekly  Plans');
    expect(await input.evaluate((field) => field.selectionStart)).toBe(15);
    await input.fill('');
    await expect(output).toBeHidden();
  }
  const input = page.getByLabel('Space', { exact: true });
  await expect(input).toHaveAttribute(
    'aria-describedby',
    /^name-help reference-preview-/,
  );
  await input.locator('..').evaluate((label) => label.closest('form').reset());
  await expect(
    input.locator('xpath=ancestor::form[1]').locator('.entity-name-reference'),
  ).toHaveText('Reference preview: @personal-plans');
});

test('dynamic rename forms show a preview without moving the input or duplicating help', async ({
  page,
}) => {
  await page.evaluate(() => {
    const form = document.createElement('form');
    form.dataset.listEdit = '';
    form.innerHTML =
      '<input name="title" data-reference-prefix="#" aria-label="Rename list"><textarea name="description"></textarea><div class="inline-actions"></div>';
    document.querySelector('main').append(form);
  });
  const input = page.getByLabel('Rename list');
  await input.fill('My New List');
  const preview = page.locator('[data-list-edit] .inline-actions .entity-name-reference');
  await expect(preview).toHaveText('Reference preview: #my-new-list');
  await input.evaluate((field) => {
    const replacement = document.createElement('textarea');
    for (const attr of field.attributes) replacement.setAttribute(attr.name, attr.value);
    replacement.value = field.value;
    field.replaceWith(replacement);
  });
  await page.getByLabel('Rename list').fill('Renamed Again');
  await expect(preview).toHaveText('Reference preview: #renamed-again');
  await expect(page.locator('[data-list-edit] .entity-name-reference')).toHaveCount(1);
});
