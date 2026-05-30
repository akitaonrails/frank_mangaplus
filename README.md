<div align="center">
  <img src="docs/logo.png" width="320" alt="FRANK MANGA+" />

  <h1>FRANK MANGA+</h1>

  <p>
    <strong>A personal-use desktop reader for <a href="https://mangaplus.shueisha.co.jp/">MANGA Plus by Shueisha</a>.</strong><br>
    Read your existing paid subscription on Linux, macOS, and Windows.
  </p>

  <p>
    <a href="https://github.com/akitaonrails/frank_mangaplus/releases/latest">Latest release</a>
    ·
    <a href="docs/install.md">Install guide</a>
    ·
    <a href="docs/android-secret.md">Get your secret</a>
    ·
    <a href="docs/debugging.md">Contributors</a>
  </p>

  <p>
    <a href="https://github.com/akitaonrails/frank_mangaplus/actions/workflows/ci.yml"><img src="https://github.com/akitaonrails/frank_mangaplus/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://github.com/akitaonrails/frank_mangaplus/releases"><img src="https://img.shields.io/github/v/release/akitaonrails/frank_mangaplus?include_prereleases&label=release" alt="Release"></a>
    <img alt="Platforms" src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-blue">
    <img alt="License" src="https://img.shields.io/badge/license-MIT-green">
  </p>
</div>

---

## What it looks like

| Library / Search | Title detail | Reader |
|:---:|:---:|:---:|
| ![Library and full catalog search](docs/screenshots/library.png) | ![Title detail with chapter list](docs/screenshots/title-detail.png) | ![Page-fit reader with snap scroll](docs/screenshots/reader.png) |
| Your bookmarked titles, plus the full catalog if you want to browse. | Banner art, synopsis, the chapter list (virtualized — One Piece has 1100+ and it still scrolls fine). | One page at a time, snap scrolling. Click the bottom half of the page to advance. |

---

## Why this exists

I pay for MANGA Plus and I wanted to read it on a desktop. Shueisha doesn't ship a desktop client. So you have two choices: sideload the Android app onto a tablet (works, but a tablet is a tablet), or squint at your phone. This is option three.

Under the hood it's the same API the official Android app uses, with the same session token your existing install already has. It does not bypass the subscription — you're still paying Shueisha for the actual content. All it does is move the rendering surface to a screen that has a keyboard attached.

## What you need

An active MANGA Plus subscription. The API has no free tier; without a valid `deviceSecret` every endpoint returns "Invalid Parameter".

Then 5 to 10 minutes the first time, to get that secret out of an install you already have. Two paths:

- If your phone is rooted, one `adb shell` away.
- If it isn't, you set up a rooted Android emulator (Magisk + Play Store). Takes about 20 minutes the first time, never again after that. Walkthrough is in [docs/android-secret.md](docs/android-secret.md).

## Install

Grab the build for your OS from the [Releases page](https://github.com/akitaonrails/frank_mangaplus/releases/latest):

| OS | File |
|---|---|
| Linux (AppImage) | `FRANK.MANGA+_*_amd64.AppImage` |
| Linux (.deb) | `FRANK.MANGA+_*_amd64.deb` |
| Linux (Arch) | `yay -S mangaplus-reader-bin` |
| macOS (Apple Silicon) | `FRANK.MANGA+_*_aarch64.dmg` |
| macOS (Intel) | `FRANK.MANGA+_*_x64.dmg` |
| Windows | `FRANK.MANGA+_*_x64-setup.exe` |

Long-form install doc: [docs/install.md](docs/install.md).

On first launch the app shows a setup dialog asking for your `deviceSecret`. Paste it, save, done.

## What's in it

A library view of your bookmarked titles. The search page hits the full English catalog and filters locally as you type.

Title detail shows the banner art, the synopsis, and the full chapter list. The list is virtualized, so a series with a thousand chapters scrolls fine. There's a sort toggle, and a "Continue ▶" button that jumps to the last chapter you opened.

The reader is page-fit and snap-scrolls. Click the bottom of a page to go forward, the top to go back. Keyboard works too (Space, arrows, PgUp/Dn). When you reach the end of a chapter the next one pre-fetches and appends to the scroll, so you don't have to bounce back to the chapter list every time.

Every page you load gets cached to `~/.cache/mangaplus-reader/`. Re-opening the same chapter is instant after the first read.

Read state lives in localStorage. The chapter list marks whichever chapter you stopped at with a "Last opened" badge, and the title page surfaces a link back to it.

## How it works

```
┌─────────────────────────────┐      ┌──────────────────────────┐
│  Tauri WebView (SvelteKit)  │      │  Rust client (reqwest)   │
│                             │      │                          │
│  Library / Search / Reader  │◄────►│  get_chapter_pages…      │
│  <img src="mpimg://…">      │      │  get_title_detail…       │
└──────────┬──────────────────┘      │  fetch_image (cookies,   │
           │                         │       okhttp UA, cache)  │
           │ mpimg:// scheme         └────────────┬─────────────┘
           │ intercepted by Tauri                 │
           └─────────────────────────────────────►│ HTTPS
                                                  ▼
                                       jumpg-api.tokyo-cdn.com
                                       jumpg-assets3.tokyo-cdn.com
```

`mangaplus-api` is a pure Rust crate with fixture-based tests. It does the protobuf decode, the cookie threading, and the `plus_vw_token` handshake the premium image CDN expects. No Tauri deps.

`mangaplus-desktop` is the Tauri 2 + SvelteKit app. Image URLs go through an `mpimg://` custom URI scheme that proxies the request back through the same Rust client. That's how the cookies survive the WebView/Rust boundary.

Every request goes directly to Shueisha's CDN, no proxy in between.

If you don't trust a random binary on the internet to do that honestly (you shouldn't), the actual on-the-wire format — every header, every gotcha I hit reverse-engineering this — is written up in [docs/debugging.md](docs/debugging.md). You can verify it yourself.

## Documentation

- [`docs/install.md`](docs/install.md): end-user install. Download, get a `deviceSecret`, set the env var or paste it in.
- [`docs/android-secret.md`](docs/android-secret.md): the rooted-AVD walkthrough. Doesn't touch your phone.
- [`docs/debugging.md`](docs/debugging.md): contributor notes. mitmproxy, Frida, the real headers, what tripped me up.

## Disclaimer

Not affiliated with Shueisha or Manga Plus. It uses a session token extracted from an install you own to talk to the official API. It does not bypass any subscription, paywall, or DRM. **You have to already pay for a MANGA Plus subscription for this to do anything useful.**

## License

MIT.
