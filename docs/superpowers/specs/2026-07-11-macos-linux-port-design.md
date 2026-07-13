# macOS + Linux Port — Design Spec

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this. This spec builds on the existing Windows app (branch `main` on `igorrehem/Fancier-Usage-Monitor`, post-merge of the settings-window + RGBA-picker/themes features).

**Goal:** Ship the same feature set (usage monitoring for Claude Code/Codex/Antigravity, full settings window with 24 style presets, RGBA popover, animations) on macOS and Linux, using each OS's native tray/menu-bar idiom for the entry point, while sharing as much implementation as possible between the two new platforms.

**Motivation:** User request: "implementa uma versão pra macOS e uma pra Linux também." Full feature parity confirmed during brainstorming — not a stripped-down MVP.

**IMPORTANT — environment constraint on THIS implementation pass:** this codebase is being developed from a Windows-only machine. `ccum-windows` (this spec's Phase 1) is fully buildable, runnable, and testable here — zero change to that guarantee. `ccum-unix` (Phase 2+) can be written and, at best, `cargo check`'d against a cross-compilation target (no linker available, so not even a full `cargo build`) — it CANNOT be linked, run, or visually verified from this environment. Every unix-targeting task in the implementation plan must say so explicitly and mark manual/runtime verification as "not possible from this environment, needs the user's own macOS/Linux hardware" rather than silently skipping or fabricating verification claims.

## 1. Workspace restructure

Convert the single-crate binary into a Cargo workspace:

```
Cargo.toml                 # [workspace] members = ["ccum-core", "ccum-windows", "ccum-unix"]
ccum-core/                 # lib crate, zero windows-crate/AppKit/GTK dependencies
  src/
    settings.rs             # moved verbatim (Settings/Appearance/Typography/Geometry/AnimationSettings/PresetId)
    presets.rs               # moved verbatim (apply_preset, theme_display_name/theme_category, all 24 presets)
    animation.rs              # moved verbatim (easing, AnimationClock, AnimationFrame)
    poller.rs                 # moved, already `sh -c`/`$HOME`-based -- audit for any Windows-only path assumptions during the move, don't assume zero changes needed
    lib.rs
ccum-windows/               # existing binary, now depends on ccum-core
  src/
    window.rs, config_window.rs, controls.rs, native_interop.rs, tray_icon.rs, localization/, updater.rs, main.rs
    (all UNCHANGED in behavior -- this is a pure extraction, re-pointing `use crate::settings::X` to `use ccum_core::settings::X` etc.)
ccum-unix/                  # new binary, macOS + Linux, cfg-gated only where the two differ
  src/
    render.rs                 # tiny-skia port of controls.rs/config_window.rs's drawing (bars, RgbaPicker popover, preset cards, sliders/dropdowns/toggles)
    window.rs                 # winit event loop + window management for the popup panel
    tray.rs                   # tray-icon crate integration; renders the mini-bar icon bitmap per poll tick
    settings_paths.rs         # macOS (~/Library/Application Support/ClaudeCodeUsageMonitor/) vs Linux (XDG: ~/.config/claude-code-usage-monitor/) path resolution, `#[cfg(target_os = ...)]`
    main.rs
```

**Zero-regression contract for `ccum-windows`:** after Phase 1, `cargo build --release` on this Windows machine must produce a byte-for-byte-behaviorally-identical app (same tests passing, same manual run-through as the existing settings-window feature). This is the one part of this whole initiative that gets FULL verification in this environment, and it must not be compromised for the sake of speed on the unix side.

## 2. `ccum-unix` rendering: `winit` + `tiny-skia`

- `winit` creates the popup panel window (borderless or minimal-chrome, shown/hidden on tray-icon click, matching the "native idiom" entry point + "same visual popup" decision from brainstorming) on both macOS and Linux via the same code path — `winit` already abstracts the platform windowing difference.
- `tiny-skia` is a pure-CPU 2D rasterizer (paths, fills, gradients, text via a separate shaping step) — the target backend for porting `controls.rs`'s draw logic (currently GDI `FillRect`/`DrawTextW`/`BitBlt` calls) to `tiny-skia`'s `Pixmap`/`Paint`/`Path` API. This is a genuine, non-trivial port (different drawing primitives, different text-rendering story — GDI's `DrawTextW` has no tiny-skia equivalent; text will need a shaping+rasterization crate, e.g. `cosmic-text` or `fontdue`, chosen during implementation and confirmed against the "no GPU, lightweight" constraint), not a mechanical find-replace. Budget real design time for this specifically once it's reached.
- Animation timing (`AnimationClock`/`AnimationFrame` from `ccum-core`) is already platform-agnostic (`std::time::Duration`-based) — no port needed, just a different "tick" driver (a `winit` timer/redraw-request loop instead of Win32 `SetTimer`/`WM_TIMER`).

## 3. Tray/menu-bar entry point

- `tray-icon` crate (cross-platform: wraps `NSStatusItem` on macOS, `StatusNotifierItem`/`libappindicator` on Linux) for the icon itself.
- Icon bitmap is rendered by US every poll tick (not a static icon) — a compact mini-bar via the same `tiny-skia` rendering core, exported as the small bitmap `tray-icon` expects, per the "continuous visual feedback" decision from brainstorming.
- Click opens/closes the `winit` popup panel (full settings window UI, same 6 sections, same 24 presets, same RGBA popover) anchored near the tray icon.
- Linux caveat to flag explicitly in the implementation plan (not resolvable by us — DE-dependent): some desktop environments (notably GNOME without an extension) don't support the tray-icon protocols at all. Document this as a known limitation, not a bug to fix.

## 4. Settings persistence

- macOS: `~/Library/Application Support/ClaudeCodeUsageMonitor/settings.json`
- Linux: XDG Base Directory — `$XDG_CONFIG_HOME/claude-code-usage-monitor/settings.json`, falling back to `~/.config/claude-code-usage-monitor/settings.json` if `XDG_CONFIG_HOME` is unset.
- Schema is IDENTICAL to Windows (`ccum-core::settings::Settings`, shared) — a `settings.json` is not portable between OSes today (different paths) but the JSON shape itself is the same, which matters if a future task ever wants cross-device sync.

## 5. Out of scope for this pass

- Packaging/installers (.dmg, .deb, AppImage, Homebrew/apt manifests) — ship as a plain cargo-built binary for now, matching how Windows ships as a raw `.exe` + separate WinGet manifest (packaging can be a later, separate initiative).
- Auto-update (`updater.rs`'s GitHub-release-polling mechanism) — Windows-specific today, not ported in this pass; unix binaries won't self-update.
- Actual runtime verification on macOS/Linux — explicitly not possible from this environment (see the environment-constraint note above). The user will need to build+run on their own hardware to confirm the unix side actually works; this pass produces code believed-correct via static review, `cargo check` where possible, and unit tests for anything platform-agnostic, not a runtime-verified working app.

## 6. Testing

- `ccum-core`: all existing unit tests (settings migration, presets, animation clock, poller parsing logic where it doesn't shell out) move with their modules and must keep passing under `cargo test -p ccum-core`.
- `ccum-windows`: all existing tests + manual run-through, unchanged, fully verifiable here.
- `ccum-unix`: unit tests for anything pure-logic (tiny-skia path-building math, layout calculations mirroring the Windows `*_layout` functions' testing style) — these DO run and verify on this Windows machine via cross-compiled `cargo test --target x86_64-unknown-linux-gnu` if that target supports test execution without linking (likely does NOT — flag this precisely in the plan; if `cargo test` can't run cross-compiled, fall back to testing the pure-logic pieces as `#[cfg(test)]` modules that get compiled and run on the HOST target too, e.g. put layout math in `ccum-core` or a target-agnostic sub-module so it's tested via the normal Windows-hosted `cargo test`, not skipped entirely).
