# Todoer public website

Static HTML and compiled Tailwind CSS. Deploy the staged `www/_site` directory to Cloudflare Pages. Domain: todoer.pro; application: app.todoer.pro.

From the repository root:

```sh
make www-build
python3 -m http.server 8098 --bind 127.0.0.1 --directory www/_site
node www/tools/check.cjs
```

Page content and layout live in `tools/build.py`, `tools/redesign.py`, `tools/articles.py` and `tools/selfhosting.py`. Legal copy lives in `content/`, separate from generated HTML. Review inherited legal drafts against the production configuration before launch. The website does not establish hosted availability.

Root `pricing.json` contains only the public offer catalog. The build copies it to `www/pricing.json` and generates matching pricing HTML and `pricing.md`. Core access is free; prepaid built-in AI credits and per-account BYOK are planned. No checkout, credit sales or spend overview is implemented. Keep provider costs, pricing calculations and commercial planning in gitignored local files, never in public artifacts.

Add substantial guide records rather than keyword-swapped pages. Sitemap and `llms.txt` are generated from published records. Social preview uses the existing local 1200×630 PNG. Browser checks cover mobile and desktop layout.

Geist loads from Bunny Fonts with a system fallback; privacy and cookie pages disclose those requests. Reveal animations respect reduced motion. Website publishing remains manual.
