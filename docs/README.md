# Todoer documentation

The public handbook at **docs.todoer.pro** is built from Markdown with a small Python static generator. Its dark surfaces, Geist typography, soft blue and lilac accents, and spacing follow the public website and application.

```sh
python3 -m venv .local/docs-venv
.local/docs-venv/bin/pip install -r docs/requirements.txt
.local/docs-venv/bin/python docs/build.py
python3 -m http.server 8099 --bind 127.0.0.1 --directory docs/_site
```

Open http://127.0.0.1:8099. No frontend package installation is needed. The build makes no network requests and checks all generated local links, assets and heading anchors. Python 3.10 or newer is required.

Edit the existing Markdown in `rebuild/` or the introductory `getting-started.md`. `GROUPS` in `build.py` is the explicit publication list and controls sidebar ordering and reader-facing titles. New guides must be added there. Existing technical source files keep their repository paths; their public pages use the same path with `.html` instead of `.md`.

Internal `.md` links are rewritten to public pages. Links to source-only documents and repository code resolve to GitHub. Raw HTML in Markdown is disabled. Planning in `marketing/`, design feedback, legacy inventories and implementation acceptance ledgers are not published. The output directory contains only generated pages, CSS, JavaScript, favicon, local search data and hosting metadata; it does not copy the source tree.

`assets/docs.css` contains the responsive theme. `assets/docs.js` progressively adds local search (⌘/Ctrl+K or `/`) and code copying. Navigation and all guide content work without JavaScript. Search queries stay in the browser. Geist is requested from Bunny Fonts, matching the public website; system fonts are the fallback. A mobile navigation menu, keyboard focus states and reduced-motion preferences are supported.

GitHub Actions publishes **only `docs/_site`** using GitHub Pages. Configure Pages to use GitHub Actions and set the custom domain to `docs.todoer.pro` in repository settings. The generator writes `CNAME`, `.nojekyll`, sitemap, robots and `llms.txt` files. Generated output is ignored by Git.

The Markdown content, site generator and design are covered by the repository's MIT license.

After building, `node docs/check.cjs` starts a temporary preview on a random loopback port and verifies local search, mobile layouts, menu navigation, no-JavaScript navigation and browser errors. This optional browser check uses the root Playwright dependency and requires Chromium (`npx playwright install chromium`).
