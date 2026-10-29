# Todoer public website

Static HTML, compiled Tailwind CSS, no browser JavaScript required. Deploy `www` to Cloudflare Pages. Domain: todoer.pro; application links: app.todoer.pro.

From the repository root:

```
python www/tools/build.py
npx tailwindcss -i www/source.css -o www/style.css --minify
python -m http.server 8098 --directory www
```

Page content and shared layout live in `tools/build.py`. Existing privacy and terms sections are preserved during generation. Review legal text against actual production providers, retention, support contact and billing before launch; these inherited pages remain explicitly drafts. No checkout or credit sales are implemented. The static site does not establish hosted availability.

Add substantial, distinct guide records to the generator; do not create keyword-swapped pages. Sitemap and llms.txt are generated from published records. Pricing.md must remain consistent with the public pricing page. Social preview is a local 1200×630 PNG. Use the website browser check in `tools/check.cjs` after serving locally.

## Multi-currency pricing

The monorepo-root `pricing.json` is the source of truth. Amounts are integer minor units, with explicit currency metadata, billing intervals, proposal status and disabled checkout. `www/pricing.json` is a generated deployment copy.

USD research anchors: $6/month, $48/year, $149 one-time. Apply dated ECB cross rates and 3% to every currency (including USD), then round upward: monthly to .99, yearly/one-time to a whole amount ending in 9. These are proposed price points, not live offers. Taxes are excluded; production tax treatment is pending.

To change anchors/rates, edit root `pricing.json`, run `python www/tools/pricing.py`, then `python www/tools/build.py`. Builds use pinned rates without network requests. The pricing page provides native expandable currency tables; every currency works without JavaScript. AI credit packs remain unpriced.

The launch catalog now anchors EUR at €12/year and €49 perpetual; planned standard prices are €24/year and €79. These are future price references, never represented as past selling prices. EUR anchors already include the allowance; foreign currencies add 3% and round upward. There is no monthly billing option. `tools/redesign.py` composes the current narrative and layouts. Legal copy is maintained in `content/`, independently of generated HTML. Reveal animation and currency selection progressively enhance the site and respect reduced motion.

Geist is loaded using Bunny Fonts CDN (weights 400–800, display swap), with a system-font fallback. Privacy and cookie pages disclose the external font requests. `tools/selfhosting.py` supplies the free self-hosted pricing option and practical FAQs. Root pricing.json includes a zero-cost self_hosted entry; infrastructure and optional AI fees are not included.
