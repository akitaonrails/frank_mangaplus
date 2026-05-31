# Troubleshooting

Mostly Linux rendering issues — that's where the surprises live.

If anything else breaks (login, library not loading, etc.), the long-form story is in [`docs/install.md`](install.md); for chapter-level reading bugs, file an issue.

---

## "The app opens but the window is blank / white"

WebKitGTK uses several rendering paths and any of them can fail on the right (wrong) combination of Wayland compositor, GPU driver, and Mesa version. The binary tries to pick the right path automatically, but the catch-all when it can't figure things out is to fall back to **safe mode** (full CPU rendering, no GPU). Most users never see this.

### How the binary decides what to do

On every launch, before WebKit gets to do anything, the binary:

1. Reads `MANGAPLUS_RENDER_MODE` from the environment.
2. Reads `~/.config/mangaplus-reader/render.conf` for a `mode = ...` line.
3. Checks for a "previous launch died mid-render" marker at `~/.config/mangaplus-reader/render-recovery`.
4. Reads the GPU vendor from `/sys/class/drm/card*/device/vendor`.
5. Looks at `$WAYLAND_DISPLAY` / `$DISPLAY` to figure out the display server.

Then it picks **one** of these modes:

| Mode | What it does | When auto-detect picks it |
|---|---|---|
| `native` | No env vars set, full GPU, WebKit defaults | X11 sessions |
| `dmabuf-off` | `WEBKIT_DISABLE_DMABUF_RENDERER=1` only; still GPU compositing via SHM | Wayland + AMD or Intel |
| `nvidia-light` | `WEBKIT_DISABLE_DMABUF_RENDERER=1` + `__NV_DISABLE_EXPLICIT_SYNC=1`; still GPU | Wayland + NVIDIA |
| `safe` | `WEBKIT_DISABLE_DMABUF_RENDERER=1` + `WEBKIT_DISABLE_COMPOSITING_MODE=1`; full CPU rendering | Crash recovery (last launch failed) |

Whatever it picked, you can `cat ~/.config/mangaplus-reader/render-state.log` to see exactly what was applied:

```
# Last-launch render policy snapshot.
mode = nvidia-light
reason = NVIDIA on Wayland: disable DMA-BUF + explicit sync, keep GPU compositing
display = Wayland
gpu = Nvidia
overridden_by_user = false
recovery_active = false
applied_env =
  WEBKIT_DISABLE_DMABUF_RENDERER=1
  __NV_DISABLE_EXPLICIT_SYNC=1
```

### The auto-recovery loop

When the binary starts, it touches `~/.config/mangaplus-reader/render-recovery`. As soon as the WebView's first frame paints, the frontend tells the backend to delete that marker. So:

- **WebView painted successfully** → marker deleted → next launch detects nothing wrong → uses the auto-detected mode.
- **WebView aborted before painting** (the blank-screen case) → marker stays → next launch sees the marker and falls back to `safe` mode automatically. You should see a working app on the second launch.

This means most blank-screen reports fix themselves: relaunch the app. If `safe` mode also fails, the issue is below WebKit (compositor crash, glibc mismatch on the bundled AppImage, etc.).

### Manual overrides

You usually don't need these — only if the auto-recovery doesn't work or you want to force a specific mode.

**Quick test (current shell only):**

```bash
MANGAPLUS_RENDER_MODE=safe ./FRANK.MANGA+_*_amd64.AppImage
# Try each in turn until the window paints:
MANGAPLUS_RENDER_MODE=native      # full GPU, WebKit defaults
MANGAPLUS_RENDER_MODE=dmabuf-off  # disable DMA-BUF, keep SHM GPU
MANGAPLUS_RENDER_MODE=nvidia-light
MANGAPLUS_RENDER_MODE=safe        # CPU rendering, slowest but most compatible
MANGAPLUS_RENDER_MODE=auto        # explicitly fall through to auto-detect
```

**Persistent (every launch):**

Write a config file at `~/.config/mangaplus-reader/render.conf`:

```
# One of: native, dmabuf-off, nvidia-light, safe, auto
mode = safe
```

The env var wins if both are set.

### Common scenarios

- **"Window is blank but only on first launch after install"** — that's the auto-recovery doing its job. Relaunch; the second launch reads the marker and switches to safe mode. If you want to skip this dance, set `mode = safe` in the config file.
- **"Page-flip animation is sluggish"** — you're probably in `safe` mode. Try `MANGAPLUS_RENDER_MODE=dmabuf-off` or `nvidia-light` depending on your GPU. If those crash, you're stuck on `safe`.
- **"It worked yesterday and now the window is blank"** — Mesa or kernel update? Try `MANGAPLUS_RENDER_MODE=safe`. If that works, the auto-detect heuristic became wrong for your system; please file an issue with the contents of `render-state.log`.
- **"NVIDIA + Wayland and still blank"** — try `MANGAPLUS_RENDER_MODE=safe`. The auto path for NVIDIA is light-touch and doesn't cover every NVIDIA driver version.

### If `safe` doesn't even work

Last-resort env vars worth trying (set before launch):

```bash
# Force XWayland instead of native Wayland
GDK_BACKEND=x11 MANGAPLUS_RENDER_MODE=safe ./FRANK.MANGA+_*_amd64.AppImage

# Disable WebKit's process sandbox (some sandboxed envs block EGL)
WEBKIT_FORCE_SANDBOX=0 MANGAPLUS_RENDER_MODE=safe ./FRANK.MANGA+_*_amd64.AppImage

# Full software OpenGL — nuclear option
LIBGL_ALWAYS_SOFTWARE=1 MANGAPLUS_RENDER_MODE=safe ./FRANK.MANGA+_*_amd64.AppImage
```

If none of these get a window painting, the bug isn't in our binary — the underlying compositor/driver/glibc combination isn't getting an EGL display at all. File an issue with `inxi -G` output, your `render-state.log`, and the launch console output.

### Reading the recovery marker yourself

The marker is just a file. If you ever want to force the next launch into `safe` mode (e.g., before trying a risky compositor change), `touch ~/.config/mangaplus-reader/render-recovery`. To force the next launch back to auto-detect, `rm` it.

---

## "I can read free chapters but my subscription content is locked"

The auto-registered free-tier secret only has access to free content. See [`docs/install.md`](install.md) for the subscriber path: extract your phone's `deviceSecret` and paste it into the in-app dialog. Detailed walkthrough in [`docs/android-secret.md`](android-secret.md).

---

## "The chapter list shows the wrong order / weird reading order"

The MANGA Plus API occasionally reassigns chapter IDs in non-monotonic ways (e.g., Kaiju No. 8 has chapters where `#125`'s id is *lower* than `#124`'s). The reader follows the publication order returned by `title_detail.chapter_list_v2` — that order is correct even when IDs aren't. If you see a chapter advancing to the wrong next chapter, capture the title id + chapter name and file an issue.
