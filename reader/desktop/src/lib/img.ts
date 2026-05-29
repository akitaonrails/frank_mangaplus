// Convert a MANGA Plus CDN URL into one our Tauri `mpimg://` custom
// protocol will handle. The Rust side then fetches with the right
// User-Agent + plus_vw_token cookie, threads the response back to the
// WebView, and caches the bytes to ~/.cache/mangaplus-reader/.
//
// Library/search thumbnails come from jumpg-assets.tokyo-cdn.com (no
// "3" suffix) and would work cookieless, but routing every image
// through one path keeps the cache logic uniform.

/**
 * Convert a CDN URL to its Tauri-proxied counterpart so the WebView
 * `<img>` requests go through our cookie-aware Rust client.
 *
 * Named `proxied` (NOT `img`) — naming the function `img` confuses
 * Svelte 5's preprocessor template-extraction regex into thinking
 * `img(...)` is an opening `<img>` HTML tag, which makes it return
 * the entire script block as the route's CSS chunk and PostCSS panics
 * with "Unknown word invoke" at line 2 of the file. Spent a half hour
 * on this once; don't rename it back.
 */
export function proxied(url: string | undefined | null): string {
  if (!url) return '';
  return url.replace(/^https:/, 'mpimg:');
}
