# Mobile layout and section contents moves

Verified 2026-11-11 against an isolated development server on port 8197 and a
synthetic database. No browser checks target a real account/database.

## Layout contracts

- Capture uses a separate 44px close-action row. Its hit area never overlaps title
  or description inputs; dialog and page fit at 320, 390, 800 and 1280px.
- Comment labels retain screen-reader geometry inside the shared flex editor.
  Fresh and locally restored comment drafts fit without horizontal page overflow.
- Mobile list rows remove their extra horizontal inset, preserve 44px action
  targets and stack comment/subtask counts in one narrow column.
- Full task pages and embedded task panels use the same title/brief alignment for
  children and nested children. Phone descriptions/checklists use the full page
  gutter rather than the task heading's completion-control gutter.
- Empty editable list, space, task and subtask descriptions say **Add description**
  and open/focus the description editor. Read-only pages do not offer editing.
- Subtask menu mounting does not change row height or shift following content.

## Section contents move

The menu action leaves the source section in place and moves its task subtrees to
an optional destination section in the same or another list. Domain/HTTP tests
verify completed/hidden descendants, 60 top-level roots, stale snapshots, target
validation, permissions, access acknowledgment, CSRF, strict input and rollback
of earlier roots after a later-root failure. Browser tests verify all destination
types, source-section preservation and retry after a rejected apply.

The reviewed transaction and existing 500-affected-task bound are documented in
`docs/rebuild/section-moves.md`. File objects are preserved through the existing
subtree mover; this check does not claim an external object-storage transfer.

## Verification

- `make setup`, `make check`, `make test`: passed. Rust results: 345 library tests,
  7 CLI tests, 1 backup test, 2 S3 transport tests and 10 HTTP tests (365 passed).
  Nine existing opt-in probes remain ignored. Clippy, formatting, actual
  service-worker handlers and three mocked deployment-script tests passed.
- Chromium and Firefox: all 40 distinct cases across `mobile-content-layout`,
  `move-actions`, `stable-render` and `task-indicators` passed across the final
  targeted runs. Draft-restoration readiness and calendar fixture isolation were
  corrected in the test harness before the successful reruns; this is not a
  claim of one clean full browser-suite run.
- Another 21 cases passed across `entry-forms`, `mobile-dialog-centering`,
  `mobile-list-gestures`, `mobile-task-navigation`, `navigation-overflow`,
  `task-gestures` and `asset-budget`. Three existing Firefox touch-specific checks
  are skipped. Cold space loads measured 32 Chromium / 34 Firefox requests,
  23 scripts, no third-party font requests and zero icon transfer on reload.
- Phone/desktop screenshots were visually inspected for capture, list rows,
  subtask briefs and the comment editor.
- WebKit remains unverified: the Arch host lacks its older required shared
  libraries, and fetching the isolated official browser image failed. No host
  libraries or system configuration were changed.

Private logs and screenshots are under ignored `.local/rebuild/mobile-followup-*`.
This pre-release verification does not claim production deployment health; that
is checked separately against the published image and live HTTPS endpoints.
