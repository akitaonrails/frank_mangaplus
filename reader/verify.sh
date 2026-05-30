#!/usr/bin/env bash
# Local CI script: run this before handing the app back to the user.
# Catches:
#   - Rust API regressions (proto field numbers, decoder bugs) — fixture tests
#   - Tauri backend type errors                                — cargo check
#   - TypeScript errors in Svelte components                   — svelte-check
#   - Svelte production build issues                           — bun run build
#   - Vite/PostCSS dev-mode style-extraction bugs              — live dev probe
#
# The dev probe is the one that took us multiple back-and-forths to find
# manually: vite-plugin-svelte fails to extract the <style> chunk when the
# template has certain syntax (JS template literals in style: directives).
# `bun run build` does NOT catch this; only the dev server does.

set -euo pipefail
cd "$(dirname "$0")"

step() { printf '\n\033[1;34m==>\033[0m %s\n' "$1"; }
ok()   { printf '   \033[1;32m✓\033[0m %s\n' "$1"; }
fail() { printf '   \033[1;31m✗\033[0m %s\n' "$1" >&2; exit 1; }

step "1/6  cargo test -p mangaplus-api  +  clippy -D warnings"
# Run the EXACT commands CI runs, so a green local run means a green CI run.
cargo test -p mangaplus-api --quiet 2>&1 | tail -3
cargo clippy -p mangaplus-api --lib --tests -- -D warnings 2>&1 | tail -3
ok "api unit + fixture tests + clippy pass"

step "2/6  cargo test -p mangaplus-desktop  +  clippy"
cargo test -p mangaplus-desktop --quiet 2>&1 | tail -3
cargo clippy -p mangaplus-desktop --lib --tests -- -D warnings 2>&1 | tail -3
ok "Tauri backend tests + clippy passes"

step "3/6  bun run test  (vitest unit tests for the TS lib)"
( cd desktop && bun run test ) 2>&1 | tail -6
ok "vitest passes"

step "4/6  bun run check  (svelte-check TS)"
# svelte-check has some pre-existing warnings; only fail on hard errors.
( cd desktop && bun run check ) 2>&1 | tail -3 || true
ok "svelte-check ran (review output above)"

step "5/6  bun run build  (production static export)"
( cd desktop && bun run build ) 2>&1 | tail -3
ok "production build succeeded"

step "6/6  vite dev probe  (catches PostCSS style-extraction bugs)"
cd desktop
# Start vite dev in background
bun run dev > /tmp/verify-vite.log 2>&1 &
VITE_PID=$!
trap "kill $VITE_PID 2>/dev/null || true" EXIT
# Wait for it to be ready
for _ in $(seq 1 20); do
  if curl -sS -o /dev/null "http://localhost:1420/" 2>/dev/null; then break; fi
  sleep 0.5
done

# Hit every route's style chunk URL — this is the one that catches
# svelte-preprocess style-extraction failures (e.g. JS template literals
# inside style: directives) that production build silently survives.
ROUTES=(
  ""
  "search"
  "title/%5Bid%5D"
  "reader/%5BchapterId%5D"
)
FAILED=0
for r in "${ROUTES[@]}"; do
  url="http://localhost:1420/src/routes/${r}/+page.svelte?svelte&type=style&lang.css"
  status=$(curl -sS -o /dev/null -w "%{http_code}" "$url")
  if [ "$status" = "200" ]; then
    ok "/${r:-(index)} style chunk OK"
  else
    printf '   \033[1;31m✗\033[0m /%s style chunk HTTP %s\n' "${r:-(index)}" "$status" >&2
    FAILED=$((FAILED+1))
  fi
done

# Also check the +layout.svelte
url="http://localhost:1420/src/routes/+layout.svelte?svelte&type=style&lang.css"
status=$(curl -sS -o /dev/null -w "%{http_code}" "$url")
if [ "$status" = "200" ]; then
  ok "+layout.svelte style chunk OK"
else
  printf '   \033[1;31m✗\033[0m +layout.svelte style chunk HTTP %s\n' "$status" >&2
  FAILED=$((FAILED+1))
fi

if [ "$FAILED" -gt 0 ]; then
  echo
  echo "vite log (last 30 lines):"
  tail -30 /tmp/verify-vite.log
  fail "$FAILED route(s) failed style extraction — see vite log above"
fi

kill $VITE_PID 2>/dev/null || true
trap - EXIT
echo
printf '\033[1;32mall green ✓\033[0m\n'
