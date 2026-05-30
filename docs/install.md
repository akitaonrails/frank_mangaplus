# Install — FRANK MANGA+

A desktop reader for [MANGA Plus](https://mangaplus.shueisha.co.jp/) that talks to the official API with your own subscriber session.

This is only useful if you already pay for a MANGA Plus subscription on a phone or tablet. It doesn't bypass the paywall. It borrows the session token from your existing install so the same content shows up on your desktop.

There are three steps, and you've already done one of them if you're a MANGA Plus subscriber.

---

## 1. Download

From the [Releases page](https://github.com/akitaonrails/frank_mangaplus/releases/latest):

| OS                  | File                                                    |
|---------------------|---------------------------------------------------------|
| Linux (AppImage)    | `mangaplus-reader_*_amd64.AppImage`                     |
| Linux (Arch / AUR)  | `yay -S mangaplus-reader-bin`                           |
| macOS (Apple Silicon)| `mangaplus-reader_*_aarch64.dmg`                       |
| macOS (Intel)       | `mangaplus-reader_*_x64.dmg`                            |
| Windows             | `mangaplus-reader_*_x64-setup.exe`                      |

The Windows installer isn't code-signed. SmartScreen will probably warn the first time. Click "More info" then "Run anyway".

---

## 2. Get your deviceSecret

The app needs one private string, your `deviceSecret`. It's a 32-character hex token the MANGA Plus servers issued to your existing install. They use it to recognize you as a paying subscriber.

You can't generate one. You have to fish it out of an install you already own. Path depends on whether your phone is rooted.

**If your phone is rooted (Magisk):**

```bash
adb shell su -c "cat /data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml"
```

Look in the output for:

```xml
<string name="secret">SOMELONGVALUEHERE</string>
```

The value between the tags is what you need.

**If your phone isn't rooted:**

Set up a desktop Android emulator. Takes about 20 minutes the first time and you never have to do it again. Full walkthrough in [docs/android-secret.md](android-secret.md). Rough shape: Android Studio, the Google Play x86_64 system image, root it with [rootAVD](https://github.com/newbit1/rootAVD), sign into Google Play with the account that has your subscription, install MANGA Plus from the Play Store. Then the `adb shell` command above.

This is what I use. It doesn't touch the daily phone.

---

## 3. Paste it into the app

Launch the binary from step 1. A setup dialog asks for your `deviceSecret`. Paste it in, hit Save, you're done.

The app saves it to a config file. On Linux and macOS that's `~/.config/mangaplus-reader/secret`. On Windows it's `%APPDATA%\mangaplus-reader\secret`. Treat that file like a password: anyone who reads it can use your subscription.

After save the app reloads, and from then on it'll just open straight to your library.

> If you'd rather skip the dialog and keep the secret in your shell environment instead, set `MANGAPLUS_SECRET=...` before launching. The env var takes precedence over the config file when both are set. This is mostly useful for launching from CI or a per-project script. Most people should just use the dialog.

---

## When to re-extract

If the app starts returning "Invalid Parameter" and you've recently tapped "Restore Subscription" somewhere else, your old secret was probably invalidated. Go back to step 2 and repeat. The dialog accepts a new value at any time from the same screen.

---

## What this isn't

It isn't a way around the paywall. The API rejects every endpoint without a valid subscriber secret, so there is no "free mode" to fall back to. Buying or restoring subscriptions still happens through the official app and Google Play. There's no offline mode beyond the image cache the app keeps under `~/.cache/mangaplus-reader/`.
