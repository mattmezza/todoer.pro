# Website positioning and launch decisions

Research date: 2026-10-28. Desk research from vendor pages, not hands-on competitor testing.

| Alternative | Strength | Tradeoff for target segment | Source |
| --- | --- | --- | --- |
| Todoist | Extensive integrations, multiple views, individual and team packaging | Competing on ecosystem breadth is unrealistic; focus Todoer on inspectability and deliberate automation | https://www.todoist.com/pricing |
| TickTick | Calendar, habits and broad personal productivity toolkit | Breadth may be unnecessary for people wanting a focused task workflow; this is a fit judgment | https://ticktick.com/about/upgrade |
| Things | Apple-specific product experience, separate platform purchases | Device-specific offering differs from a browser workflow | https://culturedcode.com/things/pricing/ |

TickTick currently lists US$49.99/year. Todoist's rendered page did not expose numerical prices consistently, so no exact competitor price is baked into the site. Things directs users to their local App Store for prices.

## Pricing recommendation — private proposal, not a published offer

Begin with one hosted individual plan. Test USD 48–60/year and USD 6–7/month in buyer interviews and small sequential pricing experiments; these are research hypotheses, not validated willingness to pay. Decide storage, attachment sizes, service costs and support obligations first. Keep the source MIT licensed. Avoid charging for basic data export as a retention tactic.

Do not sell unlimited lifetime hosting. If buyers strongly prefer a one-time purchase, define perpetual software/version access separately from a finite hosted entitlement. A USD 149–199 founder offer could be researched only after unit economics, exact entitlements, support, refund terms and shutdown policy are defined. No fake scarcity or invented discount anchors.

AI credits should be an optional separate purchase only once metering, taxes, payment processing, expiration/refund rules, failure charging and provider costs are designed. Current assistance budgets are technical limits, not a retail credit system. There is no checkout implementation in this site.

## Copy and behavioral choices

One primary app CTA reduces decision effort. The illustrative workspace makes the product concrete without fake customer proof. Guides provide useful value before purchase. Honest release boundaries address switching anxiety. No invented testimonials, urgency, user counts or performance claims.

Headline alternatives: “A clear next action. A little more breathing room.” (calmer); “Your tasks, with room to think.” (broader). Current headline preserves the legacy voice requested by the founder. CTA alternative: “Explore the app” once access requirements are confirmed.

## Content and programmatic publishing

Initial hub: /guides.html. Distinct guides cover capture, nested task decomposition and weekly review. The data-driven generator shares layout, not interchangeable keyword paragraphs. Expand only with original worked examples, current product evidence and a distinct search intent. No keyword volume research has been obtained; do not infer demand from the existence of these pages.

Next candidates: freelancer project handoff checklist, recurring household task setup, permission-safe agent workflow. Validate search demand before producing larger batches. Source of publishing guidance: https://developers.google.com/search/docs/fundamentals/creating-helpful-content

AI discoverability: static semantic HTML, internal links, canonical URLs, Article metadata, sitemap, robots, llms.txt and truthful pricing.md. These do not guarantee indexing or AI citations. Establish a baseline for relevant queries and measure qualified app visits after launch. No analytics added yet.

## Launch requirements

Privacy and terms are inherited drafts, retained transparently. Confirm actual subprocessors, hosting jurisdictions, legal bases where applicable, retention/deletion and backup practices, private electronic contact, international transfer safeguards and applicability of GDPR/Swiss FADP. Obtain legal review before adopting them. Cookie notice and operator notice are included; a DPA and subprocessor schedule depend on the actual business offering and production stack. Do not invent a VAT or registry number.

Billing requires actual checkout integration and approved sales/refund/withdrawal terms before sales begin. The current static pricing page intentionally sells nothing. Confirm app availability, production readiness, migration support and final release audits before changing the development notice.

## Updated direction — 2026-10-29

Founder-selected launch anchors supersede the earlier research ranges: EUR 12/year (EUR 1/month equivalent) and EUR 49 perpetual. Planned standard references: EUR 24/year and EUR 79, explicitly future references, not historical prices. No sales enabled; perpetual entitlements remain unresolved. Root pricing.json controls currencies. Position around relief from scattered commitments, small next actions and connecting AI conversations to task follow-through. Do not manufacture social proof or claim verified Claude/ChatGPT hosted connectors. Product evidence and transparent boundaries supply trust until genuine customer recommendations exist.

## Evergreen guide expansion

Guides now include worked examples, review-first AI prompts, failure cases and internal related links. The AI task-management article cites provider connection documentation without claiming accepted Todoer hosted connectors. No keyword volume or ranking evidence has been obtained. Track impressions and query intent after indexing before expanding into separate Claude/ChatGPT pages that might otherwise duplicate this guide. Article text is maintained in www/tools/articles.py.

Typography assessment: Geist Sans fits the current spare, technical identity, but a family substitution alone will not fix spacing or increase conversion. Keep app and website aligned if adopting it; self-host font files with their OFL notice and verify actual loading, wrapping, Cyrillic/Latin glyph needs and numeric prices. Current website names Inter but does not bundle an Inter font face, so many visitors see their system sans. No font migration performed merely on the basis of a comparison question.
