# Install — FRANK MANGA+

A desktop reader for [MANGA Plus](https://mangaplus.shueisha.co.jp/) that talks to the official API.

There are two ways to use it.

**Free-tier (default)** — install the binary, launch it. The app registers itself as a fresh device on first launch and gives you access to everything MANGA Plus offers for free: the entire catalog, the latest and first few chapters of every series, and the free-rotation backlog. No phone, no ADB, no rooting. One click and you're reading.

**Paid subscriber (optional)** — if you already pay for MANGA Plus on a phone and want the subscription-locked chapters on desktop too, paste your existing `deviceSecret` into the app's settings. That replaces the free-tier secret with your subscriber one. Getting the secret out of your phone takes 5–20 minutes the first time and is covered below.

---

## 1. Download

From the [Releases page](https://github.com/akitaonrails/frank_mangaplus/releases/latest):

| OS                  | File                                                    |
|---------------------|---------------------------------------------------------|
| Linux (AppImage)    | `FRANK.MANGA+_*_amd64.AppImage`                         |
| Linux (.deb)        | `FRANK.MANGA+_*_amd64.deb`                              |
| Linux (.rpm)        | `FRANK.MANGA+-*-1.x86_64.rpm`                           |
| Linux (Arch / AUR)  | `yay -S mangaplus-reader-bin`                           |
| macOS (Apple Silicon)| `FRANK.MANGA+_*_aarch64.dmg`                           |
| macOS (Intel)       | `FRANK.MANGA+_*_x64.dmg`                                |
| Windows installer   | `FRANK.MANGA+_*_x64-setup.exe`                          |
| Windows MSI         | `FRANK.MANGA+_*_x64_en-US.msi`                          |

The Windows installer isn't code-signed. SmartScreen will probably warn the first time. Click "More info" then "Run anyway".

---

## 2. Launch — free tier just works

Run the binary. On first launch with no secret configured, the app calls the official `/register` endpoint and is issued a fresh `deviceSecret`. That secret is saved to a config file so it's reused on every later launch.

| OS | Where the secret file lives |
|---|---|
| Linux & macOS | `~/.config/mangaplus-reader/secret` |
| Windows | `%APPDATA%\mangaplus-reader\secret` |

Treat that file like a password — it grants whatever access the secret has.

That's the whole flow if you only want free content. Skip the rest of this page.

---

## 3. Upgrade to your subscription (optional)

If you pay for MANGA Plus on a phone and want the subscription-locked chapters on desktop too, you need to replace the free-tier `deviceSecret` with your existing subscriber one. The subscriber secret lives in your phone install — there's no way to migrate the subscription itself, just the token it's tied to.

Extract from your phone. Path depends on whether it's rooted.

**Rooted phone (Magisk):**

```bash
adb shell su -c "cat /data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml"
```

Look in the output for:

```xml
<string name="secret">SOMELONGVALUEHERE</string>
```

That value is what you need.

**Non-rooted phone:**

Set up a rooted Android emulator on your desktop. About 20 minutes the first time, never again. Full walkthrough in [docs/android-secret.md](android-secret.md). Rough shape: Android Studio + Play Store image, root it with [rootAVD](https://github.com/newbit1/rootAVD), sign into Google Play with your subscription account, install MANGA Plus from the Play Store, then the `adb shell` command above.

This is what I use. It doesn't touch the daily phone.

**Paste it into the app:**

Open Settings, paste the secret, hit Save. The app reloads and from then on it has your subscriber view of the catalog.

> If you'd rather keep the secret in your shell environment instead, set `MANGAPLUS_SECRET=...` before launching. The env var takes precedence over the config file when both are set. This is mostly useful for launching from CI or a per-project script.

---

## When to re-extract

If the app starts returning "Invalid Parameter" after you've recently hit "Restore Subscription" somewhere else, your old secret was probably invalidated. Repeat step 3 with a fresh extraction. The settings screen accepts a new value at any time.

If you want to go back to the free tier, delete the secret file and launch again — the app will auto-register a new free-tier device.

---

## What this isn't

It isn't a way around the paywall. The free tier on desktop is the same free tier the official phone app gives you on first install: latest and first chapters of every series, no subscription-locked content. Paying for subscription content still happens through Google Play on the official app — there is no way to unlock subscription chapters on desktop without using a secret extracted from an already-paid phone install. The app has no offline mode beyond an image cache under `~/.cache/mangaplus-reader/`.
