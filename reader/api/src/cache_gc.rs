//! Image-cache garbage collection.
//!
//! The on-disk image cache (see `Client::cache_path_for`) is written on
//! every successful CDN fetch and was never evicted — a long-lived
//! install grows it unboundedly (observed: 4.8 GB / 10k files after
//! three months of reading). This module prunes a cache directory down
//! to a byte budget by deleting the oldest files (mtime order) first.
//!
//! Deliberately std-only and best-effort: individual unreadable or
//! undeletable files are skipped, a missing directory is a no-op, and
//! the caller runs it off the hot path (the desktop app spawns it in a
//! background thread at startup).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PruneStats {
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub files_removed: usize,
    pub bytes_removed: u64,
}

/// Delete oldest-first until the directory's total size fits inside
/// `max_bytes`. Empty subdirectories left behind by deletions are
/// removed opportunistically.
pub fn prune_image_cache(dir: &Path, max_bytes: u64) -> io::Result<PruneStats> {
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    collect_files(dir, &mut files);

    let mut stats = PruneStats {
        files_scanned: files.len(),
        bytes_scanned: files.iter().map(|f| f.1).sum(),
        ..Default::default()
    };
    if stats.bytes_scanned <= max_bytes {
        return Ok(stats);
    }

    // Oldest first. Ties (same mtime second) break by path for
    // determinism — matters for the tests, harmless in production.
    files.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));

    let mut remaining = stats.bytes_scanned;
    for (path, size, _) in files {
        if remaining <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            remaining = remaining.saturating_sub(size);
            stats.files_removed += 1;
            stats.bytes_removed += size;
        }
    }

    remove_empty_dirs(dir);
    Ok(stats)
}

/// Recursively gather (path, size, mtime) for every regular file.
/// Errors on individual entries are skipped — a cache file we can't
/// stat is a cache file we simply don't manage this round.
fn collect_files(dir: &Path, out: &mut Vec<(PathBuf, u64, SystemTime)>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return, // missing dir (fresh install) or unreadable → no-op
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            collect_files(&path, out);
        } else if meta.is_file() {
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            out.push((path, meta.len(), mtime));
        }
    }
}

/// Depth-first removal of now-empty directories. `remove_dir` fails on
/// non-empty dirs, which is exactly the guard we want — no size checks
/// needed. The root itself is preserved.
fn remove_empty_dirs(dir: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            remove_empty_dirs(&path);
            let _ = fs::remove_dir(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File, FileTimes};
    use std::io::Write as _;
    use std::time::Duration;

    /// Unique per-test scratch dir under the system temp dir, cleaned
    /// on drop. std-only stand-in for the tempfile crate.
    struct TestDir(PathBuf);
    impl TestDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "mangaplus-cache-gc-{}-{}-{tag}",
                std::process::id(),
                std::thread::current().name().unwrap_or("t").replace("::", "-"),
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            TestDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Write `size` bytes at `rel` with an mtime `age_secs` in the past
    /// (older = larger age = pruned first).
    fn put_file(root: &Path, rel: &str, size: usize, age_secs: u64) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = File::create(&path).unwrap();
        f.write_all(&vec![0u8; size]).unwrap();
        let mtime = SystemTime::now() - Duration::from_secs(age_secs);
        f.set_times(FileTimes::new().set_modified(mtime)).unwrap();
    }

    #[test]
    fn under_budget_is_a_noop() {
        let dir = TestDir::new("noop");
        put_file(dir.path(), "a/1.webp", 100, 30);
        put_file(dir.path(), "b/2.webp", 100, 10);
        let stats = prune_image_cache(dir.path(), 1000).unwrap();
        assert_eq!(stats.files_scanned, 2);
        assert_eq!(stats.bytes_scanned, 200);
        assert_eq!(stats.files_removed, 0);
        assert!(dir.path().join("a/1.webp").exists());
        assert!(dir.path().join("b/2.webp").exists());
    }

    #[test]
    fn removes_oldest_first_until_within_budget() {
        let dir = TestDir::new("oldest");
        put_file(dir.path(), "old.webp", 100, 300);
        put_file(dir.path(), "mid.webp", 100, 200);
        put_file(dir.path(), "new.webp", 100, 100);
        // Budget 150: must delete old (300→200) and mid (200→100).
        let stats = prune_image_cache(dir.path(), 150).unwrap();
        assert_eq!(stats.files_removed, 2);
        assert_eq!(stats.bytes_removed, 200);
        assert!(!dir.path().join("old.webp").exists());
        assert!(!dir.path().join("mid.webp").exists());
        assert!(dir.path().join("new.webp").exists());
    }

    #[test]
    fn cleans_emptied_subdirectories_but_keeps_root_and_occupied_dirs() {
        let dir = TestDir::new("dirs");
        put_file(dir.path(), "title/1/chapter/9/1.webp", 100, 300);
        put_file(dir.path(), "title/2/chapter/7/1.webp", 100, 10);
        let stats = prune_image_cache(dir.path(), 100).unwrap();
        assert_eq!(stats.files_removed, 1);
        // The emptied branch is gone, the occupied one and root remain.
        assert!(!dir.path().join("title/1").exists());
        assert!(dir.path().join("title/2/chapter/7/1.webp").exists());
        assert!(dir.path().exists());
    }

    #[test]
    fn missing_directory_is_a_noop() {
        let dir = TestDir::new("missing");
        let ghost = dir.path().join("does-not-exist");
        let stats = prune_image_cache(&ghost, 1000).unwrap();
        assert_eq!(stats, PruneStats::default());
    }

    #[test]
    fn zero_budget_removes_everything() {
        let dir = TestDir::new("zero");
        put_file(dir.path(), "a/1.webp", 50, 20);
        put_file(dir.path(), "a/2.webp", 50, 10);
        let stats = prune_image_cache(dir.path(), 0).unwrap();
        assert_eq!(stats.files_removed, 2);
        assert_eq!(stats.bytes_removed, 100);
    }
}
