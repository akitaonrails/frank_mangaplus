# Extracting your MANGA Plus `deviceSecret`

The desktop reader uses the official MANGA Plus API at
`https://jumpg-api.tokyo-cdn.com/api/`. Every request includes a `?secret=...`
query parameter that the server uses to identify a subscriber. That secret
("`deviceSecret`") is generated server-side on first launch of the Android app
and then bound to your paid subscription via Google Play.

You only need to do this **once**. After capture, store the secret in your
shell environment or password manager and the desktop reader uses it forever
(unless Shueisha rotates secrets, which is not observed in app v2.3.0).

---

## TL;DR

1. Create a Google-Play-enabled Android emulator (AVD)
2. Root it with [rootAVD](https://github.com/newbit1/rootAVD) (Magisk)
3. Install MANGA Plus from Play Store inside it
4. Sign in with your Google account; subscription auto-restores
5. `adb shell su -c cat /data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml`
6. Copy the `<string name="secret">…</string>` value

---

## Why this is necessary

The MANGA Plus app stores `deviceSecret` in its private SharedPreferences file
(`/data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml`). Android
sandboxes app data, so reading another app's private dir requires **root**.

Without root, the obvious workarounds also fail:

| Approach | Why it fails on a modern (Android 12+) install |
|---|---|
| `adb backup` | Manifest has `android:allowBackup="false"` |
| `adb run-as` | App is not `debuggable` in production builds |
| mitmproxy with user-installed CA | App targets `targetSdkVersion=36` and has no `networkSecurityConfig` opting into user-CA trust |
| mitmproxy with the OneTrust SDK | OneTrust SDK does its own pinning; banner-init fails behind a proxy |

The reliable path is a **rooted Android Virtual Device (AVD)** with **Google Play
installed**, so we can both extract the secret (root) *and* satisfy Google Play
Billing's "installed via Play Store" check (so subscription restore works).

---

## Prerequisites

- Linux desktop with KVM enabled (`/dev/kvm` accessible to your user)
- Disk: ~10 GB free (system image + AVD data)
- RAM: ~6 GB available while the AVD runs
- The Google account that has your active MANGA Plus subscription
- A working internet connection

Installed tools:

- **Android Studio** (provides the SDK + emulator + `adbmanager` + `sdkmanager`)
- `git`, `openssl`, `bash`

On Arch:

```sh
sudo pacman -S --needed android-tools openssl git
yay -S android-studio   # or download from developer.android.com
```

Verify after install:

```sh
ls /dev/kvm                                           # exists and readable by you
groups | grep kvm                                     # your user in kvm group
~/Android/Sdk/cmdline-tools/latest/bin/sdkmanager --list_installed
~/Android/Sdk/emulator/emulator -version
```

---

## Step 1 — Download the Google Play system image

We need `system-images;android-36;google_apis_playstore;x86_64`. It's the only
image variant that ships with the actual Play Store app **and** can be rooted
via rootAVD.

```sh
~/Android/Sdk/cmdline-tools/latest/bin/sdkmanager \
  "system-images;android-36;google_apis_playstore;x86_64"
```

Accept the license (`y`). ~4.4 GB download.

---

## Step 2 — Clone rootAVD

[rootAVD](https://github.com/newbit1/rootAVD) is a maintained open-source script
that patches the AVD's ramdisk to inject Magisk on boot. Magisk gives us a
proper `su` mechanism inside the running AVD without needing `adb root`
(`adb root` is blocked on Play Store images).

```sh
cd ~/some/working/dir
git clone https://github.com/newbit1/rootAVD.git
```

---

## Step 3 — Create the AVD

```sh
export ANDROID_AVD_HOME="$HOME/.config/.android/avd"   # adjust if you use the default $HOME/.android/avd
export JAVA_HOME=/opt/android-studio/jbr

~/Android/Sdk/cmdline-tools/latest/bin/avdmanager create avd \
  -n mangaplus \
  -k "system-images;android-36;google_apis_playstore;x86_64" \
  --device "pixel_7" \
  --force < /dev/null
```

Notes:

- The CLI is quiet on success — verify with `ls "$ANDROID_AVD_HOME"`; you
  should see `mangaplus.avd/`.
- `pixel_7` is a sensible default phone profile; any modern phone profile
  works.

---

## Step 4 — Boot the AVD once (rootAVD needs an active ADB)

```sh
ANDROID_AVD_HOME="$HOME/.config/.android/avd" \
  ~/Android/Sdk/emulator/emulator -avd mangaplus -writable-system -no-snapshot \
  -netdelay none -netspeed full &

~/Android/Sdk/platform-tools/adb wait-for-device
~/Android/Sdk/platform-tools/adb shell getprop sys.boot_completed   # waits until prints "1"
```

---

## Step 5 — Patch the ramdisk via rootAVD

With the AVD running:

```sh
cd /path/to/rootAVD
./rootAVD.sh system-images/android-36/google_apis_playstore/x86_64/ramdisk.img
```

rootAVD downloads the latest Magisk, patches the ramdisk in place, and
typically shuts the emulator down. Restart it:

```sh
ANDROID_AVD_HOME="$HOME/.config/.android/avd" \
  ~/Android/Sdk/emulator/emulator -avd mangaplus -writable-system -no-snapshot \
  -netdelay none -netspeed full &
```

---

## Step 6 — Complete Magisk setup inside the AVD

In the emulator window:

1. Open the app drawer (swipe up from home)
2. Tap the **Magisk** app (mask icon)
3. A dialog says "**Magisk requires additional setup**" → tap **OK** /
   **Install**
4. Magisk patches more system bits and prompts to reboot → tap **Reboot**

After this reboot, root is permanently active via `su`.

Verify root works from your laptop:

```sh
~/Android/Sdk/platform-tools/adb wait-for-device
~/Android/Sdk/platform-tools/adb shell su -c id
```

**The first `su -c` call pops a Magisk grant dialog inside the AVD** asking to
grant root to "Shell". Tap **Grant** (and "Forever" if you don't want to be
re-asked). Then re-run the command; you should see `uid=0(root) gid=0(root)`.

---

## Step 7 — Sign into your Google account inside the AVD

In the emulator:

1. Open **Settings**
2. **Passwords & accounts** → **Add account** → **Google**
3. Sign in with the Google account that holds your MANGA Plus subscription

Confirm from your laptop:

```sh
~/Android/Sdk/platform-tools/adb shell dumpsys account | grep "Account {name="
```

You should see your `…@gmail.com` line.

---

## Step 8 — Install MANGA Plus from Play Store

In the emulator:

1. Open **Play Store**
2. Search **"MANGA Plus by SHUEISHA"**
3. Tap **Install**

This is the **key step that makes subscription restore work**: Google Play
Billing checks the app's installer source. Sideloading via `adb install` fails
this check; installing from Play Store passes it.

---

## Step 9 — Open MANGA Plus and confirm subscription

Walk through the onboarding (language, ToS, OneTrust consent). The app calls
`PUT /api/register` in the background, gets back a fresh `deviceSecret`, and
saves it to SharedPreferences.

Then navigate to **Settings** → **Manga Plus Max / Subscription** area. After a
few seconds (or after tapping a restore option), you should see your existing
subscription plan name (e.g. "Deluxe Plan"). This means the server bound the
AVD's `deviceSecret` to your paid subscription.

If it doesn't show up immediately, try opening **Settings → Restore in-app
purchases**. The button might silently no-op the first time but trigger a
background sync; re-checking the subscription page a moment later usually shows
the plan.

---

## Step 10 — Read the secret off disk

```sh
~/Android/Sdk/platform-tools/adb shell su -c \
  "cat /data/data/jp.co.shueisha.mangaplus/shared_prefs/config.xml"
```

You'll see XML. Look for:

```xml
<string name="secret">SOMELONGVALUEHERE</string>
<boolean name="isSubscriptionPurchased" value="true" />
```

Copy the `secret` value — that's your `deviceSecret`. The
`isSubscriptionPurchased=true` line confirms your subscription is bound.

---

## Step 11 — Save the secret

Add it to your shell env file (or your password manager):

```sh
# In ~/.config/zsh/secrets (or wherever your gitignored secrets file lives):
export MANGAPLUS_SECRET="paste_value_here"
```

Reload your shell:

```sh
source ~/.config/zsh/secrets
```

Sanity-check it works from your laptop without any of the AVD running:

```sh
HOST="jumpg-api.tokyo-cdn.com"
URL="https://${HOST}/api/profile?os=android&os_ver=33&app_ver=250&secret=${MANGAPLUS_SECRET}"
curl -sS -o /tmp/profile.bin \
  -w "HTTP %{http_code}  size=%{size_download} bytes\n" "$URL"
file /tmp/profile.bin
strings /tmp/profile.bin | head -5
```

You should see `HTTP 200`, a non-trivial size (several KB), and recognizable
protobuf strings (image URLs, account fields). If you see HTTP 401 / a tiny
payload, the secret is wrong or unbinding occurred.

---

## Step 12 — Clean up the AVD

You can stop the emulator now; you only need it for capture, not for normal
desktop reading:

```sh
~/Android/Sdk/platform-tools/adb -s emulator-5554 emu kill
```

Keep the AVD around in case you ever need to re-extract (e.g. if Shueisha
invalidates your secret in a future server update).

---

## When to re-extract

You need to repeat this process if any of these happen:

- The desktop reader starts returning HTTP 401 / "Auth failed" errors — the
  server invalidated this `deviceSecret`.
- You stop and re-subscribe to MANGA Plus on a different Google account.
- You switched the AVD's binding to a different Google account (then the
  current `deviceSecret` no longer maps to a paying subscription).

If the AVD is still set up, just boot it again, open MANGA Plus, optionally tap
"Restore in-app purchases", and re-read the file.

---

## Keeping the subscription attached (the plan-drop problem)

The `deviceSecret` itself never expires — but the **subscription tier the
server associates with it does lapse**. Learned the hard way on 2026-09-04:

- The server's entitlement is backed by a **Google Play receipt** that only
  the official app can submit (`PUT /api/subscription_restore` requires
  `purchase_data` plus a Google Play **signature** — it cannot be forged or
  replayed by the desktop reader).
- Google keeps auto-renewing on the Play side, but if no app ever refreshes
  the receipt with Shueisha's server, the attested billing period runs out
  and the server **silently downgrades the account to `basic`**.
- Symptom: MAX-tier chapters (e.g. all of Bleach past chapter 3, marked
  `DELUXE` in the API's `chapter_type`) start refusing with
  `Invalid user access(11301)`, while free chapters keep working. Nothing
  else looks broken.

The desktop reader polls `GET /api/subscription` (at most twice a day, from
the Library page) and shows an amber warning banner when the plan drops below
the best plan it has seen, or when the billing period ends without renewing.
Plan tiers, lowest to highest: `basic` → `standard` → `deluxe`. Note that
Bleach-class classic catalogs need **deluxe**, not standard.

### The restore ritual (when the banner appears)

1. Boot the AVD (see the DNS flag — required, see Troubleshooting):

   ```sh
   ANDROID_AVD_HOME=~/.config/.android/avd \
     ~/Android/Sdk/emulator/emulator -avd mangaplus -no-snapshot-save \
     -dns-server 8.8.8.8,1.1.1.1
   ```

2. Open MANGA Plus inside it (signed-in state persists in the AVD).
3. Go to the MAX / subscription screen and tap **Restore subscription**.
4. Verify from the host — plan should say your paid tier:

   ```sh
   curl -sS -A "okhttp/4.12.0" \
     "https://jumpg-api.tokyo-cdn.com/api/subscription?os=android&os_ver=36&app_ver=250&secret=$(cat ~/.config/mangaplus-reader/secret)&country_code=US" \
     | protoc --decode_raw | head -6
   # field 36 → field 1 → 1: "deluxe"  ← plan_type
   ```

5. Kill the emulator (`adb -s emulator-5554 emu kill`). No desktop-side
   change is needed: the restore re-attaches server-side to the same
   `deviceSecret` the reader already uses.

There is no need to re-extract the secret for a plan restore — the AVD app
and the desktop reader share one device registration.

---

## Troubleshooting

**`emulator` complains "Unknown AVD name [mangaplus]"** — your
`ANDROID_AVD_HOME` differs from `$HOME/.android/avd`. Either prefix the
emulator command with `ANDROID_AVD_HOME=...` or set it in your shell env.

**Black emulator window on launch (Nvidia + Wayland)** — the gfxstream renderer
sometimes fails on Nvidia. Try `-gpu swiftshader_indirect` for CPU rendering as
a diagnostic. If that works, your hardware GPU path is broken and you may need
to update drivers or use a different GPU mode.

**`adb root` fails: "adbd cannot run as root in production builds"** — this is
expected on `google_apis_playstore` images. Don't use `adb root` — use Magisk
via `adb shell su -c '…'` instead.

**`adb remount` fails: "mount: '/system' not in /proc/mounts"** — Magisk uses
overlayfs in a way that breaks classic remount. With Magisk root we don't need
to write to `/system` at all for this workflow (we only read from
`/data/data/…`).

**Subscription button silent / "an error has occurred"** — usually means
sideloaded install instead of Play Store install. Make sure you installed from
the Play Store inside the AVD (Step 8), not via `adb install`. Also confirm
the right Google account is signed in (Step 7).

**OneTrust banner fetch fails** — happens when mitmproxy is in the request
path. For this workflow we don't need mitmproxy at all (we read the file
directly), so just don't set an HTTP proxy on the AVD.

**Emulator says "no internet" but `ping 8.8.8.8` works from its shell** —
two distinct causes, both hit on 2026-09-04:

1. *DNS*: the emulator inherits the host's `/etc/resolv.conf`. With
   systemd-resolved that points at the loopback stub `127.0.0.53`, which is
   unreachable from inside the guest. Always launch with
   `-dns-server 8.8.8.8,1.1.1.1`.
2. *Stale global HTTP proxy*: a previous mitmproxy session may have persisted
   `http_proxy 10.0.2.2:8080` in the AVD's global settings. Android's
   connectivity probes then fail with `Failed to connect to /10.0.2.2:8080`
   (visible in `adb shell dumpsys network_stack`) while shell tools, which
   ignore the proxy, work fine. Clear it and bounce Wi-Fi:

   ```sh
   adb -s emulator-5554 shell settings put global http_proxy :0
   adb -s emulator-5554 shell settings delete global global_http_proxy_host
   adb -s emulator-5554 shell settings delete global global_http_proxy_port
   adb -s emulator-5554 shell "svc wifi disable; sleep 2; svc wifi enable"
   ```

---

## What this guide deliberately does NOT cover

- **Patching the MANGA Plus APK** to trust user-installed CAs and capturing
  via mitmproxy — Google Play Billing checks installer signature, breaking
  subscription restore on the re-signed APK.
- **Waydroid** — black-screen issues on Nvidia + Wayland make it unreliable.
  AVD is more portable.
- **rooting your physical phone** — out of scope for many users; AVD is the
  same outcome without touching your daily-driver device.
