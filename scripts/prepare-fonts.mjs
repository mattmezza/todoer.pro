// Ship fonts with the release instead of adding a cross-origin CSS dependency
// and separate network requests for every weight and fallback format.
import { readFile, copyFile, writeFile, mkdir } from 'node:fs/promises';
await mkdir('app/assets/fonts', { recursive: true });
let css = '';
for (const name of ['geist', 'inter']) {
  const root = `node_modules/@fontsource-variable/${name}`;
  const source = await readFile(`${root}/wght.css`, 'utf8');
  for (const [, filename] of source.matchAll(/url\(\.\/files\/([^)]+)\)/g))
    await copyFile(`${root}/files/${filename}`, `app/assets/fonts/${filename}`);
  await copyFile(`${root}/LICENSE`, `app/assets/fonts/${name}-LICENSE.txt`);
  css += source
    .replaceAll(' Variable', '')
    .replaceAll('font-display: swap', 'font-display: optional')
    .replaceAll('./files/', './fonts/');
}
await writeFile('app/assets/web-fonts.css', css);
