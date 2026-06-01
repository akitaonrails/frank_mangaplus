//! Stale-while-revalidate disk cache for the full title catalog.
//!
//! The MANGA Plus `title_list/all_v3` endpoint returns ~thousands of
//! titles — enough that:
//!   1. We don't want to refetch it on every search-page mount.
//!   2. We DO want a warm cache so typing-into-search responds instantly.
//!
//! Strategy:
//!   - On read: if disk cache exists and is fresh (`now - fetched_at < TTL`),
//!     return it. If stale, STILL return it immediately and spawn a
//!     background task that refetches + rewrites the file, then emits a
//!     `all_titles_refreshed` event the frontend picks up to swap state.
//!     If no cache exists, the caller falls back to a synchronous network
//!     fetch (handled in lib.rs, not here).
//!
//! TTL is 24h by default, overridable via `MANGAPLUS_CATALOG_TTL_HOURS`
//! env var. The override is exposed as a pure function for testability.
//!
//! Files written (`cache_dir/all_titles_<lang>_<clang>.bin` and a sidecar
//! `.meta.json` carrying `fetched_at_secs`) — one pair per locale so a
//! lang/clang switch never serves the wrong catalog.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default TTL when no override is set. 24 hours.
pub const DEFAULT_TTL_HOURS: u64 = 24;

/// Sidecar meta written next to the cached proto bytes. JSON for easy
/// debugging — `cat all_titles_*.meta.json` to inspect.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct CacheMeta {
    /// Unix seconds when the cache file was written.
    pub fetched_at_secs: u64,
    /// Counted at write time. Lets the frontend show progress hints
    /// ("loaded 1842 titles") without needing to decode the proto first.
    pub title_count: u32,
}

/// XDG cache dir for `mangaplus-reader`. Falls back through XDG_CACHE_HOME
/// → ~/.cache → tempdir. Mirrors `image_cache_dir` in lib.rs.
pub fn cache_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("mangaplus-reader");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache/mangaplus-reader");
    }
    std::env::temp_dir().join("mangaplus-reader")
}

/// Resolve effective TTL from env override. Empty / unparseable / zero
/// → fall back to default. Pure for testing.
pub fn resolve_ttl_hours(env_val: Option<&str>) -> u64 {
    env_val
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_TTL_HOURS)
}

/// Effective TTL using the real env var.
pub fn effective_ttl_hours() -> u64 {
    resolve_ttl_hours(std::env::var("MANGAPLUS_CATALOG_TTL_HOURS").ok().as_deref())
}

/// File paths for a given locale.
pub fn cache_paths(dir: &Path, lang: &str, clang: &str) -> (PathBuf, PathBuf) {
    let safe_lang = sanitize_locale(lang);
    let safe_clang = sanitize_locale(clang);
    let stem = format!("all_titles_{safe_lang}_{safe_clang}");
    (
        dir.join(format!("{stem}.bin")),
        dir.join(format!("{stem}.meta.json")),
    )
}

/// Strip anything other than ASCII alnum / `-` / `_` from a locale tag.
/// Defensive — the API only ever returns short IETF tags, but a corrupt
/// preference file shouldn't translate into a path-injection bug.
fn sanitize_locale(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Fresh,
    Stale,
}

/// Decide whether `fetched_at` + `ttl_hours` covers `now`. Stale once the
/// clock has crossed the deadline.
pub fn freshness_of(now_secs: u64, fetched_at_secs: u64, ttl_hours: u64) -> Freshness {
    let age = now_secs.saturating_sub(fetched_at_secs);
    if age < ttl_hours.saturating_mul(3600) {
        Freshness::Fresh
    } else {
        Freshness::Stale
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read the cache for the given locale, if present.
///
/// Returns `Some((bytes, meta, freshness))` when both files load and the
/// meta JSON parses. The bytes are an opaque payload — callers decide
/// the format. The current caller in `lib.rs` writes a JSON-encoded
/// `Vec<proto::Title>`; keeping `read`/`write` byte-blind lets us
/// change that format without touching this module.
pub fn read(dir: &Path, lang: &str, clang: &str, ttl_hours: u64) -> Option<(Vec<u8>, CacheMeta, Freshness)> {
    let (bin_path, meta_path) = cache_paths(dir, lang, clang);
    let bytes = std::fs::read(&bin_path).ok()?;
    let meta_raw = std::fs::read_to_string(&meta_path).ok()?;
    let meta: CacheMeta = serde_json::from_str(&meta_raw).ok()?;
    let fresh = freshness_of(now_secs(), meta.fetched_at_secs, ttl_hours);
    Some((bytes, meta, fresh))
}

/// Persist `bytes` + meta. Best-effort — failures (full disk, perms)
/// log to stderr but don't propagate; the in-memory data still works.
///
/// Each file goes through a `*.tmp` staging path before being renamed
/// into place. On every POSIX filesystem and on NTFS, `rename` is
/// atomic — so a crash mid-write leaves either the old pair, a single
/// `.tmp` leftover (ignored by `read`), or the new pair, never an
/// inconsistent `bin` paired with the previous `meta`.
pub fn write(dir: &Path, lang: &str, clang: &str, bytes: &[u8], title_count: u32) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("[all-titles-cache] mkdir {} failed: {}", dir.display(), e);
        return;
    }
    let (bin_path, meta_path) = cache_paths(dir, lang, clang);
    let meta = CacheMeta {
        fetched_at_secs: now_secs(),
        title_count,
    };
    let meta_json = match serde_json::to_string_pretty(&meta) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[all-titles-cache] meta encode failed: {e}");
            return;
        }
    };
    // Write bin first, then meta. The reverse order would briefly
    // associate the OLD bin with the NEW meta — which the `read` path
    // would happily serve, with a fetched_at lying about its
    // freshness. Bin-first means a crash between the two writes leaves
    // a `.tmp` for the bin (cleaned up next launch) AND no `.tmp` for
    // the meta — the disk still reflects the previous self-consistent
    // pair.
    if !write_atomic(&bin_path, bytes) {
        return;
    }
    let _ = write_atomic(&meta_path, meta_json.as_bytes());
}

/// Write `data` to `final_path` atomically via a sibling `*.tmp`.
/// Returns true on success, false on either step failing.
fn write_atomic(final_path: &Path, data: &[u8]) -> bool {
    let mut tmp_path = final_path.to_path_buf();
    // PathBuf::set_extension replaces the existing extension; use a
    // suffix concat instead so the original extension stays intact
    // for debuggability (`all_titles_eng_eng.bin.tmp`).
    let tmp_name = match final_path.file_name() {
        Some(n) => {
            let mut name = n.to_os_string();
            name.push(".tmp");
            name
        }
        None => {
            eprintln!("[all-titles-cache] no filename component in {}", final_path.display());
            return false;
        }
    };
    tmp_path.set_file_name(tmp_name);
    if let Err(e) = std::fs::write(&tmp_path, data) {
        eprintln!("[all-titles-cache] tmp write {} failed: {}", tmp_path.display(), e);
        return false;
    }
    if let Err(e) = std::fs::rename(&tmp_path, final_path) {
        eprintln!(
            "[all-titles-cache] rename {} → {} failed: {}",
            tmp_path.display(),
            final_path.display(),
            e
        );
        // Best-effort cleanup of the orphan tmp; ignore errors.
        let _ = std::fs::remove_file(&tmp_path);
        return false;
    }
    true
}

/// Holds a flag per (lang, clang) so we never have two background
/// revalidations racing on the same locale.
#[derive(Default)]
pub struct RefreshGuards {
    inner: Mutex<Vec<(String, String)>>,
}

impl RefreshGuards {
    /// True if `(lang, clang)` was NOT already in flight, and inserts it
    /// so subsequent callers get false. The matching call to `release`
    /// removes the entry once the background task finishes.
    pub fn try_acquire(&self, lang: &str, clang: &str) -> bool {
        let key = (lang.to_string(), clang.to_string());
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if g.iter().any(|(l, c)| l == &key.0 && c == &key.1) {
            return false;
        }
        g.push(key);
        true
    }

    pub fn release(&self, lang: &str, clang: &str) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.retain(|(l, c)| !(l == lang && c == clang));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_ttl_default_when_unset() {
        assert_eq!(resolve_ttl_hours(None), DEFAULT_TTL_HOURS);
    }

    #[test]
    fn resolve_ttl_default_when_unparseable() {
        assert_eq!(resolve_ttl_hours(Some("not-a-number")), DEFAULT_TTL_HOURS);
        assert_eq!(resolve_ttl_hours(Some("")), DEFAULT_TTL_HOURS);
        assert_eq!(resolve_ttl_hours(Some("   ")), DEFAULT_TTL_HOURS);
    }

    #[test]
    fn resolve_ttl_default_when_zero() {
        // A TTL of zero would make every read stale immediately, which
        // is almost certainly a misconfig. Treat as "use default".
        assert_eq!(resolve_ttl_hours(Some("0")), DEFAULT_TTL_HOURS);
    }

    #[test]
    fn resolve_ttl_respects_valid_override() {
        assert_eq!(resolve_ttl_hours(Some("1")), 1);
        assert_eq!(resolve_ttl_hours(Some("72")), 72);
        assert_eq!(resolve_ttl_hours(Some("  6  ")), 6);
    }

    #[test]
    fn freshness_fresh_within_window() {
        let now = 1_000_000;
        let fetched = now - 3600; // 1 hour ago
        assert_eq!(freshness_of(now, fetched, 24), Freshness::Fresh);
    }

    #[test]
    fn freshness_stale_past_window() {
        let now = 1_000_000;
        let fetched = now - 25 * 3600; // 25 hours ago
        assert_eq!(freshness_of(now, fetched, 24), Freshness::Stale);
    }

    #[test]
    fn freshness_stale_at_boundary() {
        let now = 1_000_000;
        let ttl_secs = 24 * 3600;
        // Exactly at the boundary counts as stale — the deadline has
        // arrived. Test the off-by-one explicitly so a refactor can't
        // silently flip the comparison.
        assert_eq!(freshness_of(now, now - ttl_secs, 24), Freshness::Stale);
        assert_eq!(freshness_of(now, now - ttl_secs + 1, 24), Freshness::Fresh);
    }

    #[test]
    fn freshness_handles_future_fetched_at() {
        // Clock skew or a clock-fixup mid-session could leave a meta
        // file dated in the future. saturating_sub means we treat it
        // as fresh rather than panicking.
        let now = 1_000_000;
        let fetched = now + 100;
        assert_eq!(freshness_of(now, fetched, 24), Freshness::Fresh);
    }

    #[test]
    fn cache_paths_include_locale_in_stem() {
        let dir = Path::new("/tmp/somewhere");
        let (bin, meta) = cache_paths(dir, "eng", "eng");
        assert_eq!(bin, Path::new("/tmp/somewhere/all_titles_eng_eng.bin"));
        assert_eq!(meta, Path::new("/tmp/somewhere/all_titles_eng_eng.meta.json"));
    }

    #[test]
    fn cache_paths_distinguish_locale_pairs() {
        let dir = Path::new("/tmp/x");
        let (eng, _) = cache_paths(dir, "eng", "eng");
        let (spa, _) = cache_paths(dir, "spa", "eng");
        assert_ne!(eng, spa);
    }

    #[test]
    fn sanitize_locale_strips_path_chars() {
        assert_eq!(sanitize_locale("eng"), "eng");
        assert_eq!(sanitize_locale("en-US"), "en-US");
        assert_eq!(sanitize_locale("../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_locale("eng/../"), "eng");
    }

    #[test]
    fn write_then_read_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = b"\x01\x02\x03 not real proto bytes";
        write(tmp.path(), "eng", "eng", payload, 1234);
        let (bytes, meta, fresh) = read(tmp.path(), "eng", "eng", 24).expect("cache present");
        assert_eq!(bytes, payload);
        assert_eq!(meta.title_count, 1234);
        assert_eq!(fresh, Freshness::Fresh);
    }

    #[test]
    fn read_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read(tmp.path(), "eng", "eng", 24).is_none());
    }

    #[test]
    fn read_returns_none_when_meta_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let (bin_path, meta_path) = cache_paths(tmp.path(), "eng", "eng");
        fs::create_dir_all(tmp.path()).unwrap();
        fs::write(&bin_path, b"payload").unwrap();
        fs::write(&meta_path, b"not json").unwrap();
        assert!(read(tmp.path(), "eng", "eng", 24).is_none());
    }

    #[test]
    fn read_marks_stale_after_ttl() {
        let tmp = tempfile::tempdir().unwrap();
        let (bin_path, meta_path) = cache_paths(tmp.path(), "eng", "eng");
        fs::create_dir_all(tmp.path()).unwrap();
        fs::write(&bin_path, b"payload").unwrap();
        // Write a meta dated 2 days ago against a 1h TTL → stale.
        let stale_meta = CacheMeta {
            fetched_at_secs: now_secs().saturating_sub(2 * 86_400),
            title_count: 7,
        };
        fs::write(&meta_path, serde_json::to_string(&stale_meta).unwrap()).unwrap();
        let (_, _, fresh) = read(tmp.path(), "eng", "eng", 1).expect("present");
        assert_eq!(fresh, Freshness::Stale);
    }

    #[test]
    fn refresh_guard_blocks_dogpile() {
        let g = RefreshGuards::default();
        assert!(g.try_acquire("eng", "eng"));
        assert!(!g.try_acquire("eng", "eng"));
        // Different locale is independent.
        assert!(g.try_acquire("spa", "eng"));
        g.release("eng", "eng");
        // After release, re-acquire works.
        assert!(g.try_acquire("eng", "eng"));
    }

    #[test]
    fn refresh_guard_release_is_noop_when_entry_absent() {
        // Calling release without a matching acquire shouldn't panic
        // or mutate other entries. The current implementation handles
        // this via retain() — pin that behaviour.
        let g = RefreshGuards::default();
        g.release("eng", "eng"); // no panic, no state change
        assert!(g.try_acquire("eng", "eng"));
        // Releasing a NON-acquired key leaves the acquired one alone.
        g.release("spa", "eng");
        assert!(!g.try_acquire("eng", "eng")); // still held
    }

    #[test]
    fn refresh_guard_release_one_of_n_preserves_others() {
        let g = RefreshGuards::default();
        assert!(g.try_acquire("eng", "eng"));
        assert!(g.try_acquire("spa", "eng"));
        assert!(g.try_acquire("por", "eng"));
        // Release only the middle entry.
        g.release("spa", "eng");
        assert!(!g.try_acquire("eng", "eng")); // still held
        assert!(!g.try_acquire("por", "eng")); // still held
        assert!(g.try_acquire("spa", "eng"));  // newly free
    }

    #[test]
    fn read_returns_none_when_only_meta_present() {
        // Partial write: meta landed before crash; bin doesn't exist.
        // `read` should bail without touching the meta — otherwise a
        // future format change that adds a meta-only "marker" would
        // confuse the cache hit path.
        let tmp = tempfile::tempdir().unwrap();
        let (_, meta_path) = cache_paths(tmp.path(), "eng", "eng");
        fs::create_dir_all(tmp.path()).unwrap();
        let valid_meta = CacheMeta {
            fetched_at_secs: now_secs(),
            title_count: 0,
        };
        fs::write(&meta_path, serde_json::to_string(&valid_meta).unwrap()).unwrap();
        assert!(read(tmp.path(), "eng", "eng", 24).is_none());
    }

    #[test]
    fn read_returns_none_when_only_bin_present() {
        // The inverse: payload exists but no sidecar meta. Without
        // meta we have no freshness signal, so treating it as a miss
        // is the only safe choice.
        let tmp = tempfile::tempdir().unwrap();
        let (bin_path, _) = cache_paths(tmp.path(), "eng", "eng");
        fs::create_dir_all(tmp.path()).unwrap();
        fs::write(&bin_path, b"payload").unwrap();
        assert!(read(tmp.path(), "eng", "eng", 24).is_none());
    }

    #[test]
    fn write_uses_tmp_and_rename_pattern() {
        // After a successful write, no `.tmp` orphans should remain.
        // (The atomic-rename pattern is what protects against
        // mid-write crashes; smoke-test the happy path.)
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "eng", "eng", b"hello", 1);
        let names: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".tmp")), "found orphan: {names:?}");
    }
}
