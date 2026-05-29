// Convert a MANGA Plus CDN URL into one our Tauri `mpimg://` custom
// protocol will handle. The Rust side then fetches with the right
// User-Agent + plus_vw_token cookie, threads the response back to the
// WebView, and caches the bytes to ~/.cache/mangaplus-reader/.
//
// Library/search thumbnails come from jumpg-assets.tokyo-cdn.com (no
// "3" suffix) and would work cookieless, but routing every image
// through one path keeps the cache logic uniform.

export function img(url: string | undefined | null): string {
  if (!url) return '';
  return url.replace(/^https:/, 'mpimg:');
}
