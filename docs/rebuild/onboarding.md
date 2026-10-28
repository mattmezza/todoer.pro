# Account welcome

New verified accounts receive an optional two-question `/welcome` after sign-in.
Existing accounts keep their normal landing; General settings can reopen setup.
Verification and MFA still happen before onboarding.

1. Choose Personal, Work, or both. A short sentence explains spaces as containers
   for one area of life. Neither choice is selected initially.
2. Pick useful lists grouped by destination (`@personal / #groceries`, etc.).
   Suggestions have coordinated colors. A collapsed “Add your own…” supports a
   custom name in either space. “I’ll decide on lists later” creates the selected
   spaces without suggested lists. Users add actual tasks inside the app.

Migration 38 adds the space-choice draft to the existing account onboarding row.
The first step only persists that draft. Completion validates all choices, then
creates spaces and lists through authorized normal mutation methods in one
transaction. Lowercase slug names are used for new entities. Matching active
spaces owned by this account and their lists are reused. Existing content, names,
sharing and list colors are preserved. The normal system Inbox/default lists
remain available. Skipping creates no entities. Revisions catch stale tabs, and
persisted completion makes retries after lost responses safe. A previously open
old welcome draft with no choices starts at the first question.

The form is a single column without a preview, feature grid, task creation,
permissions or upsell. Explicit sign-in return destinations still take precedence
at completion. The mobile PWA installation prompt waits until after Welcome.

## Desktop chase

`welcome-chase.js` draws one decorative canvas at the bottom edge of desktop
windows (1024px+, without a coarse touch pointer). It uses projected shaded capsules and
spheres for a miniature human, two-link leg IK and ground-speed stance feet,
opposite arm swings, breathing/rest periods and a jumping task. The chase uses the empty left gutter along the bottom edge, so it does not
cross the form. The task escapes
near misses and jumps over the runner at window edges. It waves while the human
rests. These are fictional objects, unrelated to account tasks.

Successful completion fades the form downward, lets the runner catch and check
the task, and celebrates with a short jump/confetti sequence before navigating.
Failed saves never trigger that ending. A 3.8-second bound prevents animation
from holding navigation. Reduced motion, mobile and hidden tabs skip the sequence;
normal animation frames pause when the tab is hidden. The canvas does not receive
pointer events or appear in the accessibility tree. No runtime dependencies,
external assets, audio or permissions are needed.

Validation covers draft-only writes, atomic completion, input validation, retries,
reopening without duplicates, and skip/undecided behavior. Browser scenarios cover
both selected spaces, grouped list choices, reload, lost responses, desktop rest
and celebration, reduced motion, mobile layout and installation deferral.
