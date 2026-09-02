# AGENTS.md

## Repo shape

- Real development lives under `reader/`; root also contains reverse-engineering artifacts (`jadx_output/`, `rootAVD/`, `xapk_extracted/`, the XAPK) that are not normal app entrypoints.
- `reader/Cargo.toml` is the Rust workspace: `api` (`mangaplus-api`) plus `desktop/src-tauri` (`mangaplus-desktop`).
- `reader/api` is the pure Rust MANGA Plus client: protobuf decode, cookies, image fetch/cache, fixture tests. Keep it free of Tauri dependencies.
- `reader/desktop` is the Tauri 2 + SvelteKit/Svelte 5 app. It is static SPA mode (`adapter-static` fallback, `src/routes/+layout.ts` has `ssr = false`).
- Rust app entrypoint is `reader/desktop/src-tauri/src/main.rs` -> `mangaplus_desktop_lib::run()`; most Tauri commands and `mpimg://` wiring are in `src-tauri/src/lib.rs`.

## Commands

- Install frontend deps from `reader/desktop`: `bun install --frozen-lockfile`.
- Frontend dev server from `reader/desktop`: `bun run dev` (Vite strict port `1420`).
- Tauri dev app from `reader/desktop`: `bun run tauri dev`.
- Frontend checks from `reader/desktop`: `bun run check`, `bun run test`, `bun run build`.
- Focused TS test example from `reader/desktop`: `bun run test -- src/lib/img.test.ts`.
- Rust tests from `reader`: `cargo test -p mangaplus-api` and `cargo test -p mangaplus-desktop`.
- Focused Rust fixture test example from `reader`: `cargo test -p mangaplus-api bookmark_returns_subscribed_titles_view`.
- Rust clippy commands from `reader`: `cargo clippy -p mangaplus-api --lib --tests -- -D warnings` and `cargo clippy -p mangaplus-desktop --lib --tests -- -D warnings`.
- `cargo fmt --check` is warn-only in CI because no committed rustfmt config exists yet.
- Local full-ish verification from `reader`: `./verify.sh`; it also probes Vite dev CSS chunks that `bun run build` can miss.
- Release binary from `reader/desktop`: `bun run tauri build --no-bundle` (drop the flag to also produce deb/rpm/AppImage).
- No frontend lint script/config was found; do not invent `bun run lint`.

## Build/test prerequisites

- CI uses Rust stable with `rustfmt`/`clippy`, Bun latest, and `protoc` 27.x.
- Linux Tauri builds need WebKitGTK/Tauri system libs: `libwebkit2gtk-4.1-dev`, `librsvg2-dev`, `patchelf`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libjavascriptcoregtk-4.1-dev`.

## Gotchas

- Do not edit generated protobuf output; edit `reader/api/proto/mangaplus.proto`. `reader/api/build.rs` runs `prost-build` and derives serde camelCase for Tauri IPC/cache JSON.
- MANGA Plus request details are brittle: `app_ver` is pinned to `250`, default `os_ver` is `36`, cookies must be enabled, and the User-Agent must look like `okhttp/4.12.0`.
- Premium image fetches require the `plus_vw_token` cookie set by `manga_viewer_v3`; browser `<img>` requests must go through the Tauri `mpimg://` protocol so Rust can send the cookie and UA.
- Use `src/lib/img.ts` `proxied(...)` for CDN image URLs. Do not rename it to `img`; that name previously broke Svelte 5 style extraction/PostCSS in dev mode.
- `MANGAPLUS_SECRET` overrides the on-disk secret. Without an env/config secret, startup auto-registers a free-tier device and writes local config under the platform config dir.
- Image/cache data lives under XDG-style cache dirs such as `~/.cache/mangaplus-reader/`; local secret/render config lives under `~/.config/mangaplus-reader/` on Linux.
- The full-catalog SWR cache (`all_titles_<lang>_<clang>.bin` + `.meta.json` in the cache dir) defaults to a 24h TTL; `MANGAPLUS_CATALOG_TTL_HOURS=N` overrides it.
- Linux render mode is decided before WebKit starts: `MANGAPLUS_RENDER_MODE` wins, then `~/.config/mangaplus-reader/render.conf`, then crash-recovery marker, then GPU/display auto-detect. See `docs/troubleshooting.md` before changing this path.
- For UI/Svelte route changes, run a dev-server check or `./verify.sh`; production build alone may not catch style-chunk extraction failures.
- Build release binaries with `bun run tauri build --no-bundle`, never bare `cargo build --release`. Only the Tauri CLI sets the env its codegen reads, so a plain cargo release build silently embeds `devUrl` (`http://localhost:1420`) instead of `frontendDist` — the binary compiles and runs, then fails to connect at launch unless `bun run dev` happens to be up. Check a suspect binary with `strings target/release/mangaplus-desktop | grep -c _app/immutable`: nonzero means the frontend is embedded, 0 means it is not.
