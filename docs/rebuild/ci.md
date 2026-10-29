# Checks and releases

PR checks use GitHub-hosted runners, read-only repository permissions and synthetic
data. They run formatting, Clippy, Rust tests, service-worker checks, builds,
docs/site builds, CLI checks and current Chromium/Firefox smoke, capture/loading and network-recovery
scenarios. The broader `make e2e` suite is retained for maintenance: some older
scenarios still target retired selectors and are not the PR gate. Fork PRs receive no
production secrets or private-runner access.

Ordinary main pushes run nothing unless `docs/**` changed; then only documentation
publishes. Cloudflare website publishing uses `workflow_dispatch`, exposed through
`make www-publish` and `make www-deploy`. The VPS is never deployed by CI.

## App releases

`make release name=vX.Y.Z` creates a GitHub release from clean, current main.
Only a published release starts the image publishing workflow. The job validates
the tag and main ancestry, runs application checks, then pushes the explicit
version to GHCR. Tag pushes and draft releases do not publish images.

## CLI releases

`make cli-release name=vX.Y.Z` creates `cli-vX.Y.Z`. The release builds Linux
amd64/arm64, macOS Intel/Apple Silicon and Windows amd64 executables and attaches
each with a SHA-256 sidecar. Linux builds require glibc: amd64 2.35+, arm64 2.39+.
CLI releases do not build/push app images or replace the app's latest GitHub release.

## Hosting configuration

Configure GitHub Pages to use Actions and the custom domain docs.todoer.pro.
Configure Cloudflare Pages as Direct Upload with production branch main; add
CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID secrets to the cloudflare-pages
environment. Optional variable CLOUDFLARE_PAGES_PROJECT overrides todoer-www.
Set GHCR package visibility separately after its first publish for anonymous pulls.
See the repository README and deployment runbook for setup and recovery.
