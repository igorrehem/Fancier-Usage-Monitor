# macOS + Linux Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure into a Cargo workspace with a shared, OS-agnostic `ccum-core` crate, then build `ccum-unix` (macOS + Linux, native tray entry point + winit/tiny-skia popup panel) alongside the untouched `ccum-windows` binary.

**Architecture:** See `docs/superpowers/specs/2026-07-11-macos-linux-port-design.md` for the full design — this plan restates it as tasks.

**Tech Stack:** Rust workspace. `ccum-core`: no new deps (pure logic, already exists). `ccum-windows`: unchanged deps. `ccum-unix`: new deps `winit`, `tiny-skia`, `tray-icon`, plus a text-rendering crate chosen during Task 4 (candidates: `cosmic-text`, `fontdue` — final choice and rationale documented in that task's report, not pre-decided here).

## Global Constraints

- `ccum-windows` must remain behaviorally IDENTICAL after Phase 1 (Tasks 1-3) — every existing test passes, every existing manual-verification scenario still works, zero regression. This is the one part of this whole plan that gets full build+run+test verification in this development environment.
- `ccum-unix` (Tasks 4+) CANNOT be linked, run, or visually verified from this Windows-only development environment. Every unix task's verification section must say so explicitly (`cargo check` against a cross target at best, no linker available) rather than claiming untested code works.
- No GPU rendering anywhere (`tiny-skia` is CPU-only by design — verify any chosen text-rendering crate is also CPU-only, not a GPU-backed shaping pipeline).
- Commit messages end with:
  ```
  Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_015nrrhG6NU1CQ9UGn5nvwJs
  ```
- Build check after each Windows-affecting task: `.\dev.ps1 build` + `.\dev.ps1 test` (clean, 0 failures). Unix tasks: `cargo check -p ccum-unix --target x86_64-unknown-linux-gnu` (or note if even that isn't achievable, e.g. missing system deps for `tray-icon`'s Linux backend headers) — document the exact command run and its output.

---

## Phase 1 — Workspace restructure (fully verifiable, do this first and get it rock-solid)

### Task 1: Convert to a Cargo workspace, extract `ccum-core`

**Files:**
- Create: `ccum-core/Cargo.toml`, `ccum-core/src/lib.rs`
- Move: `src/settings.rs` → `ccum-core/src/settings.rs`, `src/presets.rs` → `ccum-core/src/presets.rs`, `src/animation.rs` → `ccum-core/src/animation.rs`, `src/poller.rs` → `ccum-core/src/poller.rs` (and any small modules those four exclusively depend on — audit imports first, see Step 1)
- Create: `ccum-windows/Cargo.toml` (existing binary's manifest, now depending on `ccum-core` via a workspace path dependency)
- Move: everything else from `src/` → `ccum-windows/src/` (`window.rs`, `config_window.rs`, `controls.rs`, `native_interop.rs`, `tray_icon.rs`, `localization/`, `updater.rs`, `main.rs`)
- Modify: root `Cargo.toml` becomes a `[workspace]` manifest listing `members = ["ccum-core", "ccum-windows"]` (add `"ccum-unix"` in Task 4)

**Interfaces:**
- Produces: `ccum_core::settings::{Settings, Appearance, Typography, Geometry, AnimationSettings, PresetId, Rgba, PaletteStops, Weight, Easing, load, save}`, `ccum_core::presets::{apply_preset, theme_display_name, theme_category, PresetCategory}`, `ccum_core::animation::{AnimationClock, AnimationFrame, ease}`, `ccum_core::poller::{poll, UsageData, PollError}` (or whatever poller's actual current public surface is — audit before moving, don't guess).
- Consumes (by `ccum-windows`): the above, via `use ccum_core::settings::Settings` etc. replacing every current `use crate::settings::Settings`-style import.

- [ ] **Step 1: Audit before moving.** Read `src/settings.rs`, `src/presets.rs`, `src/animation.rs`, `src/poller.rs` in full and grep each for `use crate::` / `use super::` references to anything OUTSIDE these four files (e.g. does `poller.rs` reach into `native_interop.rs` or `window.rs` for anything? Does `settings.rs` reference `theme::Color` — check, `Color` currently lives in `native_interop.rs` per this codebase's established layout, which is Windows-specific/GDI-adjacent). Any such cross-reference needs a decision: either the referenced item also moves into `ccum-core` (if it's genuinely platform-agnostic, e.g. a plain RGBA struct with no GDI calls) or `ccum-core`'s code needs a small adjustment to not depend on it (e.g. `Rgba::to_color()` returning a `ccum_core`-local color type instead of `native_interop::Color`, with `ccum-windows` doing the `Rgba → native_interop::Color` conversion at its own boundary instead of inside `ccum-core`). Document every cross-reference found and the resolution chosen — this is the highest-judgment step in the whole extraction, get it right before moving code.
- [ ] **Step 2:** Create the workspace `Cargo.toml`, `ccum-core/Cargo.toml` (name `ccum-core`, appropriate `serde`/`serde_json` deps carried over from the current root manifest, nothing Windows-specific), move the four files + `mod.rs`/`lib.rs` wiring, fix every import.
- [ ] **Step 3:** Create `ccum-windows/Cargo.toml` (same binary name/metadata as today's root `Cargo.toml`, now with a `ccum-core = { path = "../ccum-core" }` dependency plus its existing `windows`-crate/etc. deps), move the remaining `src/*` files into `ccum-windows/src/`, fix every `use crate::settings::X` → `use ccum_core::settings::X` (and similarly for presets/animation/poller) across the moved files.
- [ ] **Step 4: Build** — `.\dev.ps1 build` (you'll likely need to `cd ccum-windows` or adjust `dev.ps1`/invoke `cargo build -p ccum-windows` from the workspace root — check `dev.ps1`'s current `Set-Location $PSScriptRoot` behavior and adjust if needed for the new workspace layout, but don't rewrite `dev.ps1`'s whole structure, just what's needed for the workspace). Must be clean, no new warnings, and — critically — the built binary's location/name should still work with the existing Windows Startup registry entry (`ClaudeCodeUsageMonitor` → `target\release\claude-code-usage-monitor.exe`) — verify the workspace build still produces the binary at the SAME relative path (`target/release/claude-code-usage-monitor.exe` from the repo root, not `ccum-windows/target/release/...` — Cargo workspaces share one `target/` at the workspace root by default, so this should hold, but confirm it rather than assume).
- [ ] **Step 5: Test** — `.\dev.ps1 test` (or `cargo test --workspace`). Must show the SAME test count/names as before the extraction (just now split across `ccum-core`'s and `ccum-windows`'s own test binaries) — 0 failures, and confirm no test was silently dropped in the move (compare the full test name list before/after, not just the pass count, since a moved-but-orphaned test module would silently vanish rather than fail).
- [ ] **Step 6: Manual** — run the actual built app (`.\dev.ps1 run` or launch the built exe directly), confirm the widget renders identically to before, right-click → Settings… still opens and works identically (spot-check a couple of sections/presets), confirm the Windows Startup registry path still resolves correctly if "Start with Windows" is checked.
- [ ] **Step 7: Commit** — `refactor: extract ccum-core workspace crate (settings/presets/animation/poller), zero behavior change`.

### Task 2: `poller.rs` OS-path audit

**Files:** Modify `ccum-core/src/poller.rs` (only if Task 1's Step 1 audit found something)

**Interfaces:** No new public interface — this task hardens what Task 1 moved, it doesn't add features.

- [ ] **Step 1:** Re-read the moved `poller.rs` specifically for anything that assumes Windows path conventions (backslashes, `%APPDATA%`/`%USERPROFILE%` env vars, WSL-bridge-specific shell invocation quirks) versus things that are already POSIX-flavored (`~/.claude/...`, `sh -c`, `$HOME`) and would work as-is on macOS/Linux. Since this module already shells out via `sh -c` (confirmed present in the current code), most of it is likely already portable — this task is a confirmation/documentation pass, not a rewrite, unless the audit finds a genuine Windows-only assumption.
- [ ] **Step 2:** If a genuine Windows-only assumption is found (e.g. a hardcoded `%APPDATA%`-style path used as a fallback before shelling out), fix it to be `#[cfg(target_os = "windows")]`-gated with a portable equivalent for other targets, OR confirm via a code comment that the existing shell-based path is the ONLY path used (no OS-specific fallback needed) if that's what the audit finds.
- [ ] **Step 3: Test** — `.\dev.ps1 test` (or `cargo test -p ccum-core`). 0 failures, same as Task 1's baseline.
- [ ] **Step 4: Commit** — `docs: confirm poller.rs cross-platform path handling` (or `fix: ...` if a real Windows-only assumption was corrected).

### Task 3: `ccum-windows` regression pass

**Files:** none expected (verification-only, unless Task 1/2 left something to polish)

- [ ] **Step 1:** Full `.\dev.ps1 test` + `.\dev.ps1 release` — clean, 0 failures.
- [ ] **Step 2: Manual full run-through** of the settings window (every section, a few presets, the RGBA popover, Save/Cancel/Reset) against the freshly-workspace-restructured build, confirming genuinely zero behavior change from before Task 1.
- [ ] **Step 3: Commit** — `docs: Phase 1 (workspace restructure) verification complete` if anything needed fixing during this pass; otherwise this task produces no commit (note that in your report instead).

---

## Phase 2 — `ccum-unix` skeleton (NOT runtime-verifiable from this environment)

### Task 4: `ccum-unix` crate skeleton + text-rendering crate selection

**Files:**
- Create: `ccum-unix/Cargo.toml`, `ccum-unix/src/main.rs`
- Modify: root `Cargo.toml` workspace `members` to add `"ccum-unix"`

**Interfaces:**
- Produces: a `ccum-unix` binary crate that depends on `ccum-core`, `winit`, `tiny-skia`, `tray-icon`, and a chosen text-rendering crate. `main.rs` at this task's scope just needs to: parse no meaningful args, construct a `winit::event_loop::EventLoop`, create ONE blank window, run the event loop until closed — a true "hello world" skeleton, no actual usage-monitor UI yet (that's later tasks).

- [ ] **Step 1:** Research and pick the text-rendering crate (compare `cosmic-text` vs `fontdue` vs alternatives on: CPU-only rendering — no GPU dependency, compatibility with feeding rasterized glyphs into a `tiny-skia::Pixmap`, crate maturity/maintenance, binary-size impact). Document the choice and why in this task's report — this is a real decision affecting every later rendering task, don't defer it.
- [ ] **Step 2:** Create `ccum-unix/Cargo.toml` with the chosen dependencies, `ccum-unix/src/main.rs` with the minimal `winit` event loop skeleton described above.
- [ ] **Step 3: Verify** — `cargo check -p ccum-unix --target x86_64-unknown-linux-gnu` (the Linux target should be installed in this environment — confirm with `rustup target list --installed` first; if `cargo check` fails due to missing system headers/libs that `tray-icon`'s Linux backend needs at build time even for a check — e.g. `libappindicator`/`gtk` dev headers not present on this Windows machine — document the EXACT error and treat it as an expected, disclosed limitation, not something to work around by disabling features silently). Do NOT attempt `cargo build`/`cargo test` for this crate (no linker for the Linux target on this machine) — `cargo check` (type-check only, no codegen/link) is the ceiling of what's achievable here.
- [ ] **Step 4: Report clearly** what level of verification was actually achieved (does it even type-check? if not, why, and is the "why" a real code problem or an environment limitation like missing system libs) — do not claim more than what `cargo check`'s actual output supports.
- [ ] **Step 5: Commit** — `feat: ccum-unix crate skeleton (winit event loop, no rendering yet)` — commit regardless of whether `cargo check` fully succeeded, as long as the code is a genuine, reasoned attempt (a partially-blocked-by-environment skeleton is still real progress and should be committed with its limitations documented in the commit body).

### Task 5: STOP AND CHECK IN

This is not a normal task — it's a mandatory pause. After Task 4, the orchestrator (not a subagent) must report back to the user with: what was achieved in Phase 1 (fully verified) and Task 4 (partially verified, environment-limited), and explicitly ask whether to continue deeper into Phase 2/3 (the full `tiny-skia` rendering port — thousands of lines of genuinely unverifiable-from-this-environment code) or pause here so the user can set up actual macOS/Linux hardware (or a CI runner) to provide a real feedback loop before more code gets written blind. Do not proceed past this point without that check-in.

**Resolution (2026-07-11): user chose to continue.** Phase 3+ below decomposes the remaining design-spec scope (§2-4) into tasks. Every task in Phase 3+ inherits the same verification ceiling disclosed above — Windows-side additions (none expected) get full verification, `ccum-unix` additions get best-effort `cargo check`/native-Windows-build-as-circumstantial-evidence only, per Task 4's established pattern.

---

## Phase 3 — Rendering foundation (`ccum-unix`)

### Task 6: `tiny-skia` + `cosmic-text` rendering primitives + double-buffered paint pipeline

**Files:**
- Create: `ccum-unix/src/render/mod.rs` (or `render.rs` if a single file stays manageable — implementer's call, split if it grows large), `ccum-unix/src/render/text.rs` (cosmic-text integration)
- Modify: `ccum-unix/src/main.rs` (wire the new paint pipeline into the winit redraw cycle)

**Interfaces:**
- Consumes: `tiny_skia::{Pixmap, Paint, Path, PathBuilder, Color as SkColor}`, `cosmic_text::{FontSystem, SwashCache, Buffer, Metrics, Attrs}` (or whatever `cosmic-text`'s actual current API surface is — Task 4's chosen version is 0.19.0, verify the real API against that version's docs/source rather than assuming an API shape).
- Produces: a small `Canvas` abstraction (or similarly-named type) wrapping a `tiny_skia::Pixmap` with helper methods mirroring `ccum-windows`'s GDI helpers' role — `fill_rect(rect, color)`, `fill_rounded_rect(rect, radius, color)`, `draw_text(pos, text, font_size, color)` — that later rendering tasks (bars, controls, sections) will build on, exactly like `window.rs`'s `fill_rect`/`draw_field_label` helpers did for the Windows GDI implementation. A `paint(canvas: &mut Canvas, ...)` entry point wired into `winit`'s `WindowEvent::RedrawRequested`, presenting the finished `Pixmap` to the actual window surface (research how `tiny-skia`-rendered pixels reach an on-screen `winit` window — likely via `softbuffer` crate, a common pairing for `tiny-skia`+`winit` software rendering; add it as a new dependency if so, and document why in this task's report, since it wasn't anticipated in Task 4's dependency list).

- [ ] **Step 1:** Research the `tiny-skia` → on-screen-pixels path for a `winit` window (very likely needs `softbuffer` or an equivalent software-presentation crate, since neither `tiny-skia` nor `winit` alone gets CPU-rendered pixels onto a window surface — `winit` only manages the window/event loop). Confirm this new dependency is still CPU-only (no GPU requirement) before adding it — this is a continuation of Task 4's "no GPU" constraint, don't relax it here.
- [ ] **Step 2:** Implement the `Canvas` abstraction with `fill_rect`/`fill_rounded_rect`/`draw_text` (and any other primitive later tasks will clearly need — keep this minimal, don't speculatively build primitives no later task asks for, per this whole plan's YAGNI discipline).
- [ ] **Step 3:** Wire double-buffering: build the frame into an off-screen `Pixmap` each `RedrawRequested`, then present it in one shot — mirroring the exact lesson learned from `ccum-windows`'s own flicker fix (commit `2b6d4f3` on `main`) — do not skip this, single-buffered CPU rendering into a live window is a well-known flicker source, and this codebase already paid to learn that lesson once on Windows; don't relearn it here.
- [ ] **Step 4: Verify.** Windows-side: N/A (no `ccum-windows` changes expected — confirm `.\dev.ps1 build`/`test` still pass unaffected regardless, as a matter of course). `ccum-unix` (best-effort): `cargo check -p ccum-unix --target x86_64-unknown-linux-gnu` (report the exact outcome, distinguishing "my code has an error" from "never reached my code" per the established pattern), AND — since Task 4 discovered this works and is genuinely informative — also try `cargo build -p ccum-unix` targeting the NATIVE `x86_64-pc-windows-msvc` (this crate has zero OS-gating so far; if it still builds/runs natively on Windows after this task's changes, that's real evidence, not just a formality) and, if it builds, actually RUN it and visually confirm SOMETHING renders (even a solid-color rectangle) in the window on this machine — this is the first task where you can get genuine visual confirmation of the rendering pipeline, even though the final target platforms are different, since `tiny-skia`+`softbuffer`(if used)+`winit` should behave identically in terms of the CPU-rasterization logic regardless of OS. Take this opportunity seriously — it's your best available signal before Phase 4.
- [ ] **Step 5: Commit** — `feat: tiny-skia/cosmic-text rendering primitives + double-buffered paint pipeline`.

### Task 7: Usage bar widget rendering

**Files:**
- Create: `ccum-unix/src/render/bars.rs`
- Modify: `ccum-unix/src/main.rs` (drive `ccum_core::animation::AnimationClock` on a timer, feed `AnimationFrame` into the bar renderer each tick)

**Interfaces:**
- Consumes: `ccum_core::animation::{AnimationClock, AnimationFrame}`, `ccum_core::settings::{Settings, AnimationSettings, Appearance}`, `ccum_core::models::{UsageData, UsageSection}`, Task 6's `Canvas`.
- Produces: `fn draw_bars(canvas: &mut Canvas, settings: &Settings, frame: &AnimationFrame, usage: &UsageData)` — the actual "does this look like a usage bar" rendering: label + filled bar (fill percentage from `frame.fill_pcts`, color from `settings.appearance`, shimmer/glow effects if enabled) + text, for however many sections are active (Claude Code/Codex/Antigravity) — this is a genuine PORT of `ccum-windows/src/window.rs`'s `paint_widget`/`draw_usage_bar`-equivalent logic (read that Windows code in full before starting — the animation MATH is identical since `ccum-core::animation` is shared, only the DRAWING calls change from GDI to `tiny-skia`).

- [ ] **Step 1:** Read `ccum-windows/src/window.rs`'s current bar-drawing logic in full (whatever it's now named post-Task-1's extraction) to understand exactly what visual elements exist and in what order they're drawn, so the port is faithful, not reimagined from scratch.
- [ ] **Step 2:** Implement `draw_bars` using Task 6's `Canvas` primitives, driven by a real `AnimationClock` ticking on a `winit` timer (research the right `winit` 0.30 mechanism for a periodic redraw — likely `event_loop.set_control_flow(ControlFlow::WaitUntil(...))` recomputed each frame, or a separate thread posting `UserEvent`s — pick one, document why).
- [ ] **Step 3: Verify** — same pattern as Task 6: best-effort Linux `cargo check`, and (valuable) a native Windows run to visually confirm the bars actually render and animate on this machine, comparing by eye against the real `ccum-windows` app's own bars for rough visual fidelity (not pixel-perfect, but "does this look like the same idea").
- [ ] **Step 4: Commit** — `feat: port usage bar rendering (fill/shimmer/glow/fade) to tiny-skia`.

### Task 8: Tray icon integration

**Files:**
- Create: `ccum-unix/src/tray.rs`
- Modify: `ccum-unix/src/main.rs` (wire tray-icon creation, click-to-toggle popup visibility, periodic icon-bitmap regeneration from real `ccum_core::poller::poll()` results)

**Interfaces:**
- Consumes: `tray_icon` crate's API (`TrayIconBuilder`, `Icon::from_rgba`), Task 7's `draw_bars` (or a compact variant of it sized for an icon — icons are much smaller than the popup panel, so this is likely a SEPARATE, simpler compact rendering path, not a literal reuse of `draw_bars` at a smaller canvas size — use your judgment, document the choice), `ccum_core::poller::poll()`.
- Produces: a real system tray/menu-bar icon that regenerates its bitmap each poll tick (matching the "continuous visual feedback" design decision from brainstorming) and toggles the popup panel (from Task 9, once it exists — if Task 9 isn't done yet when this task runs, stub the click handler to just log an event, don't block this task on a not-yet-existing later task).

- [ ] **Step 1:** Implement compact icon-bitmap rendering (a small mini-bar, sized per typical tray/menu-bar icon dimensions — research typical sizes, e.g. ~22x22 on Linux, ~18-22pt on macOS at the relevant scale factors).
- [ ] **Step 2:** Wire `tray_icon` crate creation + a real `ccum_core::poller::poll()` call on a timer, regenerating the icon bitmap from real (or, if polling genuinely fails in this dev environment — likely, since this is a Windows machine without the exact `claude`/`codex` POSIX setup Task 2's new code paths expect — clearly-fallback demo/placeholder) usage data each tick.
- [ ] **Step 3: Verify** — best-effort Linux check (note: `tray-icon`'s Linux backend needs GTK3 system libs per Task 4's finding, so this will likely hit the same wall — that's expected and fine, document it, don't try to work around it by disabling the tray feature just to get a green checkmark). Native Windows run: `tray-icon` DOES support Windows too (it's cross-platform) — if it builds/runs, confirm an actual tray icon appears in the Windows system tray on this machine and updates over time, which would be strong real evidence.
- [ ] **Step 4: Commit** — `feat: tray icon with live-rendered mini-bar, real poll-driven updates`.

---

## Phase 4 — Popup panel + settings port

### Task 9: Popup window shell + section-nav sidebar

**Files:** Create `ccum-unix/src/panel.rs` (window management: show/hide/position-near-tray-icon), `ccum-unix/src/render/sections.rs` (sidebar nav rendering, mirroring `ccum-windows`'s `config_window.rs` section-list)

**Interfaces:** Consumes Task 6-8's rendering pipeline + tray click events. Produces a togglable popup panel with the 6-section sidebar (Appearance/Font/Size/Animations/Update/Presets) rendered and clickable, content area still empty (sections' actual content is Tasks 10-12) — this task proves the panel shell + navigation works before any section's content is ported.

- [ ] **Step 1:** Read `ccum-windows/src/config_window.rs`'s section-nav layout/draw/dispatch logic (from the original settings-window plan) as the porting reference.
- [ ] **Step 2:** Implement the popup window (borderless or minimal chrome — research what's idiomatic per-platform via `winit`'s window-attribute options), positioned near the tray icon on open, with the sidebar rendered via Task 6's primitives and section-switch click handling.
- [ ] **Step 3: Verify** — native Windows run: open the popup via the tray icon, confirm the sidebar renders and section clicks switch the highlighted section (even with empty content areas).
- [ ] **Step 4: Commit** — `feat: popup panel shell with section-nav sidebar`.

### Task 10: Appearance section port (6 RgbaPickers + popover)

**Files:** `ccum-unix/src/render/sections.rs` (or split into `sections/appearance.rs` if it grows — mirror `ccum-windows`'s per-section file organization instinct even though that codebase kept everything in one `config_window.rs`; use your judgment on when a split earns its keep, this port is a good moment to start clean rather than recreate that file's eventual 3000+-line size problem from day one).

**Interfaces:** Consumes `ccum-core::settings::{Appearance, Rgba, PaletteStops}`, Task 6-9's rendering/panel infra. Produces the Appearance section's real content: 6 compact RgbaPicker-equivalent rows, each opening a popover (quick-swatch grid + Custom sliders) on click — a genuine PORT of `ccum-windows/src/controls.rs`'s `RgbaPicker` (from CT-Task 1/2 of the prior color-picker/themes plan) to `tiny-skia`, including its hit-testing logic (which is pure math, not GDI-specific, so the CORE hit-test logic — `popover_rect`, `swatch_cell_rect`, `custom_row_rect` — can likely port with only the coordinate-type/rect-type changed, not reimagined; read that code and reuse its logic/structure closely).

- [ ] **Step 1:** Read `ccum-windows/src/controls.rs`'s full `RgbaPicker` implementation (Task 1/2 of the color-picker/themes plan) as the porting reference — this is a substantial, already-battle-tested design (including the outside-click-bounds fix from that plan's own review cycle), don't redesign it, port it.
- [ ] **Step 2:** Implement the port, including click/hit-testing wired to `winit`'s pointer events (`WindowEvent::CursorMoved`/`MouseInput` — different event model than Win32's `WM_LBUTTONDOWN`, but the same underlying hit-test math applies once you have current cursor position + click state).
- [ ] **Step 3: Verify** — native Windows run: open Appearance, click through all 6 pickers, open a popover, pick a quick swatch, open Custom and drag a slider, confirm values visually update.
- [ ] **Step 4: Commit** — `feat: port Appearance section (RgbaPicker popover) to tiny-skia`.

### Task 11: Font/Size/Animations/Update sections port

**Files:** `ccum-unix/src/render/sections.rs` (or split files per Task 10's judgment call).

**Interfaces:** Consumes `ccum-core::settings::{Typography, Geometry, AnimationSettings}`, the remaining `ccum-windows/src/controls.rs` control types (`Dropdown`, `Segmented`, `Toggle`, bare `Slider`) as porting references. Produces the 4 simpler sections' real content.

- [ ] **Step 1:** Port `Slider`/`Dropdown`/`Segmented`/`Toggle` from `controls.rs` to `tiny-skia`-drawn equivalents (same porting philosophy as Task 10 — reuse the proven hit-test/layout math, change only the drawing calls).
- [ ] **Step 2:** Wire the 4 sections' actual field bindings (Font's family/size/weight, Size's 6 geometry sliders, Animations' 4 groups + reduce-motion, Update's frequency segmented + custom slider).
- [ ] **Step 3: Verify** — native Windows run: exercise every control in all 4 sections, confirm values change and (once Task 6/7's live bar rendering is wired into the popup preview, if this plan reaches that — otherwise defer preview-wiring to a later polish task and just confirm the controls themselves respond correctly) the settings actually update.
- [ ] **Step 4: Commit** — `feat: port Font/Size/Animations/Update sections to tiny-skia`.

### Task 12: Presets section port (24-card scrollable grid)

**Files:** `ccum-unix/src/render/sections.rs` (or split).

**Interfaces:** Consumes `ccum_core::presets::{apply_preset, theme_display_name, theme_category, ALL_PRESET_IDS, PresetCategory}`. Produces the full 24-preset, 3-category-grouped, scrollable card grid — a genuine PORT of `ccum-windows/src/config_window.rs`'s Presets section (from CT-Task 5 of the color-picker/themes plan), including its scroll-offset/hit-test consistency discipline (that plan's own review found and fixed a real bug there — read that history, in `.superpowers/sdd/progress.md`'s CT-Task 5 entry, before porting, so the same class of mistake isn't reintroduced fresh in the new rendering backend).

- [ ] **Step 1:** Read `ccum-windows/src/config_window.rs`'s Presets section (layout/draw/dispatch/scroll) in full as the porting reference, plus the CT-Task 5 review history for the scroll-offset bug class to avoid repeating.
- [ ] **Step 2:** Implement the port, including mouse-wheel scroll handling (`winit`'s `WindowEvent::MouseWheel`) and the same draw/hit-test-offset-consistency discipline.
- [ ] **Step 3: Verify** — native Windows run: scroll through all 24 cards, click cards at top/mid-scroll/bottom, confirm each applies its own correct preset (the exact same high-risk check CT-Task 5's own verification used).
- [ ] **Step 4: Commit** — `feat: port Presets section (scrollable 24-card grid) to tiny-skia`.

### Task 13: Save/Cancel/Reset + settings persistence paths

**Files:** Create `ccum-unix/src/settings_paths.rs`. Modify `ccum-unix/src/panel.rs` (button bar).

**Interfaces:** Consumes `ccum_core::settings::{Settings, load, save}` (check `load`/`save`'s actual current signature — Task 1's report should document whether they take an explicit path or resolve one internally; if internal, they likely need a `ccum-windows`-specific path baked in that must be generalized to accept a path parameter so `ccum-unix` can supply its own OS-appropriate path — this may require a small, additive `ccum-core` change, which is in-scope for this task if needed, just keep it minimal and don't regress `ccum-windows`'s own call sites). Produces: `fn settings_path() -> PathBuf` with `#[cfg(target_os = "macos")]` → `~/Library/Application Support/ClaudeCodeUsageMonitor/settings.json` and `#[cfg(target_os = "linux")]` → XDG (`$XDG_CONFIG_HOME` or `~/.config/`) `/claude-code-usage-monitor/settings.json`, per the design spec §4. Save/Cancel/Reset buttons in the popup panel's button bar, mirroring `ccum-windows`'s Task 14 (original settings-window plan) semantics.

- [ ] **Step 1:** Check `ccum_core::settings::{load, save}`'s actual signature (added in Task 1) and generalize it to accept a path if it doesn't already, updating `ccum-windows`'s call site to pass its existing Windows path explicitly (zero behavior change there — verify with `.\dev.ps1 test` after this change, this is the one place in Phase 3+ that touches `ccum-windows`/`ccum-core` and needs that full regression check).
- [ ] **Step 2:** Implement `settings_path()` for macOS/Linux, wire Save/Cancel/Reset.
- [ ] **Step 3: Verify** — Windows regression: `.\dev.ps1 build`/`test` must still be clean (this task touches shared `ccum-core` code, unlike Tasks 6-12 which were `ccum-unix`-only). `ccum-unix`: native Windows run to confirm Save/Cancel/Reset behave correctly against SOME settings path (even if it's not the "real" macOS/Linux path on this machine, the mechanism itself is verifiable).
- [ ] **Step 4: Commit** — `feat: Save/Cancel/Reset + macOS/Linux settings persistence paths`.

---

## Phase 5 — Polish

### Task 14: Full pass + i18n verification + handoff documentation

**Files:** Possibly none (verification/docs task) unless a genuine bug surfaces.

- [ ] **Step 1:** Full native-Windows run-through of the entire `ccum-unix` app (tray icon, popup, all 6 sections, Save/Cancel/Reset) as the best available proxy for real macOS/Linux behavior.
- [ ] **Step 2:** Confirm `cosmic-text` correctly renders at least one non-Latin locale's strings (e.g. temporarily point the app at Japanese or Russian `Strings` from `ccum_core::localization` and visually confirm glyphs render, not tofu/boxes) — this was the whole reason `cosmic-text` was chosen over `fontdue` in Task 4, so it needs a real check, not just an assumption.
- [ ] **Step 3:** Write a clear, concise "how to actually test this on real macOS/Linux hardware" doc (e.g. `ccum-unix/TESTING.md` or a section in the design spec) covering: how to build (`cargo build -p ccum-unix --release` from a real Mac/Linux checkout), what to check first (does the tray icon appear, does the popup open, do all sections work), and a list of every place in this plan's tasks where the implementer explicitly flagged low confidence (the Antigravity credential-read POSIX guesses from Task 2 chief among them) so the user knows exactly what to scrutinize first.
- [ ] **Step 4: Commit** — `docs: macOS/Linux port Phase 3-5 complete, handoff testing guide` (or a `fix: ...` if Step 1/2 found something).

---

## Self-Review (Phase 3-5 addendum)

**Spec coverage:** design spec §2 (rendering) ✓ Tasks 6-7,10-12; §3 (tray) ✓ Task 8; §4 (persistence paths) ✓ Task 13; §5 (out of scope) correctly not tasked; §6 (testing) ✓ Task 14 + each task's own verification step.

**Placeholders:** none — every task names its exact files/interfaces; research-dependent decisions (the `tiny-skia`-to-window presentation mechanism in Task 6, exact popup positioning in Task 9) are flagged as decisions FOR the implementer to make and document, not vague hand-waves.

**Type consistency:** every later task's "Consumes" line names the exact `ccum_core`/earlier-task type it depends on, traceable back to Task 1's documented public API or an earlier Phase-3+ task's own "Produces" line.

## Self-Review

**Spec coverage:** workspace restructure (T1) ✓; poller portability audit (T2) ✓; regression pass (T3) ✓; ccum-unix skeleton + tech decision (T4) ✓; the design spec's full rendering port, tray integration, and settings-path handling are DELIBERATELY not broken into tasks yet — they depend on Task 4's text-rendering decision and, per the mandatory check-in (T5), on the user's go-ahead to keep writing large amounts of unverifiable code. This is an intentional scope boundary, not an oversight — the remaining design spec sections (§2-6) are known and documented, ready to decompose into Phase 3 tasks once T5's check-in resolves.

**Placeholders:** none in Phase 1 (fully concrete). Task 4's text-rendering crate choice is deliberately left as a research decision for the implementer, not a placeholder — the brief specifies exactly what to compare and document.

**Type consistency:** `ccum_core::settings`/`presets`/`animation`/`poller`'s public surface (Task 1) is what every later `ccum-unix` task will consume — Task 1's implementer must document the EXACT final public API (function/struct names) in their report so Task 4+ can reference it precisely rather than guessing.
