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

---

## Self-Review

**Spec coverage:** workspace restructure (T1) ✓; poller portability audit (T2) ✓; regression pass (T3) ✓; ccum-unix skeleton + tech decision (T4) ✓; the design spec's full rendering port, tray integration, and settings-path handling are DELIBERATELY not broken into tasks yet — they depend on Task 4's text-rendering decision and, per the mandatory check-in (T5), on the user's go-ahead to keep writing large amounts of unverifiable code. This is an intentional scope boundary, not an oversight — the remaining design spec sections (§2-6) are known and documented, ready to decompose into Phase 3 tasks once T5's check-in resolves.

**Placeholders:** none in Phase 1 (fully concrete). Task 4's text-rendering crate choice is deliberately left as a research decision for the implementer, not a placeholder — the brief specifies exactly what to compare and document.

**Type consistency:** `ccum_core::settings`/`presets`/`animation`/`poller`'s public surface (Task 1) is what every later `ccum-unix` task will consume — Task 1's implementer must document the EXACT final public API (function/struct names) in their report so Task 4+ can reference it precisely rather than guessing.
