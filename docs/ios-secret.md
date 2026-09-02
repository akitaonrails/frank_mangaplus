# Extract your `deviceSecret` (iOS, via mitmproxy)

1. Run mitmproxy on your desktop: mitmproxy --listen-port 8080
2. On the iPhone: Wi-Fi → your network → Configure Proxy → Manual → your desktop's LAN IP, port 8080.
3. Visit <http://mitm.it> in Safari on the phone, download the iOS cert profile, then install it: Settings → General → VPN & Device Management. Then the step people miss — Settings → General → About → Certificate Trust Settings → toggle full trust on for the mitmproxy CA.
4. Open MANGA Plus, load any chapter, and filter mitmproxy for jumpg-webapi (or whatever cfg.host is set to). Pull secret= out of the query string — 32 hex chars, same shape as the Android one.
