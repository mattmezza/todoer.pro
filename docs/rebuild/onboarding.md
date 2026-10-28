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
Migration 39 removes the automatic Inbox trigger. Google/local registration and
all sign-in paths create no spaces or lists. Space creation also creates no list.
The first step only persists that draft. Completion validates all choices, then
creates spaces and lists through authorized normal mutation methods in one
transaction. Lowercase slug names are used for new entities. Matching active
spaces owned by this account and their lists are reused. Existing content, names,
sharing and list colors are preserved. Newly created spaces and lists receive
short, purpose-specific descriptions; custom lists get a description based on
their name. Blank descriptions on selected existing items are filled too,
including the system Personal space. Existing nonempty descriptions are preserved. New Personal weekly-task lists
start with seven weekday sections; Work weekly lists use Monday–Friday only.
Section order follows the account’s week-start preference. Reused lists keep
their existing sections. Inbox is a virtual empty destination until its first explicit task capture;
that write creates only its private storage list, never a space. Reading Inbox,
opening capture, or skipping Welcome does not provision anything. Skipping creates no entities. Revisions catch stale tabs, and
persisted completion makes retries after lost responses safe. A previously open
old welcome draft with no choices starts at the first question.

The form is a single column without a preview, feature grid, task creation,
permissions or upsell. Explicit sign-in return destinations still take precedence
at completion. The mobile PWA installation prompt waits until after Welcome.

## Desktop chase

`welcome-chase.js` draws one decorative canvas at the bottom edge of desktop
windows (1024px+, without a coarse touch pointer). A faceless, single-color line figure
runs with two-link leg IK, a short ground-contact phase, opposite bent-arm swings
and a speed-matched stride. The task is a checkbox card with a short chore such as “Sort receipts”.
Each offscreen wrap picks the next chore; its label stays stable while visible.
It jumps away near the runner and waves during rest periods.

The chase makes fast one-way passes across the bottom edge. Both actors completely
exit before wrapping together offscreen to re-enter from the left. It does not
reverse at the screen edge; rests can also happen outside the window. Conservative
horizontal drawing envelopes (including hands, waving arms and card tilt) stay at
least 14px apart throughout the chase and finale. These are fictional objects,
unrelated to account tasks.

Successful completion fades the form downward, lets the runner catch and check
the task, and celebrates with a short jump/confetti sequence before navigating.
Failed saves never trigger that ending. A 9-second bound prevents animation
from holding navigation. Reduced motion, mobile and hidden tabs skip the sequence;
normal animation frames pause when the tab is hidden. The canvas does not receive
pointer events or appear in the accessibility tree. No runtime dependencies,
external assets, audio or permissions are needed. The canvas uses at least 2×
resolution to smooth the thin moving limbs on standard-density screens.

Validation covers draft-only writes, atomic completion, input validation, retries,
reopening without duplicates, and skip/undecided behavior. Browser scenarios cover
both selected spaces, grouped list choices, reload, lost responses, desktop rest
and celebration, reduced motion, mobile layout and installation deferral.

Motion invariants can be checked independently with
`node --test rebuild/tests/welcome-chase-motion.test.mjs`. Tests simulate multiple
window sizes/frame rates, verify separation on every frame, check that wrapping
happens entirely offscreen, and complete from different chase/rest/resize states.

The same chase is selectable as “The elusive task” under General → A little play.
It runs continuously on Home and the other existing playful-animation surfaces.
Changing the selection or disabling play tears down its canvas and listeners.
The check-off ending remains exclusive to successful Welcome completion.

Completion lifts the task toward the center above the ready message, with the runner following a little behind. The flight lands after 2.1 seconds and celebrates for 1.7 seconds, then holds the finished scene still for 3 seconds to let the user read before navigation. The full-height canvas is used only for this finale; normal chasing stays at the bottom edge.

The final pair has an extra 40px of breathing room above the completion message. If completion starts on the right, both racers finish their exit and turn offscreen, then return from the right with the runner facing left behind the task. Card text stays upright.
