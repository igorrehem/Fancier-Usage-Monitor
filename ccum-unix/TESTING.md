# Testing `ccum-unix` on real macOS/Linux hardware

`ccum-unix` is the macOS/Linux port of Claude Code Usage Monitor, developed entirely on a
**Windows-only** machine (see "Environment limitation" below). This document is the handoff
guide for verifying it on real hardware: how to build it, what to check first, and — most
important — a prioritized list of every place the implementation is a best-effort guess rather
than something that was actually compiled and run on the target OS.

## Environment limitation (read this first)

Every commit in this port (`feat/macos-linux-port`, Tasks 1-14) was written and tested on a
**Windows 11 machine with no macOS toolchain and no Linux GTK3/OpenSSL cross-compilation
sysroot**. Concretely, that means:

- **macOS**: zero automated verification of any kind was possible, ever. No build, no `cargo
  check`, nothing — there is no Apple toolchain anywhere in this environment. Every macOS-specific
  code path (the `~/Library/Application Support` settings path, the Keychain `security` CLI
  credential read, the tray icon, the popup's top-right positioning convention) is based on
  reading crate/OS documentation and vendored crate source, never on running real macOS code.
- **Linux**: `cargo check --target x86_64-unknown-linux-gnu` was run repeatedly throughout the
  port, but it **never once reached this port's own `#[cfg(target_os = "linux")]` code** — it
  always failed first at one of two pre-existing build walls this machine can't clear:
  `native-tls`/`openssl-sys` needing a Linux OpenSSL sysroot (blocks `ccum-core`), and
  `tray-icon`'s Linux backend needing GTK3 + `libappindicator` dev headers via `pkg-config`
  (blocks `ccum-unix`). So Linux confidence rests on source-level reasoning and (for the
  `ccum-core`-only slice that gets past the GTK wall) pure-logic unit tests, not a real compiler
  pass over the Linux-specific branches.
- **Windows**: full confidence. Every task in this plan built, ran, and — largely via a
  synthetic-event-driven verification harness (see Task 14's report for the pattern) — exercised
  `ccum-unix`'s actual production code on this machine as a native `x86_64-pc-windows-msvc`
  binary, since `main.rs`/`panel.rs`/`render/*` have zero `#[cfg(target_os = ...)]` gates of their
  own (only `ccum-core`'s `poller.rs`/`settings.rs` and Antigravity credential reads are
  OS-gated). This is the best available proxy for real macOS/Linux behavior, but it is still a
  proxy — winit/tiny-skia/cosmic-text/softbuffer/tray-icon all have real per-OS backend code this
  never exercised.

**What this means for confidence**: treat everything below "Known limitations" as verified by
code review and Windows-as-proxy testing, not by running real macOS/Linux code. The prioritized
list in this document exists specifically so a real-hardware tester knows where to look first.

## How to build for real

From an actual macOS or Linux checkout (not this Windows machine):

```sh
# macOS: Xcode command-line tools must be installed (cc, the system SDK).
# Linux: needs GTK3 + libappindicator dev headers plus libxdo (e.g. Debian/Ubuntu:
#   sudo apt install libgtk-3-dev libappindicator3-dev libxdo-dev pkg-config
# ), plus OpenSSL dev headers (libssl-dev) for native-tls.
cargo build -p ccum-unix --release
```

Verified on real Linux (Ubuntu 24.04 server, 2026-07-12): with the packages above the whole
workspace slice (`ccum-core` + `ccum-unix`) compiles, links, and passes its test suite natively.
Two gaps the original guess-list missed, both found on that first run:

- **`libxdo-dev` is required at link time** (`rust-lld: unable to find library -lxdo`) — it is a
  real dependency of `muda`/`tray-icon`'s Linux backend, missing from the original package list.
- **`libxkbcommon-x11` is required at runtime** (Debian/Ubuntu package `libxkbcommon-x11-0`):
  `winit`'s X11 backend dlopens it and aborts at startup when absent. Present on normal desktop
  installs, missing on minimal/server systems. Under a headless `xvfb-run` smoke test the app
  then runs and stays alive; GTK prints tray-related `Gtk-CRITICAL` warnings because no
  StatusNotifier tray host exists there (the known limitation in item 1 below), but does not
  crash.

The binary lands at `target/release/ccum-unix` (no `.exe` extension, unlike the Windows build).
There is no installer/packaging step — see "Known, accepted limitations" below.

If the build fails on Linux with a `pkg-config`/GTK error, install the dev packages named above
first; this is not a code bug, it's `tray-icon`'s real Linux dependency (confirmed and documented
since Task 4 — see "Consolidated low-confidence list" below for the exact package names other
distros may use).

## What to check first, in priority order

1. **Does the tray/menu-bar icon appear at all?**
   - **Linux gotcha (known limitation, not a bug to report)**: `tray-icon`'s Linux backend is
     built on GTK3 + `libappindicator`, which in turn needs a **StatusNotifierItem-compatible
     tray host** to actually display anything at runtime. KDE Plasma supports this out of the
     box. Vanilla GNOME Shell does **not** show AppIndicator-style tray icons without the
     "AppIndicator and KStatusNotifierItem Support" GNOME Shell extension installed. Minimal
     window managers (i3, sway, bare Xorg) have no tray host at all unless one is running
     separately (e.g. `stalonetray`, `polybar`'s tray module). If the icon doesn't appear on
     Linux, check the desktop environment/tray host **before** assuming the app is broken.
   - On macOS, the icon should appear in the menu bar; if the menu bar is very full (many other
     menu-bar apps), it can be pushed off-screen by macOS's own overflow behavior — check
     `~/Library/Preferences` menu-bar-overflow settings if it seems to be "missing."
2. **Does clicking the icon open the popup panel?** A single left click should toggle it open
   (and close it again on a second click, or a click anywhere outside it). No context menu is
   implemented — right-click currently does nothing (this is disclosed, matches this task's
   scope, not a bug).
3. **Do all 6 settings sections render and respond to clicks?** Appearance (the RgbaPicker
   popover — open it, pick a quick swatch, expand "Custom…" and drag a slider), Font (family
   dropdown, size slider, weight segmented control), Size (6 geometry sliders), Animations (4
   on/off toggles + their sliders, plus the standalone Reduce Motion toggle), Update (frequency
   segmented control + custom-minutes slider), Presets (scroll through all 24 theme cards across
   the 3 category headers — Built-in/Code editors/Apps — and click one from a scrolled-down
   position, not just the first visible row).
4. **Do Save/Cancel/Reset behave correctly?** Save should persist to disk and close the panel;
   Cancel should discard edits and close without touching disk; Reset should revert the draft to
   defaults (preserving window position fields) **without** closing the panel or touching disk.
   Restart the app afterward and confirm a Saved change survived.
5. **Does non-Latin text render correctly anywhere text is user-controlled or localized** (e.g.
   if you wire in a non-English `ccum_core::localization::Strings` table — see "i18n status"
   below)? Real glyphs, not tofu/empty boxes. Task 14 confirmed this works correctly on Windows
   via DirectWrite-backed `cosmic-text`/fontdb; a real Linux (fontconfig) or macOS (CoreText)
   font-fallback pass has never been checked (see item 3 in the low-confidence list).

## i18n status (as of Task 14)

`ccum-unix` does **not** currently render any localized strings — every section label, button,
and field name in `panel.rs`/`render/*` is a hardcoded English literal (this mirrors the original
Windows settings window's own phased history: it hardcoded English first too, with real i18n
added in a later task). Task 14 confirmed `cosmic-text` genuinely CAN render non-Latin scripts
correctly (Japanese and Russian, both via real `ccum_core::localization::Strings` values,
screenshotted with real glyphs, no tofu) using a temporary, fully-reverted test harness — see
Task 14's report for the screenshots and exact strings used. Wiring real i18n into
`ccum-unix`'s settings panel (the equivalent of the original Windows plan's Task 18) is a
reasonable next task, not done here.

## Consolidated low-confidence list (Tasks 1-13), highest risk first

Every implementer in this plan was asked to honestly flag low confidence rather than assert
something they couldn't verify. This section compiles every such flag found by re-reading
`.superpowers/sdd/mlp-task-1-report.md` through `mlp-task-13-report.md`, ranked by how likely
each is to actually cause a visible problem on real hardware.

1. **Antigravity credential read on macOS/Linux (Task 2) — the single most speculative code in
   the whole port.**
   - **macOS** (`security find-generic-password -s <target> -w`): confidence **medium**. The
     search-attribute (`-s <target>`) naming convention was never confirmed against a real macOS
     Antigravity installation — it's "the most standard-shaped guess," per Task 2's own report.
   - **Linux** (`secret-tool lookup service "<target>"`, via `libsecret-tools`): confidence
     **low**, explicitly marked `TODO(macos-linux-port)` in the source. Linux has no single
     standard secret store for CLI tools (libsecret/GNOME Keyring vs. KWallet vs. a plain file are
     all plausible). If Antigravity on Linux uses a different backend/schema, this **fails
     closed** (returns `None`, Antigravity usage silently doesn't show) rather than crashing —
     but that silent failure is exactly the kind of thing to check first if Antigravity usage
     never appears on a real Linux box with a real Antigravity install.
   - **If you have a real macOS/Linux machine with Antigravity installed and signed in**, this is
     the highest-value single thing to verify: run `security find-generic-password -s
     <the-real-target-string> -w` (macOS) or `secret-tool lookup service <the-real-target-string>`
     (Linux) by hand against a real credential store and compare to what `poller.rs` expects.

2. **This port's Linux `#[cfg(target_os = "linux")]` code has never been compiled, at all**
   (Tasks 2, 4, 6-13). Every Linux `cargo check` run throughout this whole plan failed at one of
   two pre-existing, environment-only walls — `native-tls`/`openssl-sys` needing a Linux OpenSSL
   sysroot, or `tray-icon`'s GTK3/`libappindicator` `pkg-config` requirement — before ever
   reaching this port's own code. Concretely: the macOS/Linux `settings_path()` branches
   (Task 13), the POSIX poller mechanisms (Task 2), and literally all of `ccum-unix`'s
   winit/tiny-skia/cosmic-text/tray-icon rendering code have only ever been read and reasoned
   about for Linux, never type-checked by `rustc` for that target. A clean `cargo build -p
   ccum-unix --release` on a real Linux box with the right dev packages installed is the actual
   first real compiler pass this code will ever get.

3. **macOS has zero automated verification of any kind** (Task 4 onward) — no Apple toolchain
   exists anywhere this port was developed. Every macOS-specific decision (the
   `~/Library/Application Support` settings path via `dirs::config_dir()`, verified only by
   reading the vendored `dirs` crate's own macOS source — see Task 13; the Keychain credential
   read; the popup's top-right positioning convention for the menu bar) is unverified beyond
   source-level reasoning.

4. **DPI/scale-factor handling is completely unported, on purpose, everywhere** (Tasks 6, 7, 9,
   10, 11, 12, 13 all independently disclose this same scope boundary). Every layout constant in
   `ccum-unix` — the usage bars, the popup panel shell, all 6 settings sections, the RgbaPicker
   popover — is used at its raw 96-DPI-baseline pixel value with no scale-factor awareness at
   all. This is internally consistent (Task 7's `PhysicalSize` fix keeps the canvas matching the
   geometry it's drawn from, so nothing is visually broken/misaligned), but on a real HiDPI
   display the whole app will render smaller relative to the rest of the OS's chrome than a
   DPI-aware app would. This is the single most likely "looks off on my real machine" report a
   first-time real-hardware tester is likely to file — it is a known, accepted gap, not a bug to
   re-report (see "Known, accepted limitations" below), but worth confirming it's merely
   "smaller than expected," not something worse (garbled/misaligned) on a real Retina/HiDPI panel.

5. **Task 9's tray-click/focus-loss race fix was verified with synthetic timestamps, not real OS
   focus-event timing.** The fix (`Panel::focus_lost_at`/`suppress_reopen`) prevents a
   tray-icon click from reopening a panel it just closed, but its test coverage drives
   `handle_focus_change`/`suppress_reopen` directly with hand-set `Instant`s, bypassing real OS
   focus-change delivery timing entirely. Windows' own message-queue ordering was reasoned about
   explicitly; a real macOS/Linux window manager's focus-event ordering/timing relative to
   `tray-icon`'s click relay was never checked and could plausibly differ. If tray-icon clicks
   ever seem to "not register" or "flicker open-then-closed" on real hardware, start here.

6. **The fade animation is not pixel-identical to the Windows original** (Task 6/7). There is no
   `UpdateLayeredWindow`-equivalent per-pixel alpha compositing available in this rendering
   stack, so `ccum-unix`'s fade is reimplemented as a color-lerp toward the background color
   instead. Explicitly disclosed as reaching "the same visible end-state," not a literal port —
   worth a side-by-side glance on real hardware, not expected to be a functional problem.

7. **`tray_icon::TrayIcon::rect()` is documented unsupported on Linux** (Task 9), so the popup
   panel's "anchor near the tray icon" positioning falls back to a fixed screen-corner heuristic
   (bottom-right) on Linux rather than tracking the icon's actual position. This is expected
   (not a bug) but means the popup will NOT visually anchor to the tray icon on Linux the way it
   does on Windows/macOS.

8. **No real OS-level mouse click was ever physically delivered to this app, anywhere in this
   whole plan** (Tasks 8-13, and this task's own Task 14 run-through). `GetCursorPos`/screen
   capture both fail in every environment this port was developed in (no attached interactive
   input device). Every single interaction test — tray clicks, section navigation, dragging
   sliders, opening the RgbaPicker popover, scrolling the Presets grid, clicking Save/Cancel/
   Reset — was verified via a synthetic-event-driven harness calling the exact same production
   dispatch functions a real OS event would call (`Panel::handle_window_event`,
   `App::handle_tray_event`, etc.), not a literal physical click delivered by the OS. This is a
   structural property of the whole verification methodology used across this plan, not a gap in
   any one task — but it means "click things with a real mouse on real hardware" has, in a very
   literal sense, never actually happened yet for any part of this UI.

9. **Font-fallback/glyph coverage was only checked against this Windows machine's installed font
   set** (Task 14). `cosmic-text`'s `FontSystem`/fontdb integration is genuinely
   cross-platform (DirectWrite on Windows, CoreText on macOS, fontconfig on Linux), and Task 14
   confirmed real Japanese and Russian glyphs render correctly here — but that confirmation used
   whatever CJK/Cyrillic-capable fonts happen to already be installed on this Windows box. A
   minimal/fresh Linux distro install or a locked-down macOS profile could plausibly have a
   narrower default font set with worse cross-script fallback; this was never checked.

10. **Task 2's POSIX WSL-bridge replacement dropped login-shell (`bash -lic`) semantics.** The
    original Windows path shelled a command through WSL with login-shell semantics (sourcing
    profile scripts); the direct POSIX replacement runs the same inner script via a plain `sh -c`
    without that login-shell wrapping. This fails closed (a missing PATH entry just means the
    underlying command isn't found, not a crash), but if a real macOS/Linux user's `claude`/
    `codex` CLI is only on `PATH` via a shell profile script (not a system-wide install), polling
    could silently fail to find it. Low-priority since it's disclosed and fails safely, but worth
    knowing about if polling comes back empty on a real machine that does have the CLI installed.

## Known, accepted limitations (NOT bugs to report back)

These are deliberate, already-disclosed scope boundaries from the design spec and/or specific
tasks — please don't file these as newly-discovered bugs:

- **No DPI-awareness** (see item 4 above — Tasks 6-7's documented scope boundary).
- **No window-decoration/borderless-panel polish beyond what Task 9 shipped.** The popup panel
  is a plain borderless `winit` window (`with_decorations(false)`); there is no drop shadow,
  rounded-corner window chrome, or platform-native flyout animation.
- **No auto-update mechanism.** Out of scope per the design spec — `ccum-unix` has no equivalent
  of the Windows build's update-checking/self-update flow.
- **No packaging or installers** (no `.dmg`, `.pkg`, `.deb`, `.rpm`, AppImage, Flatpak, etc.).
  Out of scope per the design spec — building from source via `cargo build --release` is the
  only supported path today.
- **No right-click context menu.** A single left click toggles the popup panel; there is
  currently no equivalent of the Windows tray icon's right-click menu (refresh, models,
  frequency, language, startup, exit).
- **Presets section has no word-wrap for its intro sentence** (drawn single-line, per
  `render::presets`'s own doc comment — `TextRenderer` has no word-wrap primitive yet).
