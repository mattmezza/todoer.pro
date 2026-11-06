import { readFile, writeFile } from 'node:fs/promises';

// HTMX 4.0.1 passes parseHTMLUnsafe an options object that older Chromium
// releases reject. Keep the newer comment-preservation workaround where supported,
// and fall back to the standard DOMParser on browsers without that overload.
let source = await readFile('node_modules/htmx.org/dist/htmx.min.js', 'utf8');
const expression = /Document\.parseHTMLUnsafe\?\.\((\w+),\{\}\)/;
if (!expression.test(source)) throw Error('Review the HTMX browser compatibility patch after upgrading.');
source = source.replace(expression, (_, value) =>
  `(()=>{try{return Document.parseHTMLUnsafe?.(${value},{})}catch{}})()`);
await writeFile('app/assets/htmx.min.js', source);
