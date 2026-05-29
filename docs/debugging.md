# Debugging the MANGA Plus API

Notes for contributors who want to inspect the live MANGA Plus API — verify a new endpoint, capture a header that changed, etc.

This is **not** needed to install or use the reader. For that see [`install.md`](install.md).

---

## The setup we used to figure all this out

1. A rooted Android emulator (Google Play system image + Magisk via rootAVD).
2. **mitmproxy** on the host, with its CA cert pushed into the AVD's APEX Conscrypt trust store.
3. Frida-server as an alternative when mitmproxy was inconvenient (e.g. when OneTrust SDK pinning broke the consent flow).

The ad-hoc scripts that drove this live in `reader/.tmp/` on the maintainer's machine and are not tracked in git (intentionally; some download large binaries).

---

## mitmproxy capture

Why mitmproxy: it shows the exact URL, headers, cookies, and request/response bodies of every call MANGA Plus makes.

### Pushing the CA into APEX Conscrypt on Android 14+

On API 34+ (our AVD is 36), the system trust store moved from `/system/etc/security/cacerts/` to `/apex/com.android.conscrypt/cacerts/`. `/apex` is read-only and not Magisk-overlayable in the standard way, so we bind-mount a writable copy of the directory on top.

The general flow (run inside `adb shell su`):

```sh
adb push ~/.mitmproxy/mitmproxy-ca-cert.cer /sdcard/mitm.cer
HASH=$(openssl x509 -inform PEM -subject_hash_old -in ~/.mitmproxy/mitmproxy-ca-cert.cer | head -1)

# inside `adb shell su -`:
mkdir -p /data/local/tmp/cacerts
cp /apex/com.android.conscrypt/cacerts/* /data/local/tmp/cacerts/
cp /sdcard/mitm.cer /data/local/tmp/cacerts/${HASH}.0
chmod 644 /data/local/tmp/cacerts/${HASH}.0

mount -t tmpfs tmpfs /apex/com.android.conscrypt/cacerts
cp /data/local/tmp/cacerts/* /apex/com.android.conscrypt/cacerts/
chmod 644 /apex/com.android.conscrypt/cacerts/*

# Also bind into every running process's mount namespace:
for pid in $(ls /proc | grep -E '^[0-9]+$'); do
  [ -e /proc/$pid/ns/mnt ] && nsenter --mount=/proc/$pid/ns/mnt -- \
    /bin/sh -c "mount -t tmpfs tmpfs /apex/com.android.conscrypt/cacerts && \
      cp /data/local/tmp/cacerts/* /apex/com.android.conscrypt/cacerts/ && \
      chmod 644 /apex/com.android.conscrypt/cacerts/*" 2>/dev/null
done
```

(The cert install lasts until the AVD reboots — fine for a capture session.)

### Running mitmweb with OneTrust passthrough

MANGA Plus uses OneTrust for consent. OneTrust SDK does its own cert pinning that fights mitm — bypass it by telling mitmweb to leave OneTrust hosts alone:

```sh
mitmweb --ignore-hosts '(onetrust|cookielaw|googleapis|gstatic|googleads)' &
adb shell settings put global http_proxy 10.0.2.2:8080
```

Restart MANGA Plus in the AVD. Open a chapter. In `http://127.0.0.1:8081`, filter for `jumpg-` to see API calls, `jumpg-assets3` to see manga-page image requests.

When done:
```sh
adb shell settings put global http_proxy :0
```

---

## Frida hook (alternative)

Frida sees the request from inside the running app, so it bypasses TLS entirely. Useful when mitm cert install is painful or the app does extra fingerprinting we haven't replicated.

Setup (pin to Frida 16.x because 17+ removed the auto-global `Java` and the inline-script API path is fiddly):

```sh
/usr/bin/python3 -m pip install --user "frida-tools<14.0" "frida>=16,<17"
FV=$(frida --version)
curl -L -o /tmp/fs.xz "https://github.com/frida/frida/releases/download/${FV}/frida-server-${FV}-android-x86_64.xz"
xz -dk /tmp/fs.xz
adb push /tmp/frida-server-${FV}-android-x86_64 /data/local/tmp/frida-server
adb shell chmod 755 /data/local/tmp/frida-server
adb shell "su -c 'nohup /data/local/tmp/frida-server >/dev/null 2>&1 &'"
```

> ⚠ On API 36 (Android 16) we've seen `frida-server` 16.x crash `system_server` and reboot the AVD. Frida 17.x doesn't have this problem but needs the new ESM-style Java bridge import. Pick your poison.

A minimal Python hook that prints every URL + headers MANGA Plus's OkHttp sends:

```python
import frida, sys
proc = frida.get_usb_device(timeout=5).attach("Manga Plus")  # display name, not package
script = proc.create_script("""
Java.perform(function() {
  var OkHttpClient = Java.use("okhttp3.OkHttpClient");
  OkHttpClient.newCall.implementation = function(req) {
    var url = req.url().toString();
    if (url.indexOf("jumpg-") >= 0) {
      var lines = ["=== " + req.method() + " " + url];
      var h = req.headers();
      for (var i = 0; i < h.size(); i++) lines.push("  " + h.name(i) + ": " + h.value(i));
      send(lines.join("\\n"));
    }
    return this.newCall(req);
  };
  send("[ok] hooked");
});
""")
script.on("message", lambda m, _: print(m.get("payload") or m))
script.load()
sys.stdin.read()
```

---

## What the headers actually look like

For posterity — typical request the official Android app sends:

```
GET https://jumpg-assets3.tokyo-cdn.com/secure/title/100020/chapter/1000486/manga_page/high/1.webp\
?hash=<x>&expires=<unix>&os=android&os_ver=36&app_ver=250&secret=<deviceSecret>
accept-encoding: gzip
cookie: plus_vw_token=<32-char-hex>
user-agent: okhttp/4.12.0
```

The `plus_vw_token` cookie is set on `manga_viewer_v3` responses (`domain=.tokyo-cdn.com`, `SameSite=None`, `HttpOnly`) and is **required** on premium image fetches. Without it: 400. With a stale one: 403.

The `secret=` query param is your `deviceSecret` (see [install.md](install.md) for how to extract it). It's appended by an OkHttp interceptor on every request to any `*.tokyo-cdn.com` host.

---

## Common pitfalls

- **`reqwest` defaults disable cookies.** If you fork the Rust client and build with `default-features = false`, you must explicitly enable `cookies` or premium image fetches will fail with 400.
- **`reqwest/<ver>` User-Agent is rejected.** Use `okhttp/4.12.0` or similar.
- **OneTrust SDK breaks behind mitm.** Use `--ignore-hosts` for OneTrust domains.
- **Frida-server 16.x crashes `system_server` on API 36.** Use Frida 17.x with the ESM-style Java bridge import, or stay on API 33–35 for hooking work.
- **`busybox wget` from `adb shell` gets 400 too** — even Android's stock TLS isn't enough; the cookie is what matters, not the TLS fingerprint.
