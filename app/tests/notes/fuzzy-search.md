# Indexed, typo-tolerant search

The existing FTS5 index already covers entity titles/descriptions, comments,
tags and attachment filenames, with Unicode tokenization, accent folding,
prefix indexes and transactional update triggers. Migration 34 adds only an
`fts5vocab` virtual table over that index: no duplicated content, backfill,
new dependency, service, or background worker.

Queries keep AND semantics across words and literal quoting. Each eligible
word is expanded to real dictionary terms at one edit distance (insertion,
deletion, substitution or adjacent transposition), found through indexed
vocabulary lookups. Exact title, title prefix, other title matches and exact
body matches rank ahead of typo-only matches. Existing authorization, archived
ancestor exclusion, filters and snapshot-bound pagination still apply to every
result; dictionary words are never exposed as suggestions.

Work is bounded: at most eight query words receive expansion, ASCII letters
only, length 4–24, at most eight dictionary alternatives per word. Short words,
long queries, non-ASCII input and explicit reference queries keep the existing
Unicode/accent-insensitive prefix behavior. Corrections match complete words;
ordinary non-misspelled prefixes still work. This intentionally avoids broad
substring matching and multi-edit guesses that would add noise and cost.

Tests cover all four edit types, first-letter errors, multiword queries,
description matches, ranking, private/archived exclusions, list filters and
index synchronization after edits. The existing ignored 10k-task/10k-comment
performance test now exercises a misspelled comment query.

Validation (local debug build): 228 library tests passed, eight unrelated
performance tests ignored; the explicit 10k-task + 10k-comment typo benchmark
passed, querying all 10k matching comments in 788 ms with no concurrent suite.
Ten live HTTP omnibar queries (`concurent`) measured 70 ms median / 88 ms max.
These are local observations, not production latency guarantees. Browser smoke
confirmed typo results through the actual omnibar with no JavaScript errors.
