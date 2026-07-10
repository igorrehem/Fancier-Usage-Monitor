# RGBA Picker Popover + 20 Theme Presets — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the RGBA picker's always-visible 4-slider layout with a compact popover (swatch grid + hex + optional manual sliders), and grow the Presets section from 4 built-in style presets to 24 (4 built-in + 20 named after well-known editor/product color schemes), with a scrollable, grouped card grid — the settings window's first scrolling section.

**Architecture:** `RgbaPicker` (`src/controls.rs`) gains an `open`/`close()`/popover-rect mechanism identical in shape to the existing `Dropdown` control's "renders outside my own rect" pattern. `PresetId`/`apply_preset` (`src/settings.rs`/`src/presets.rs`) grow from 4 to 24 variants using the exact same per-preset match-arm contract already established. The Presets section (`src/config_window.rs`) gains its first scroll mechanism, scoped to that section only.

**Tech Stack:** Rust, `windows` crate (raw Win32/GDI), zero new dependencies — same as the rest of this codebase. Build via `.\dev.ps1`.

**Full design spec:** `docs/superpowers/specs/2026-07-10-color-picker-and-themes-design.md` — read this first for the "why" behind every task below; this plan only restates the "what."

## Global Constraints

- Zero new crate dependencies.
- No GPU — all rendering is GDI, matching the existing settings window.
- Every `apply_preset` arm mutates ONLY `Appearance` + `AnimationSettings` fields — never `Geometry`, `Typography`, or position/identity settings (`tray_offset`, `taskbar_index`, `poll_interval_ms`, etc.).
- New user-facing strings (the "Custom…" label, preset category headers) go into `Strings` and must be filled in all 10 locale files (`english.rs`, `dutch.rs`, `french.rs`, `german.rs`, `japanese.rs`, `korean.rs`, `portuguese_brazil.rs`, `russian.rs`, `spanish.rs`, `traditional_chinese.rs`) — a missing field is a compile error (exhaustive struct), which is the enforcement mechanism.
- The 20 new preset *names* (Dracula, Nord, …) are proper nouns and are NOT translated — same literal English string in every locale, same treatment already given to font family names elsewhere in this codebase.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_015nrrhG6NU1CQ9UGn5nvwJs
  ```
- Build check after each task: `.\dev.ps1 build` (clean, no new warnings). Test check: `.\dev.ps1 test` (0 failures). Widget/settings-window smoke-check via `.\dev.ps1 run` where a task calls for manual verification — use the real "Settings…" menu/tray entry point (Task 15 of the original settings-window plan made this permanent, no temporary hooks needed).

---

## File Structure

- **Modify `src/controls.rs`** — `RgbaPicker` gains `open: bool`, `close()`, popover layout/hit-testing, curated quick-swatch palette constant.
- **Modify `src/config_window.rs`** — Appearance section: collapsed-row layout + popover vertical reflow + "only one popover open at a time" + generalized auto-close-on-section-switch. Presets section: scroll state, mouse-wheel handling, clip region, grouped/scrollable card grid for 24 items.
- **Modify `src/settings.rs`** — `PresetId` enum grows from 4 to 24 variants.
- **Modify `src/presets.rs`** — `apply_preset` grows 20 new match arms + tests. New `THEME_CATEGORY` grouping data (name → category) for the Presets section's group headers.
- **Modify `src/localization/mod.rs`** + all 10 locale files — new `Strings` fields: `rgba_custom` (the "Custom…" popover row label) and category header labels (exact count depends on Task 5's category split, expected 3-4: e.g. `preset_category_editors`, `preset_category_apps`, `preset_category_neon`).

---

## Phase A — RgbaPicker popover

### Task 1: `RgbaPicker` popover (controls.rs, no config_window.rs wiring yet)

**Files:**
- Modify: `src/controls.rs`
- Test: inline `#[cfg(test)]` in `src/controls.rs`

**Interfaces:**
- Consumes: `Rgba`, `Color`, existing `Slider`, `Control` trait, `ControlEvent` (all already in `src/controls.rs`).
- Produces: `RgbaPicker` gains `open: bool` (new field, default `false`), `pub fn close(&mut self)` (sets `open = false`), `fn popover_rect(&self, rect: RECT) -> RECT` (the expanded area below the closed row — mirror `Dropdown::list_rect`'s existing shape: same-width as `rect`, positioned directly below it, tall enough for the swatch grid + the "Custom…" row). Const `QUICK_SWATCHES: [Rgba; N]` (N ≈ 10) — a fixed curated palette: the existing brand accent `#D97757`, a spread of complementary hues (amber, red, green, blue, purple), and 2-3 neutrals (near-white, mid-gray, near-black), each `Rgba { r, g, b, a: 255 }` (quick swatches are always fully opaque).

- [ ] **Step 1: Write failing tests for the pure layout/hit-test math**

```rust
#[cfg(test)]
mod popover_tests {
    use super::*;

    #[test]
    fn closed_row_is_one_row_tall() {
        let picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        // The closed control's own draw rect is exactly what's passed in — no
        // extra height is consumed until `open` is true.
        assert_eq!(picker.open, false);
        let popover = picker.popover_rect(rect);
        assert!(popover.top >= rect.bottom, "popover must render below the closed row");
    }

    #[test]
    fn click_on_closed_row_opens_popover() {
        let mut picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        let _ = picker.on_mouse(WM_LBUTTONDOWN, 10, 10, rect);
        assert!(picker.open, "clicking the closed row must open the popover");
    }

    #[test]
    fn click_outside_when_open_closes_it() {
        let mut picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        picker.open = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        // A click far outside both the row and the popover.
        let _ = picker.on_mouse(WM_LBUTTONDOWN, 500, 500, rect);
        assert!(!picker.open, "an outside click must close the popover");
    }

    #[test]
    fn close_forces_open_false() {
        let mut picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        picker.open = true;
        picker.close();
        assert!(!picker.open);
    }

    #[test]
    fn clicking_a_quick_swatch_sets_value_and_closes() {
        let mut picker = RgbaPicker::new(Rgba { r: 0, g: 0, b: 0, a: 255 });
        picker.open = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        let popover = picker.popover_rect(rect);
        // Click the first quick-swatch cell (top-left of the grid).
        let event = picker.on_mouse(WM_LBUTTONDOWN, popover.left + 5, popover.top + 5, rect);
        assert!(event.is_some(), "clicking a swatch must fire a Changed event");
        assert_eq!(picker.value, QUICK_SWATCHES[0]);
        assert!(!picker.open, "picking a swatch closes the popover");
    }

    #[test]
    fn clicking_custom_reveals_sliders_without_closing() {
        let mut picker = RgbaPicker::new(Rgba { r: 0, g: 0, b: 0, a: 255 });
        picker.open = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        let custom_row = picker.custom_row_rect(rect);
        let _ = picker.on_mouse(WM_LBUTTONDOWN, custom_row.left + 5, custom_row.top + 5, rect);
        assert!(picker.open, "Custom stays open to show the manual sliders");
        assert!(picker.custom_expanded, "Custom must flip on the manual-slider view");
    }
}
```

- [ ] **Step 2: Run tests, verify they fail** — `.\dev.ps1 test controls` → FAIL (fields/methods don't exist yet).
- [ ] **Step 3: Implement.**
  - Add `open: bool` (default `false` in `RgbaPicker::new`) and `custom_expanded: bool` (default `false`) fields.
  - `fn popover_rect(&self, rect: RECT) -> RECT`: width = `rect`'s width, `top = rect.bottom`, height covers a swatch grid (lay out `QUICK_SWATCHES` in a fixed-column grid, e.g. 5 columns × 2 rows for 10 swatches) plus one "Custom…" row below the grid, plus — when `custom_expanded` — the existing 4 slider rows (reuse `row_rect`/`slider_rect`, now positioned inside the popover instead of always-visible below the swatch/hex header).
  - `fn custom_row_rect(&self, rect: RECT) -> RECT`: the "Custom…" row's own rect within the popover (below the swatch grid).
  - `pub fn close(&mut self)`: `self.open = false; self.custom_expanded = false;`.
  - Rework `draw()`: closed state draws only the swatch + hex + a trailing "▾" glyph in the same row (no slider rows). Open state additionally draws the popover: swatch grid (each `QUICK_SWATCHES[i]` as a small filled rect, with the currently-selected one visually indicated — e.g. an outline — if `self.value` exactly matches that swatch), the "Custom…" row, and — if `custom_expanded` — the 4 R/G/B/A slider rows (existing code, relocated).
  - Rework `on_mouse()`: `WM_LBUTTONDOWN` when `!open` and the click lands in the closed row → `open = true`, return `None` (opening isn't itself a value change). When `open`: click in the closed row again → toggle closed (`close()`); click in a swatch grid cell → `self.value = QUICK_SWATCHES[i]` (preserving existing alpha is NOT applicable here since quick swatches are fully opaque per the design spec — set `value` to the swatch's own `a: 255` directly), sync the 4 sliders' internal state to match (so if the user later opens Custom, the sliders reflect the swatch they picked), `close()`, return `Some(ControlEvent::Changed)`; click in the "Custom…" row → `custom_expanded = true`, return `None`; click in a revealed slider row (only reachable when `custom_expanded`) → delegate to the existing per-channel slider dispatch, same as today, still fires `Changed` on drag. A click anywhere else while `open` (i.e. outside the row AND outside the popover) → `close()`, return `None`.
  - `WM_MOUSEMOVE`/`WM_LBUTTONUP` while `custom_expanded` and a slider is mid-drag: unchanged from today's slider-drag handling, just gated on `custom_expanded`.
- [ ] **Step 4: Run tests + build** — `.\dev.ps1 test controls` PASS; `.\dev.ps1 build` OK.
- [ ] **Step 5: Commit** — `feat: compact popover for RgbaPicker (quick swatches + custom sliders)`.

### Task 2: Wire the popover into the Appearance section (config_window.rs)

**Files:**
- Modify: `src/config_window.rs`

**Interfaces:**
- Consumes: `RgbaPicker::open`, `close()`, `popover_rect()` from Task 1; existing `AppearanceControls`/`appearance_grid`/`dispatch_appearance`/`draw_appearance_controls` (from the original settings-window plan's Task 13).
- Produces: Appearance section rows now reflow vertically around whichever `RgbaPicker` (if any) has its popover open; at most one `RgbaPicker` popover open at a time across all 6 (Calm/Attention/Critical/Background/Text/Divider); switching sections or opening a different picker closes any currently-open one.

- [ ] **Step 1:** Read the existing `appearance_grid`/`AppearanceControls`/`draw_appearance_controls`/`dispatch_appearance` in `src/config_window.rs` (from the original settings-window feature) to find the exact current row-layout math for the 6 `RgbaPicker`s + the opacity `Slider` below them.
- [ ] **Step 2:** Change the row-height calculation so each closed `RgbaPicker` occupies one compact row (matching Task 1's closed-row height) instead of the previous 4-slider-row height. When a picker's `open == true`, its row's effective height for layout purposes becomes `closed row height + popover_rect height`, and every row below it (remaining pickers, the opacity slider) shifts down by that popover height. Implement this as a small helper (e.g. `fn appearance_row_top(index, open_index, dpi) -> i32`) that all of `draw_appearance_controls`/`dispatch_appearance` call, so draw and hit-test can't drift apart from each other (same discipline already used for every other section triplet in this file).
- [ ] **Step 3:** Enforce single-open-at-a-time: when a `RgbaPicker`'s `on_mouse` returns an "opened" transition (you may need `on_mouse` or a wrapping call site to signal this — either inspect `open` before/after the call, or check the return value's semantics from Task 1), close every other `RgbaPicker` in `AppearanceControls` before repainting.
- [ ] **Step 4:** Generalize the existing "close any open `Dropdown` on section switch" fix (added during the settings-window feature's final review, at the single `active_section` mutation site) to also call `close()` on any `RgbaPicker` whose `open` is `true` — i.e. section switch now closes both control kinds, not just `Dropdown`.
- [ ] **Step 5: Manual** — open the real settings window (via the permanent "Settings…" menu entry), go to Appearance, confirm all 6 pickers render as compact closed rows by default (section is visibly much shorter than before), open one, confirm the swatch grid appears and rows below it shift down, click a swatch and confirm the live preview updates immediately and the popover closes, open "Custom…" and confirm the 4 sliders still work exactly as before, open a second picker while the first is still open and confirm the first one closes, switch sections while one is open and confirm it's closed when you return to Appearance.
- [ ] **Step 6: Commit** — `feat: wire RgbaPicker popover into the Appearance section`.

---

## Phase B — 20 new presets

### Task 3: `PresetId` grows to 24 + 20 new `apply_preset` arms (data only, no UI)

**Files:**
- Modify: `src/settings.rs` (`PresetId` enum)
- Modify: `src/presets.rs`
- Test: inline `#[cfg(test)]` in `src/presets.rs`

**Interfaces:**
- Consumes: existing `Settings`/`Appearance`/`AnimationSettings`/`Rgba` (`src/settings.rs`).
- Produces: `PresetId` gains 20 new variants (exact identifiers, `PascalCase` matching the existing `Default`/`Glass`/`Neon`/`Minimal` style): `Dracula`, `Nord`, `SolarizedDark`, `SolarizedLight`, `Gruvbox`, `Catppuccin`, `TokyoNight`, `OneDark`, `Monokai`, `Material`, `GitHubDark`, `Discord`, `Spotify`, `RosePine`, `Everforest`, `Kanagawa`, `SynthwaveEighty4` (Rust identifiers can't start with a digit or contain `'`, so `Synthwave '84` becomes the identifier `SynthwaveEighty4` with its *display* name — added in Task 4 — remaining the literal string `"Synthwave '84"`), `Ayu`, `Palenight`, `Cyberpunk`. `apply_preset` gains one match arm per new variant, each setting concrete `Appearance` + `AnimationSettings` values per the design spec's §4 direction for that theme (pick real, reasoned hex values — do not copy another arm's values verbatim for two different themes).

- [ ] **Step 1: Write failing tests** — extend the existing parameterized "every preset leaves geometry/typography untouched" test (from the original settings-window Task 16) to iterate all 24 `PresetId` variants instead of 4, plus one new specific-value test per new preset spot-checking its defining characteristic, e.g.:

```rust
#[test]
fn dracula_uses_purple_accent_over_dark_background() {
    let mut s = Settings::default();
    apply_preset(PresetId::Dracula, &mut s);
    let bg = s.appearance.background.expect("Dracula sets an explicit background");
    assert!(bg.b > bg.r, "Dracula's background should lean blue/purple, not warm");
}

#[test]
fn spotify_uses_pure_black_background_and_vivid_green() {
    let mut s = Settings::default();
    apply_preset(PresetId::Spotify, &mut s);
    let bg = s.appearance.background.expect("Spotify sets an explicit background");
    assert_eq!((bg.r, bg.g, bg.b), (0, 0, 0), "Spotify's background is pure black");
}

#[test]
fn cyberpunk_and_synthwave_enable_strong_alert_glow() {
    let mut s1 = Settings::default();
    apply_preset(PresetId::Cyberpunk, &mut s1);
    assert!(s1.animation.alert_glow.on && s1.animation.alert_glow.intensity > 0.7);

    let mut s2 = Settings::default();
    apply_preset(PresetId::SynthwaveEighty4, &mut s2);
    assert!(s2.animation.alert_glow.on && s2.animation.alert_glow.intensity > 0.7);
}

#[test]
fn everforest_and_rose_pine_are_calm_low_motion() {
    let mut s1 = Settings::default();
    apply_preset(PresetId::Everforest, &mut s1);
    assert!(s1.animation.shimmer.speed < d_shimmer_speed_default_for_test());

    let mut s2 = Settings::default();
    apply_preset(PresetId::RosePine, &mut s2);
    assert!(s2.animation.shimmer.speed < d_shimmer_speed_default_for_test());
}

#[test]
fn all_24_presets_never_touch_geometry_or_typography() {
    let all = [
        PresetId::Default, PresetId::Glass, PresetId::Neon, PresetId::Minimal,
        PresetId::Dracula, PresetId::Nord, PresetId::SolarizedDark, PresetId::SolarizedLight,
        PresetId::Gruvbox, PresetId::Catppuccin, PresetId::TokyoNight, PresetId::OneDark,
        PresetId::Monokai, PresetId::Material, PresetId::GitHubDark, PresetId::Discord,
        PresetId::Spotify, PresetId::RosePine, PresetId::Everforest, PresetId::Kanagawa,
        PresetId::SynthwaveEighty4, PresetId::Ayu, PresetId::Palenight, PresetId::Cyberpunk,
    ];
    for id in all {
        let mut s = Settings::default();
        let before_geo = s.geometry;
        let before_type = s.typography.clone();
        apply_preset(id, &mut s);
        assert_eq!(s.geometry, before_geo, "{id:?} must not touch geometry");
        assert_eq!(s.typography.family, before_type.family, "{id:?} must not touch typography");
    }
}
```

  (Note: `d_shimmer_speed_default_for_test()` is a placeholder name for whatever the existing default-shimmer-speed constant/helper already is in `src/settings.rs` — use the real one, e.g. `d_shimmer_speed()`, when writing this for real; don't introduce a new helper just for the test.)

- [ ] **Step 2: Run tests, verify they fail** — `.\dev.ps1 test presets` → FAIL (variants/arms don't exist).
- [ ] **Step 3: Implement.** Add the 20 variants to `PresetId` (`src/settings.rs`). Add 20 match arms to `apply_preset` (`src/presets.rs`), each with a short doc comment stating the chosen hex values and the design-spec direction it maps to (matching the existing 4 presets' documentation convention). Concrete values are the implementer's judgment per theme (see design spec §4 for direction per theme) — e.g. Dracula: background near `#282A36`, accent/calm near `#BD93F9` (purple) or `#FF79C6` (pink), text near `#F8F8F2`; Spotify: background `#000000` exactly, accent `#1DB954` (Spotify green); Solarized Dark: background `#002B36`, accent `#B58900`/`#268BD2`; and so on for all 20 — pick real, distinct, on-theme values for every one, don't leave any two presets with near-identical palettes.
- [ ] **Step 4: Run tests + build** — `.\dev.ps1 test presets` PASS (all 24-preset tests green); `.\dev.ps1 build` OK.
- [ ] **Step 5: Commit** — `feat: 20 new theme presets (Dracula, Nord, Solarized, ...)`.

### Task 4: i18n for the new UI strings + theme category data (NOT the theme names)

**Files:**
- Modify: `src/localization/mod.rs` (`Strings` struct)
- Modify: all 10 locale files (`english.rs`, `dutch.rs`, `french.rs`, `german.rs`, `japanese.rs`, `korean.rs`, `portuguese_brazil.rs`, `russian.rs`, `spanish.rs`, `traditional_chinese.rs`)
- Modify: `src/presets.rs` (theme display-name + category constant, English-only, not localized)

**Interfaces:**
- Produces: new `Strings` fields — `rgba_custom: &'static str` (the "Custom…" popover row label) and category header labels. Use exactly 3 categories per the design spec's suggested grouping (collapse "Material" into "Apps" rather than giving it its own single-item category): `preset_category_builtin: &'static str` ("Built-in"), `preset_category_editors: &'static str` ("Code editors"), `preset_category_apps: &'static str` ("Apps") — `preset_category_neon` folds into "Code editors" too (Synthwave/Cyberpunk are still developer-culture references) to keep exactly 3 groups: Built-in (4), Code editors (17: everything except Discord/Spotify), Apps (2: Discord, Spotify).
- Produces: `pub const THEME_DISPLAY_NAME: fn(PresetId) -> &'static str` (or a `match` returning the literal English name for every `PresetId`, e.g. `PresetId::Dracula => "Dracula"`, `PresetId::SynthwaveEighty4 => "Synthwave '84"`) and `pub const THEME_CATEGORY: fn(PresetId) -> PresetCategory` (a new small enum `PresetCategory { Builtin, Editors, Apps }`) in `src/presets.rs` — both pure, total functions over all 24 variants, used by Task 5's UI code. These are NOT part of `Strings` (not localized) since theme names are proper nouns per the design spec.

- [ ] **Step 1:** Add `rgba_custom`, `preset_category_builtin`, `preset_category_editors`, `preset_category_apps` to `Strings` (`src/localization/mod.rs`), placed near the existing `open_settings`/settings-window fields for locality.
- [ ] **Step 2:** Fill English (`english.rs`) and Portuguese Brazilian (`portuguese_brazil.rs`) first, build to confirm the struct compiles, per the existing i18n task convention in this codebase.
- [ ] **Step 3:** Fill the remaining 8 locales with real, distinct translations (not English copy-pasted — this gets checked).
- [ ] **Step 4:** In `src/presets.rs`, add `pub enum PresetCategory { Builtin, Editors, Apps }` and the two total-match functions `THEME_DISPLAY_NAME`/`THEME_CATEGORY` covering all 24 `PresetId` variants (English literal names, e.g. `"Dracula"`, `"Nord"`, `"Solarized Dark"`, `"Solarized Light"`, `"Gruvbox"`, `"Catppuccin"`, `"Tokyo Night"`, `"One Dark"`, `"Monokai"`, `"Material"`, `"GitHub Dark"`, `"Discord"`, `"Spotify"`, `"Rosé Pine"`, `"Everforest"`, `"Kanagawa"`, `"Synthwave '84"`, `"Ayu"`, `"Palenight"`, `"Cyberpunk"`, plus the existing `"Default"`/`"Glass"`/`"Neon"`/`"Minimal"`).
- [ ] **Step 5: Build** — `.\dev.ps1 build` OK (a missing locale field or an unhandled `PresetId`/`PresetCategory` match arm is a compile error — this is the completeness check).
- [ ] **Step 6: Commit** — `feat: localize new settings strings, add theme name/category data`.

### Task 5: Presets section UI — scrollable, grouped card grid for 24 items

**Files:**
- Modify: `src/config_window.rs`

**Interfaces:**
- Consumes: `THEME_DISPLAY_NAME`/`THEME_CATEGORY`/`PresetCategory` (Task 4), all 24 `PresetId` variants (Task 3), `strings.preset_category_*` (Task 4).
- Produces: the Presets section renders all 24 as cards in a scrollable grid grouped under 3 headers (Built-in, Code editors, Apps, in that order), mouse-wheel scrolls, content clips to the section's content rect, a passive scrollbar-position indicator shows on the right edge when content overflows.

- [ ] **Step 1:** Read the existing Presets section code (`presets_layout`/`draw_presets_controls`/`dispatch_presets`/the intro-text fix from the settings-window final review) in `src/config_window.rs` to find the current 2×2 card grid's layout math.
- [ ] **Step 2:** Add `presets_scroll_offset: i32` to wherever per-section transient UI state already lives (e.g. `ConfigState` or `SectionControls`, matching the existing convention for section-local state). Add a helper computing the full (unclipped) content height of all 24 cards + 3 group headers at the section's current width, so the max scroll offset can be clamped (`max_offset = (full_content_height - visible_height).max(0)`).
- [ ] **Step 3:** Handle `WM_MOUSEWHEEL` in `config_wnd_proc`: when the active section is Presets and the cursor is within the Presets content rect, adjust `presets_scroll_offset` by a fixed step per wheel notch (e.g. one card-row's height), clamped to `[0, max_offset]`, then repaint.
- [ ] **Step 4:** Update `draw_presets_controls` to: (a) offset every card/header's drawn `top`/`bottom` by `-presets_scroll_offset`, (b) clip drawing to the Presets content rect (so scrolled-out content doesn't bleed into the sidebar/button bar — use the same off-screen-bitmap-then-blit machinery already in place from the flicker fix, or an explicit `IntersectClipRect` if drawing directly, whichever fits this file's existing structure most naturally), (c) draw the 3 group headers (Built-in / Code editors / Apps) as small text labels above their respective card rows, using `strings.preset_category_*`, (d) draw a thin passive scrollbar indicator on the right edge of the content rect when `max_offset > 0` (a filled rect whose height ≈ `visible_height / full_content_height * track_height` and whose top ≈ `presets_scroll_offset / max_offset * (track_height - indicator_height)`).
- [ ] **Step 5:** Update `dispatch_presets`'s hit-testing to account for the scroll offset (subtract `presets_scroll_offset` from the click's `y` before comparing against each card's unscrolled layout rect, or equivalently add it to the card rects before comparing — whichever direction matches Step 4's drawing offset) and to use `THEME_DISPLAY_NAME`/`PresetId`'s full 24-variant list instead of the old 4.
- [ ] **Step 6: Manual** — open the real settings window, go to Presets, confirm all 24 cards render grouped under 3 headers, scroll with the mouse wheel through the full list, confirm the passive scrollbar indicator moves, click cards at the top, middle, and bottom of the scrolled list and confirm each correctly applies its distinct preset to the live preview (not an off-by-scroll-offset wrong card).
- [ ] **Step 7: Commit** — `feat: scrollable grouped preset grid for 24 style presets`.

---

## Phase C — Final verification

### Task 6: Full run-through + polish

**Files:** none (verification only, unless a genuine bug surfaces — see below)

- [ ] **Step 1:** `.\dev.ps1 test` — full suite, 0 failures.
- [ ] **Step 2:** `.\dev.ps1 build` — clean, no warnings.
- [ ] **Step 3: Manual full run-through** via the real settings window: for each of the 6 Appearance color fields, open the popover, pick a quick swatch, confirm live preview updates; open Custom, drag a slider, confirm live preview updates; confirm only one popover is ever open at once; switch sections with a popover open and confirm it closes. For Presets: scroll through and click at least one card from each of the 3 groups, confirm each visually changes the preview distinctly. Save once with a mix of a custom Appearance color + a clicked preset, confirm the real widget updates and `settings.json` persists both correctly on restart.
- [ ] **Step 4:** If this run-through surfaces a genuine bug, fix it as part of this task (small, targeted fix) — document what was found and fixed in the commit message/report rather than silently patching.
- [ ] **Step 5: Commit** — `docs: final verification for RGBA picker popover and theme presets` (or fold into the fix commit's message if Step 4 found something to fix).

---

## Self-Review

**Spec coverage:** RgbaPicker popover (T1-2) ✓; 20 new presets with distinct, reasoned values (T3) ✓; i18n for new strings, theme names left untranslated (T4) ✓; scrollable grouped Presets grid (T5) ✓; only-one-popover-open + section-switch auto-close generalization (T2) ✓; final verification (T6) ✓.

**Placeholders:** none — every preset's direction is specified in the design spec §4 and restated per-task; the one internal test-helper name placeholder (`d_shimmer_speed_default_for_test`) is explicitly flagged inline as "use the real existing helper," not a spec gap.

**Type consistency:** `PresetId` (T3) variants are the exact identifiers `THEME_DISPLAY_NAME`/`THEME_CATEGORY` (T4) and the Presets UI (T5) consume — same 24-name list repeated verbatim across T3/T4's test code and T4's display-name match, so a reviewer can diff them directly.
