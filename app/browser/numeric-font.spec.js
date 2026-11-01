import { test, expect } from '@playwright/test';

test('numbers use the same-width font in text and form fields', async ({ page }) => {
  await page.goto('/signin');
  await page.getByRole('button', { name: 'Enter development workspace' }).click();
  await page.waitForFunction(() =>
    getComputedStyle(document.body).fontFamily.includes('Todoer Numeric'),
  );
  const result = await page.evaluate(async () => {
    await document.fonts.load('16px "Todoer Numeric"', '0123456789');
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    ctx.font = '16px "Todoer Numeric", sans-serif';
    const widths = [...'0123456789'].map((digit) => ctx.measureText(digit).width);
    const input = document.createElement('input');
    document.body.append(input);
    const result = {
      body: getComputedStyle(document.body).fontFamily,
      input: getComputedStyle(input).fontFamily,
      spread: Math.max(...widths) - Math.min(...widths),
      loaded: document.fonts.check('16px "Todoer Numeric"', '0123456789'),
    };
    input.remove();
    return result;
  });
  expect(result.loaded).toBe(true);
  expect(result.body).toContain('Todoer Numeric');
  expect(result.input).toContain('Todoer Numeric');
  expect(result.spread).toBeLessThan(0.01);
});
