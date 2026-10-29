# Contributing

Branch from `main`, install the prerequisites in [README.md](README.md), and run
`make setup`. The current application lives in `app/`; `legacy/` is reference only.

Run `make format` and `make check` before opening a PR. For frontend behavior,
also run `make e2e` against a running synthetic development server. Never use real
accounts, databases or tokens in tests. Docs build with `python3 docs/build.py`
after installing `docs/requirements.txt`; the public site builds with
`make www-build`.

Rust schema migrations live in `app/migrations/`. Add the next numbered migration;
never edit an already-released migration. Verify both fresh and existing databases
using synthetic fixtures. Production upgrades require a verified backup.

Use `cargo fmt` and Clippy; use Prettier for app JavaScript and CSS. Keep changes
focused and explain observed behavior and validation in PRs. CI runs on PRs;
release publishing is explicit and deployment is separate.

Report bugs with reproducible steps, expected/actual behavior, and browser/OS
when relevant. Report security issues privately as described in
[SECURITY.md](SECURITY.md). Follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
Contributions are licensed under [MIT](LICENSE); retain third-party notices.
