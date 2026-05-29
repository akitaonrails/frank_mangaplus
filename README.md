# FRANK MANGA+

Personal-use desktop reader for [MANGA Plus by Shueisha](https://mangaplus.shueisha.co.jp/) — read your existing paid subscription on Linux, macOS, or Windows.

## Quick start

→ **[Install instructions for users](docs/install.md)**
→ **[Extract your `deviceSecret`](docs/android-secret.md)** (one-time setup)

## For contributors

→ **[Debugging the MANGA Plus API](docs/debugging.md)** — mitmproxy / Frida setup, request format, common pitfalls

## What's inside

- `reader/api/` — Rust async client crate (`reqwest` + `prost`). Pure library, no Tauri deps, fixture-tested.
- `reader/desktop/` — Tauri 2 + SvelteKit app that uses the api crate.
- `reader/docs/android-secret.md` *(legacy path; same as `docs/android-secret.md`)*.
- `reader/verify.sh` — local CI script: cargo tests + Tauri type-check + Svelte build + dev-server PostCSS probe.

## Status

Working: library, search, title detail with sortable + virtualized chapter list, page-fit reader with snap scroll + click-zone navigation + chapter auto-advance, persistent image cache, local read state.

## License

MIT.

## Disclaimer

This project is not affiliated with Shueisha or Manga Plus. It accesses the official API using a session token extracted from a MANGA Plus install owned by the user. It does not bypass any subscription, paywall, or DRM.
