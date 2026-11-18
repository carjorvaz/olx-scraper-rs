# AGENTS.md

A map, not a manual. Keep this file short; the README, the flake and the code are the
deeper sources.

## What this is

A small batch notifier: fetch OLX search pages, compare listing URLs against a plain-text
seen set, post new listings to Discord webhooks. It runs from a systemd timer; it is not
a server and has no UI.

## Commands

- `direnv allow` or `nix develop` — toolchain.
- `just check` — format, lint and test.
- `cargo test` — unit and integration tests (offline; a local HTTP server plays Discord).
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `nix build` — the package; tests run as part of the build.

## Layout

- `src/lib.rs` — the run loop and per-page dedupe.
- `src/olx.rs` — HTTP fetch and card extraction (selectors, price/location quirks).
- `src/discord.rs` — webhook payload and POST.
- `src/state.rs` — load and atomically save the seen set.
- `src/config.rs` — webhook/URL group parsing.
- `tests/cli.rs` — end-to-end: fixture page to webhook to state.

## Invariants

- Config and state formats stay compatible with `cl-olx-scraper`: groups separated by
  blank lines; one URL per line in `seen-urls.txt`.
- The state is written atomically (temp file, fsync, rename) after every group. Never
  write `seen-urls.txt` in place.
- A listing is marked seen only after its Discord POST succeeds, so a failed run retries
  it; a scrape error for one search URL does not abort the rest, and a failed webhook
  stops only its own group (the run exits non-zero).
- Discord payloads are clipped to Discord's limits; one oversized listing must not block
  the run.
- One notification per URL per run, even when the same ad appears twice on a page.
- `SEED_ONLY=1` records listings without posting.
- No async runtime, no web framework, no database, no CLI parser. Four runtime deps.
- Keep it small enough to read in one sitting.

## Style

- Subtractive: prefer deleting a line or a layer to adding one. No abstraction until a
  cut point is obvious; simple duplication beats a clever helper.
- Locality of behavior: parsing quirks stay next to the parsing, the payload next to the
  POST, the state rules next to the file.
- Errors say what failed and what to do about it (see `CONFIG_FILE` and `state::save`).
- Tests are the feedback loop; a bug fix gets a regression test.

## Do not

- No web UI. If one ever becomes necessary, it is server-rendered HTML (htmx if
  interactivity is needed), not a SPA and not a JavaScript build step.
- No database or config crate; plain text and environment variables are the design.
- No state-format change without a migration.
- No tests against Discord; use a local server.

## Sources

- `README.md` — usage, config format, NixOS module.
- `flake.nix` — package, dev shell, module.
