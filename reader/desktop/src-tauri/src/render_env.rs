//! GPU/compositor detection + render-mode policy for Linux.
//!
//! WebKitGTK has several rendering paths, some of which crash with
//! "EGL_SUCCESS" / "EGL_BAD_PARAMETER" / blank screens on specific
//! combinations of Wayland compositor, GPU vendor, and driver version.
//! The historical fix was to set two env vars that disable WebKit's
//! GPU paths entirely — safe but throws away hardware acceleration for
//! every user, even those whose hardware works fine.
//!
//! This module replaces the always-on hammer with a small policy:
//!
//!   1. If the user has an explicit override (env var or config file),
//!      honour that.
//!   2. Else, if a crash-recovery marker is present from a previous
//!      launch, the previous attempt failed mid-init → use the safe
//!      hammer this time.
//!   3. Else, read the GPU vendor from sysfs + the display server from
//!      env, and apply the lightest workaround that's known to work on
//!      that combination.
//!
//! The pure decision logic (everything that doesn't touch syscalls or
//! the filesystem) is what we test. Side-effecting code at the bottom
//! reads sysfs and sets env vars; those wrappers are kept tiny.
//!
//! Side note on `set_var` safety: this module is invoked exactly once,
//! from the very top of `run()`, before any user code has spawned a
//! thread. The unsafe blocks below are sound under that invariant.

#![cfg(target_os = "linux")]
// dead_code is suppressed at the module level only until the next
// commit wires apply_mode + decide_mode into run(). Tests already
// exercise everything here.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// GPU vendor identified from sysfs's `device/vendor` hex code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

/// Display server, derived from environment variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    Wayland,
    X11,
    Unknown,
}

/// Rendering policy — describes what env vars (if any) the binary
/// should set to make WebKit happy on the current system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// No env vars set. WebKit picks its own defaults (current
    /// upstream: GPU + DMA-BUF renderer). The right choice on X11 and
    /// on Wayland setups that aren't known to have issues.
    Native,
    /// `WEBKIT_DISABLE_DMABUF_RENDERER=1` — disables the newer
    /// (sometimes flaky) DMA-BUF renderer but keeps the older SHM path
    /// which still uses GPU compositing. Light touch; default for
    /// Wayland + AMD/Intel.
    DmabufOff,
    /// DmabufOff + `__NV_DISABLE_EXPLICIT_SYNC=1` — additional NVIDIA-
    /// driver-specific workaround that lets the explicit-sync path
    /// fall back to the older implicit-sync code. Light touch for
    /// NVIDIA on Wayland; doesn't disable GPU.
    NvidiaLight,
    /// `WEBKIT_DISABLE_DMABUF_RENDERER=1` + `WEBKIT_DISABLE_COMPOSITING_MODE=1`
    /// — the heavy hammer. Pushes WebKit into full CPU rendering. Slow
    /// but reliable on systems where even the SHM path can't initialise
    /// EGL. Triggered by crash-recovery (last launch failed) or by
    /// explicit user opt-in.
    Safe,
}

impl RenderMode {
    /// The (env-var-name, value) pairs this mode requires. Caller sets
    /// them via `std::env::set_var` if they're not already set in the
    /// process environment.
    pub fn env_vars(self) -> &'static [(&'static str, &'static str)] {
        match self {
            RenderMode::Native => &[],
            RenderMode::DmabufOff => &[
                ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
            ],
            RenderMode::NvidiaLight => &[
                ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
                ("__NV_DISABLE_EXPLICIT_SYNC", "1"),
            ],
            RenderMode::Safe => &[
                ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
                ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
            ],
        }
    }

    /// Short slug used in logs + the config file. Parser accepts this
    /// verbatim so users can drop it in `MANGAPLUS_RENDER_MODE=...`.
    pub fn slug(self) -> &'static str {
        match self {
            RenderMode::Native => "native",
            RenderMode::DmabufOff => "dmabuf-off",
            RenderMode::NvidiaLight => "nvidia-light",
            RenderMode::Safe => "safe",
        }
    }
}

/// Map the lowercase user-supplied slug back to a RenderMode, plus the
/// special "auto" sentinel which means "use the detection result."
pub fn parse_mode_override(s: &str) -> Option<ModeOverride> {
    match s.trim().to_lowercase().as_str() {
        "auto" => Some(ModeOverride::Auto),
        "native" => Some(ModeOverride::Explicit(RenderMode::Native)),
        "dmabuf-off" => Some(ModeOverride::Explicit(RenderMode::DmabufOff)),
        "nvidia-light" => Some(ModeOverride::Explicit(RenderMode::NvidiaLight)),
        "safe" => Some(ModeOverride::Explicit(RenderMode::Safe)),
        _ => None,
    }
}

/// What the user (or crash recovery) said about which mode to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeOverride {
    Auto,
    Explicit(RenderMode),
}

/// Parse the "vendor" file under /sys/class/drm/cardN/device/vendor.
/// File content is a hex PCI vendor id (e.g. "0x10de\n" for NVIDIA).
pub fn parse_vendor_hex(s: &str) -> GpuVendor {
    // Lowercase first so case-variant prefixes ("0X") strip correctly.
    let cleaned_lower = s.trim().to_lowercase();
    let cleaned = cleaned_lower.trim_start_matches("0x");
    match cleaned {
        "10de" => GpuVendor::Nvidia,
        "1002" => GpuVendor::Amd,
        "8086" => GpuVendor::Intel,
        _ => GpuVendor::Unknown,
    }
}

/// Decide which RenderMode to apply given the detected environment.
/// This is the heart of the policy and has full test coverage.
///
/// Precedence (highest first):
///   1. explicit override (env or config) — honour it verbatim
///   2. recovery (last run died) — fall back to Safe
///   3. auto: based on (display server, GPU vendor)
pub fn decide_mode(
    explicit_override: Option<RenderMode>,
    recovery_needed: bool,
    display: DisplayServer,
    gpu: GpuVendor,
) -> (RenderMode, &'static str) {
    if let Some(m) = explicit_override {
        return (m, "explicit user override");
    }
    if recovery_needed {
        return (RenderMode::Safe, "previous launch did not signal app-ready (crash recovery)");
    }
    match (display, gpu) {
        // X11 is rarely affected by the EGL crashes; trust upstream
        // defaults.
        (DisplayServer::X11, _) => (RenderMode::Native, "X11 session: WebKit defaults work reliably"),

        // Wayland + NVIDIA: notoriously flaky on the DMABUF + explicit-
        // sync path. Lighter touch keeps the GPU on but avoids both.
        (DisplayServer::Wayland, GpuVendor::Nvidia) => (
            RenderMode::NvidiaLight,
            "NVIDIA on Wayland: disable DMA-BUF + explicit sync, keep GPU compositing",
        ),

        // Wayland + AMD or Intel: just disable DMABUF; SHM compositing
        // path is GPU-accelerated and reliable on these stacks.
        (DisplayServer::Wayland, GpuVendor::Amd) |
        (DisplayServer::Wayland, GpuVendor::Intel) => (
            RenderMode::DmabufOff,
            "AMD/Intel on Wayland: disable DMA-BUF, keep SHM-based GPU compositing",
        ),

        // Wayland + unknown vendor: conservative — disable DMABUF, keep
        // the rest. Users on broken setups will fall through to Safe
        // mode automatically on the next launch via crash recovery.
        (DisplayServer::Wayland, GpuVendor::Unknown) => (
            RenderMode::DmabufOff,
            "Wayland with unknown GPU vendor: conservative DMA-BUF disable",
        ),

        // No display server detected (server / headless / weird env).
        // Native is fine — Tauri will likely fail elsewhere anyway.
        (DisplayServer::Unknown, _) => (RenderMode::Native, "no display server detected"),
    }
}

// ---------- side-effecting wrappers ----------
//
// Below this line: code that reads sysfs / env / files. The pure logic
// above gets unit-tested; the side-effecting wrappers are kept tiny so
// inspection is enough.

/// Read the GPU vendor from sysfs. Tries each `/sys/class/drm/cardN/`
/// entry until it finds one whose `device/vendor` is a recognised
/// vendor id. Falls back to `Unknown` when nothing is readable.
pub fn detect_gpu_vendor_from_sysfs() -> GpuVendor {
    let drm_dir = Path::new("/sys/class/drm");
    let entries = match std::fs::read_dir(drm_dir) {
        Ok(e) => e,
        Err(_) => return GpuVendor::Unknown,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let vendor_path = entry.path().join("device").join("vendor");
        if let Ok(s) = std::fs::read_to_string(&vendor_path) {
            let v = parse_vendor_hex(&s);
            if v != GpuVendor::Unknown {
                return v;
            }
        }
    }
    GpuVendor::Unknown
}

/// Display server from env: `WAYLAND_DISPLAY` set → Wayland; else
/// `DISPLAY` set → X11; else Unknown.
pub fn detect_display_server_from_env() -> DisplayServer {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        DisplayServer::Wayland
    } else if std::env::var_os("DISPLAY").is_some() {
        DisplayServer::X11
    } else {
        DisplayServer::Unknown
    }
}

/// Honour `MANGAPLUS_RENDER_MODE` env var if it's set to a valid slug.
/// Anything else (unset, empty, garbage) → None.
pub fn override_from_env() -> Option<ModeOverride> {
    std::env::var("MANGAPLUS_RENDER_MODE")
        .ok()
        .and_then(|s| parse_mode_override(&s))
}

/// Parse a `key=value` config file body for a `mode = ...` entry.
/// Quotes, surrounding whitespace, and `#` comments are tolerated.
pub fn parse_config_mode(body: &str) -> Option<ModeOverride> {
    for raw_line in body.lines() {
        // Strip trailing comments.
        let line = match raw_line.split_once('#') {
            Some((before, _)) => before,
            None => raw_line,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = match line.split_once('=') {
            Some(p) => p,
            None => continue,
        };
        if k.trim() != "mode" {
            continue;
        }
        // Strip surrounding quotes if present.
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
        return parse_mode_override(v);
    }
    None
}

/// Apply the chosen RenderMode by setting its env vars — but only if
/// the user hasn't already set the same var themselves (then their
/// value wins). Idempotent; safe to call once before Tauri init.
///
/// SAFETY: caller must hold the invariant that no other thread has
/// spawned yet. `run()` is the binary's first user-code call after
/// `main`, so this is satisfied there.
pub unsafe fn apply_mode(mode: RenderMode) {
    for &(k, v) in mode.env_vars() {
        if std::env::var_os(k).is_none() {
            unsafe { std::env::set_var(k, v) };
        }
    }
}

// ---------- crash-recovery marker + config file paths ----------
//
// Both files live in the same config_dir as the existing `secret` file
// (~/.config/mangaplus-reader on Linux). Layout:
//
//   secret               (already there)
//   auto-registered      (already there)
//   render.conf          (this commit; user override)
//   render-recovery      (this commit; touched on startup, removed by
//                         the frontend mark_app_ready command)
//
// The recovery file's mere existence is the signal — its contents are
// ignored.

/// Path of the user-editable render config: `<config_dir>/render.conf`.
pub fn render_config_path(config_dir: &Path) -> PathBuf {
    config_dir.join("render.conf")
}

/// Path of the crash-recovery marker: `<config_dir>/render-recovery`.
pub fn recovery_marker_path(config_dir: &Path) -> PathBuf {
    config_dir.join("render-recovery")
}

/// Path of the human-readable state dump: `<config_dir>/render-state.log`.
/// Written on every launch so users can `cat` it to see what was applied.
pub fn render_state_log_path(config_dir: &Path) -> PathBuf {
    config_dir.join("render-state.log")
}

/// Read the user override, preferring the env var over the config file.
/// `env_value` is what callers got from std::env::var; passing it in
/// keeps this function unit-testable. `config_dir` is checked for a
/// `render.conf` file with a `mode = ...` line.
pub fn resolve_user_override(
    env_value: Option<&str>,
    config_dir: &Path,
) -> Option<ModeOverride> {
    if let Some(s) = env_value {
        if let Some(o) = parse_mode_override(s) {
            return Some(o);
        }
    }
    let body = std::fs::read_to_string(render_config_path(config_dir)).ok()?;
    parse_config_mode(&body)
}

/// True when the previous launch left a recovery marker behind.
pub fn is_recovery_needed(config_dir: &Path) -> bool {
    recovery_marker_path(config_dir).exists()
}

/// Create the recovery marker at the start of a launch. Best-effort —
/// if writing fails (read-only home, etc.) we silently continue; the
/// only downside is that a future crash wouldn't auto-recover.
pub fn create_recovery_marker(config_dir: &Path) {
    let path = recovery_marker_path(config_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, "1");
}

/// Remove the recovery marker. Called from the Tauri `mark_app_ready`
/// command after the frontend finishes its first render.
pub fn clear_recovery_marker(config_dir: &Path) {
    let _ = std::fs::remove_file(recovery_marker_path(config_dir));
}

/// Write a one-shot snapshot of what was decided + what was applied
/// to <config_dir>/render-state.log so users can inspect it from a
/// terminal. Overwrites the file each launch — no log rotation.
pub fn write_state_log(
    config_dir: &Path,
    mode: RenderMode,
    reason: &str,
    display: DisplayServer,
    gpu: GpuVendor,
    overridden_by_user: bool,
    recovery_active: bool,
) {
    let path = render_state_log_path(config_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let env_dump = mode
        .env_vars()
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n  ");
    let body = format!(
        "# Last-launch render policy snapshot.\n\
         # Overwritten each launch. Override via MANGAPLUS_RENDER_MODE=...\n\
         # or by editing {}.\n\
         mode = {}\n\
         reason = {}\n\
         display = {:?}\n\
         gpu = {:?}\n\
         overridden_by_user = {}\n\
         recovery_active = {}\n\
         applied_env =\n  {}\n",
        render_config_path(config_dir).display(),
        mode.slug(),
        reason,
        display,
        gpu,
        overridden_by_user,
        recovery_active,
        if env_dump.is_empty() { "(none)".into() } else { env_dump },
    );
    let _ = std::fs::write(path, body);
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vendor_hex_known() {
        assert_eq!(parse_vendor_hex("0x10de\n"), GpuVendor::Nvidia);
        assert_eq!(parse_vendor_hex("0x1002\n"), GpuVendor::Amd);
        assert_eq!(parse_vendor_hex("0x8086\n"), GpuVendor::Intel);
    }

    #[test]
    fn parse_vendor_hex_tolerates_casing_and_no_prefix() {
        assert_eq!(parse_vendor_hex("10DE"), GpuVendor::Nvidia);
        assert_eq!(parse_vendor_hex("  0X1002 \n"), GpuVendor::Amd);
        assert_eq!(parse_vendor_hex("0x8086"), GpuVendor::Intel);
    }

    #[test]
    fn parse_vendor_hex_unknown() {
        assert_eq!(parse_vendor_hex("0xffff"), GpuVendor::Unknown);
        assert_eq!(parse_vendor_hex(""), GpuVendor::Unknown);
        assert_eq!(parse_vendor_hex("nope"), GpuVendor::Unknown);
    }

    #[test]
    fn render_mode_env_vars_layered() {
        assert!(RenderMode::Native.env_vars().is_empty());
        let dmabuf = RenderMode::DmabufOff.env_vars();
        assert_eq!(dmabuf, [("WEBKIT_DISABLE_DMABUF_RENDERER", "1")]);
        let nv = RenderMode::NvidiaLight.env_vars();
        assert!(nv.iter().any(|&(k, _)| k == "WEBKIT_DISABLE_DMABUF_RENDERER"));
        assert!(nv.iter().any(|&(k, _)| k == "__NV_DISABLE_EXPLICIT_SYNC"));
        let safe = RenderMode::Safe.env_vars();
        assert!(safe.iter().any(|&(k, _)| k == "WEBKIT_DISABLE_DMABUF_RENDERER"));
        assert!(safe.iter().any(|&(k, _)| k == "WEBKIT_DISABLE_COMPOSITING_MODE"));
    }

    #[test]
    fn render_mode_slugs_round_trip_through_parser() {
        for &m in &[RenderMode::Native, RenderMode::DmabufOff, RenderMode::NvidiaLight, RenderMode::Safe] {
            let parsed = parse_mode_override(m.slug()).unwrap();
            assert_eq!(parsed, ModeOverride::Explicit(m), "slug {} did not round-trip", m.slug());
        }
        // "auto" maps to the Auto sentinel, not a mode.
        assert_eq!(parse_mode_override("auto"), Some(ModeOverride::Auto));
        assert_eq!(parse_mode_override("AUTO"), Some(ModeOverride::Auto));
        assert_eq!(parse_mode_override("safe\n"), Some(ModeOverride::Explicit(RenderMode::Safe)));
    }

    #[test]
    fn parse_mode_override_rejects_garbage() {
        assert_eq!(parse_mode_override(""), None);
        assert_eq!(parse_mode_override("unknown-mode"), None);
        assert_eq!(parse_mode_override("on"), None);
    }

    #[test]
    fn decide_mode_explicit_override_wins_over_everything() {
        // Even with recovery needed AND a Wayland/NVIDIA situation, the
        // explicit override is honoured verbatim.
        let (mode, _) = decide_mode(
            Some(RenderMode::Native),
            /* recovery */ true,
            DisplayServer::Wayland,
            GpuVendor::Nvidia,
        );
        assert_eq!(mode, RenderMode::Native);
    }

    #[test]
    fn decide_mode_recovery_falls_back_to_safe() {
        let (mode, reason) = decide_mode(None, true, DisplayServer::Wayland, GpuVendor::Amd);
        assert_eq!(mode, RenderMode::Safe);
        assert!(reason.contains("crash recovery"));
    }

    #[test]
    fn decide_mode_x11_is_always_native() {
        for gpu in [GpuVendor::Nvidia, GpuVendor::Amd, GpuVendor::Intel, GpuVendor::Unknown] {
            let (mode, _) = decide_mode(None, false, DisplayServer::X11, gpu);
            assert_eq!(mode, RenderMode::Native, "X11 + {gpu:?} should be Native");
        }
    }

    #[test]
    fn decide_mode_wayland_nvidia_uses_light_touch() {
        let (mode, _) = decide_mode(None, false, DisplayServer::Wayland, GpuVendor::Nvidia);
        assert_eq!(mode, RenderMode::NvidiaLight);
    }

    #[test]
    fn decide_mode_wayland_amd_or_intel_disables_dmabuf_only() {
        for gpu in [GpuVendor::Amd, GpuVendor::Intel] {
            let (mode, _) = decide_mode(None, false, DisplayServer::Wayland, gpu);
            assert_eq!(mode, RenderMode::DmabufOff, "Wayland + {gpu:?} should be DmabufOff");
        }
    }

    #[test]
    fn decide_mode_wayland_unknown_vendor_is_conservative_dmabuf_off() {
        let (mode, _) = decide_mode(None, false, DisplayServer::Wayland, GpuVendor::Unknown);
        assert_eq!(mode, RenderMode::DmabufOff);
    }

    #[test]
    fn decide_mode_unknown_display_is_native() {
        let (mode, _) = decide_mode(None, false, DisplayServer::Unknown, GpuVendor::Nvidia);
        assert_eq!(mode, RenderMode::Native);
    }

    #[test]
    fn parse_config_mode_basic() {
        assert_eq!(parse_config_mode("mode = safe\n"), Some(ModeOverride::Explicit(RenderMode::Safe)));
        assert_eq!(parse_config_mode("mode=native"), Some(ModeOverride::Explicit(RenderMode::Native)));
        assert_eq!(parse_config_mode("mode = \"nvidia-light\""), Some(ModeOverride::Explicit(RenderMode::NvidiaLight)));
        assert_eq!(parse_config_mode("mode = auto"), Some(ModeOverride::Auto));
    }

    #[test]
    fn parse_config_mode_ignores_comments_and_other_keys() {
        let body = "\
# header comment\n\
something = else\n\
mode = dmabuf-off   # inline comment\n\
trailing = ignored\n\
";
        assert_eq!(parse_config_mode(body), Some(ModeOverride::Explicit(RenderMode::DmabufOff)));
    }

    #[test]
    fn parse_config_mode_returns_none_when_absent() {
        assert_eq!(parse_config_mode(""), None);
        assert_eq!(parse_config_mode("# just a comment\nother = value\n"), None);
    }

    #[test]
    fn parse_config_mode_returns_none_for_unknown_value() {
        assert_eq!(parse_config_mode("mode = lol"), None);
    }

    // ---------- crash-recovery + override resolution tests ----------

    #[test]
    fn resolve_user_override_env_wins_over_config_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(render_config_path(dir.path()), "mode = native").unwrap();
        // Env says safe; file says native; env wins.
        let r = resolve_user_override(Some("safe"), dir.path());
        assert_eq!(r, Some(ModeOverride::Explicit(RenderMode::Safe)));
    }

    #[test]
    fn resolve_user_override_falls_through_to_config_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(render_config_path(dir.path()), "mode = dmabuf-off\n").unwrap();
        let r = resolve_user_override(None, dir.path());
        assert_eq!(r, Some(ModeOverride::Explicit(RenderMode::DmabufOff)));
    }

    #[test]
    fn resolve_user_override_returns_none_when_neither_set() {
        let dir = tempfile::tempdir().unwrap();
        // No file, no env.
        assert_eq!(resolve_user_override(None, dir.path()), None);
    }

    #[test]
    fn resolve_user_override_ignores_garbage_env_then_tries_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(render_config_path(dir.path()), "mode = safe").unwrap();
        // Garbage env → fall through to file.
        let r = resolve_user_override(Some("nonsense"), dir.path());
        assert_eq!(r, Some(ModeOverride::Explicit(RenderMode::Safe)));
    }

    #[test]
    fn recovery_marker_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_recovery_needed(dir.path()));
        create_recovery_marker(dir.path());
        assert!(is_recovery_needed(dir.path()));
        clear_recovery_marker(dir.path());
        assert!(!is_recovery_needed(dir.path()));
        // Idempotent — clearing a non-existent marker is a silent no-op.
        clear_recovery_marker(dir.path());
        assert!(!is_recovery_needed(dir.path()));
    }

    #[test]
    fn create_recovery_marker_creates_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested").join("config");
        // Nested doesn't exist yet.
        assert!(!nested.exists());
        create_recovery_marker(&nested);
        // create_recovery_marker should have made it + the marker file.
        assert!(is_recovery_needed(&nested));
    }

    #[test]
    fn write_state_log_produces_inspectable_file() {
        let dir = tempfile::tempdir().unwrap();
        write_state_log(
            dir.path(),
            RenderMode::NvidiaLight,
            "test reason",
            DisplayServer::Wayland,
            GpuVendor::Nvidia,
            false,
            false,
        );
        let body = std::fs::read_to_string(render_state_log_path(dir.path())).unwrap();
        assert!(body.contains("mode = nvidia-light"));
        assert!(body.contains("reason = test reason"));
        assert!(body.contains("display = Wayland"));
        assert!(body.contains("gpu = Nvidia"));
        // Applied env should list both NVIDIA-light vars.
        assert!(body.contains("WEBKIT_DISABLE_DMABUF_RENDERER=1"));
        assert!(body.contains("__NV_DISABLE_EXPLICIT_SYNC=1"));
    }

    #[test]
    fn write_state_log_native_shows_no_env_applied() {
        let dir = tempfile::tempdir().unwrap();
        write_state_log(
            dir.path(),
            RenderMode::Native,
            "X11 session: WebKit defaults work reliably",
            DisplayServer::X11,
            GpuVendor::Amd,
            false,
            false,
        );
        let body = std::fs::read_to_string(render_state_log_path(dir.path())).unwrap();
        assert!(body.contains("mode = native"));
        assert!(body.contains("applied_env =\n  (none)"));
    }
}
