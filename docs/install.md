# Install — FRANK MANGA+

A personal-use desktop reader for [MANGA Plus by Shueisha](https://mangaplus.shueisha.co.jp/) that talks to the official API with your own subscriber session.

> **Heads up.** This is **only useful if you already pay for MANGA Plus on a phone or tablet**. It does not bypass the subscription — it reuses the session token from your existing install so you can read the same content on a desktop screen.

---

## 1. Download a release

Grab the bundle for your OS from the [Releases page](https://github.com/akitaonrails/frank_mangaplus/releases/latest):

| OS                  | File                                                    |
|---------------------|---------------------------------------------------------|
| Linux (AppImage)    | `mangaplus-reader_*_amd64.AppImage`                     |
| Linux (Arch / AUR)  | `yay -S mangaplus-reader-bin`                           |
| macOS (Apple Silicon)| `mangaplus-reader_*_aarch64.dmg`                       |
| macOS (Intel)       | `mangaplus-reader_*_x64.dmg`                            |
| Windows             | `mangaplus-reader_*_x64-setup.exe`                      |

On Windows the installer is **not** code-signed; SmartScreen may show a "Windows protected your PC" dialog the first time — click **More info → Run anyway**.

---

## 2. Get your `deviceSecret`

The reader needs one private string — your `deviceSecret`. It's a 32-character hex token that the MANGA Plus servers issue to each install and use to identify a paying subscriber.

You have to extract it from a MANGA Plus install you already use. There are two practical ways:

### Option A — your real phone (easiest if rooted)

Works on a phone with **root access** (Magisk, etc.):

```bash
adb shell su -c "cat /data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml"
```

Find the line:

```xml
<string name="secret">SOMELONGVALUEHERE</string>
```

The value between the tags is your `deviceSecret`.

### Option B — a rooted Android emulator (works without root on your phone)

If your phone is not rooted, use a desktop Android emulator. Full step-by-step is in **[`docs/android-secret.md`](android-secret.md)** — rough outline:

1. Install Android Studio + the **Google Play** `x86_64` system image (Android 16 / API 36 works)
2. Create an AVD, boot once with `-writable-system`
3. Root it via [rootAVD](https://github.com/newbit1/rootAVD) (installs Magisk)
4. Sign into Google Play with the account that holds your MANGA Plus subscription
5. Install MANGA Plus from Play Store, let it bind to your subscription
6. `adb shell su -c "cat /data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml"`

Same XML, same `<string name="secret">…</string>` line.

This is what the project maintainer uses — it doesn't touch your daily-driver phone.

### Option C — capture from network traffic

If you can MITM proxy your phone's traffic, every API request URL contains `secret=…` as a query parameter. See [`docs/debugging.md`](debugging.md) — it's the same mitmproxy setup, just used to read instead of debug.

---

## 3. Tell the app

Two ways, pick one:

**A. Environment variable.** Set `MANGAPLUS_SECRET` in your shell:

```bash
# in ~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish, etc.
export MANGAPLUS_SECRET="paste_your_value_here"
```

Then launch the app from a terminal so it inherits the variable.

**B. Paste into the app.** Launch the app with no `MANGAPLUS_SECRET` set; it'll show a setup dialog. Paste your value there. The app writes it to:

- **Linux/macOS:** `~/.config/mangaplus-reader/secret`
- **Windows:** `%APPDATA%\mangaplus-reader\secret`

That file is plain text containing exactly the secret. Treat it like a password — it gives full access to your MANGA Plus account's content.

The env variable takes precedence; the file is the fallback.

---

## 4. Read

That's it. The app talks to `jumpg-api.tokyo-cdn.com` directly — nothing self-hosted, no third party in the middle.

Open a title, tap a chapter, scroll. Pages cache to `~/.cache/mangaplus-reader/` so re-opening is instant.

---

## When to re-extract your secret

If the app starts returning `Invalid Parameter` errors and `Restore Subscription` re-bound your subscription to a new device in the meantime, your old secret was invalidated. Repeat **step 2** to get the new one.

---

## What this app does NOT do

- Buy or restore subscriptions (you need the official app + Google Play for that)
- Work without a valid paid subscription on your account (there is no free tier in the API)
- Save chapters offline beyond the runtime image cache
- Mirror or redistribute content
