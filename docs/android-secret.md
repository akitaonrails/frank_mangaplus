# Extract your `deviceSecret` (rooted Android emulator)

You only need this if you're a **paying MANGA Plus subscriber** and want the subscription-locked chapters on desktop too. The free tier works without any of this — the app auto-registers on first launch.

If your daily phone is rooted, skip this whole page: `adb shell su -c "cat /data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml"` and grep for `<string name="secret">`.

This page covers the case where your phone *isn't* rooted, so we set up a throwaway Android emulator on the desktop and root that.

Takes about 20 minutes the first time, never again. Once the emulator is built and the secret is extracted, you can delete the emulator — the secret stays valid until the next time you "Restore Subscription" somewhere.

---

## 1. Install Android Studio and create an AVD

Get Android Studio from https://developer.android.com/studio. After install, open the SDK Manager and grab:

- **Android SDK Platform 33** (or newer)
- **Google Play system image** for `x86_64` — this is the one that includes Play Store. Without Play Store you can't install MANGA Plus from inside the emulator.
- **Android Emulator** (under SDK Tools)

Create a new AVD via Device Manager. Pick a phone profile (Pixel 6 is fine), select the Google Play system image you just downloaded, finish.

Boot the AVD once to make sure it works. Sign in to Google Play with the account that has your MANGA Plus subscription. **Do not install MANGA Plus yet.**

---

## 2. Root the AVD with rootAVD

Stock Google Play images aren't rooted. You can't `adb root` them. So you patch the boot image with Magisk.

Clone https://github.com/newbit1/rootAVD and follow its README. Short version:

```bash
git clone https://github.com/newbit1/rootAVD.git
cd rootAVD
./rootAVD.sh ListAllAVDs
# pick your AVD's ramdisk path from the output, then:
./rootAVD.sh <path-to-ramdisk.img>
```

It patches the AVD's ramdisk in place. Restart the emulator. Open the Magisk app — it should say "Installed". You now have root.

Verify:

```bash
adb shell su -c "id"
# uid=0(root) gid=0(root) ...
```

---

## 3. Install MANGA Plus and sign in

In the rooted AVD, open Play Store, search for **MANGA Plus by SHUEISHA**, install. Open the app, sign in with the same Google account, restore your subscription. Read at least one chapter so the app fully initialises — it has to call `/api/register` once to get a `deviceSecret`.

---

## 4. Extract the secret

From your host:

```bash
adb shell su -c "cat /data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml"
```

Look for:

```xml
<string name="secret">SOMELONGHEXVALUEHERE</string>
```

The value between the tags is what you want. It's a 32-character hex string. Copy it.

---

## 5. Paste it into FRANK

Open FRANK's Settings, paste the secret, hit Save. The app reloads and from then on it has your subscriber view of the catalog. You can shut down the AVD now.

Or, if you'd rather skip the dialog, launch FRANK with `MANGAPLUS_SECRET=<value>` in your shell environment. The env var takes precedence over the saved secret.

---

## When this breaks

If FRANK starts returning "Invalid Parameter" errors after you've hit "Restore Subscription" on your phone or elsewhere, the secret you extracted got invalidated server-side. Boot the AVD again, repeat steps 3–5 — you'll get a fresh secret.

If you ever want to drop back to the free tier, delete `~/.config/mangaplus-reader/secret` (Linux/macOS) or `%APPDATA%\mangaplus-reader\secret` (Windows) and relaunch. FRANK auto-registers a new free-tier device.
