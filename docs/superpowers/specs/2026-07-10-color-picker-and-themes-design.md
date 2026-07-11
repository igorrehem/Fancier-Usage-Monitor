# RGBA Picker Popover + 20 Theme Presets — Design Spec

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> to implement this. This spec builds directly on the settings-window feature shipped on
> `feat/settings-window` (branch continues from commit `aa28009`).

**Goal:** Fix the RGBA picker's cramped, overly-technical layout (4 stacked sliders per
color × 6 colors in Appearance) with a compact popover, and grow the Presets section from
4 built-in style presets to 24 (4 built-in + 20 named after well-known editor/product
color schemes), with a scrollable, grouped card grid.

**Motivation (user feedback, verbatim intent):** "o seletor RGBA tá horrível, implementa
temas e presets" — the picker is too raw/technical, takes too much vertical space, and
has no fast way to pick a color; separately, "temas" turned out to mean more built-in
presets (confirmed via brainstorming), researched from other products' well-known palettes
("pesquisa em outros produtos refs").

## 1. RgbaPicker popover (`src/controls.rs`)

**Current state:** `RgbaPicker` always renders a swatch + hex readout + 4 stacked
R/G/B/A `Slider` rows (`swatch_rect`/`rows_top`/`row_rect`/`slider_rect`, `Control` impl
around line 308). Always ~4 row-heights tall, always shows raw slider math.

**New state:** Closed by default. One row: swatch + hex text + a trailing disclosure
arrow (▾). Clicking anywhere in that row toggles `open`.

When `open`, an additional **popover rect** renders below the row — same architectural
pattern as `Dropdown::list_rect` (a rect that extends outside the control's own bounding
`rect`, already solved in this codebase for exactly this "renders outside my own rect"
problem, including the "outside click closes it" concern flagged in Task 9 and finally
resolved for `Dropdown` during the settings-window final review). Reuse that pattern
directly:

- `RgbaPicker` gains `open: bool` (mirrors `Dropdown.open`) and a `fn close(&mut self)`.
- `fn popover_rect(&self, rect: RECT) -> RECT` computes the expanded area below the
  closed row.
- The popover shows a grid of curated quick-pick swatches (fixed constant palette, ~10
  colors — see §3), plus a trailing **"Custom…"** row. Clicking a swatch sets `value`
  to that color (alpha stays at whatever it currently is — quick swatches are always
  fully-opaque picks, alpha is only adjustable via Custom), fires `ControlEvent::Changed`,
  and closes the popover (matches the rest of this app's "change = immediate live preview
  update" philosophy — no separate Apply step). Clicking "Custom…" instead reveals the
  existing 4 R/G/B/A `Slider` rows inline within the popover (reusing the current
  `row_rect`/`slider_rect` layout code, just relocated into the popover instead of always
  visible), so power users retain full manual control.
- Hit-testing: extend `on_mouse` the same way `Dropdown` does — clicks inside the closed
  row toggle `open`; while open, clicks route to whichever popover sub-region (swatch grid
  cell, "Custom…" row, or a sub-slider once Custom is revealed) contains the point.
- Section-switch auto-close: the settings-window fix already added for `Dropdown` (closing
  it on the single `active_section` mutation site in `src/config_window.rs`) must be
  extended to also close any open `RgbaPicker` popover — generalize that call site to a
  "close any open popover-style control" step rather than a `Dropdown`-specific one, since
  `RgbaPicker` now has the identical failure mode.

**Layout impact on Appearance section:** with all 6 pickers closed by default, the
Appearance section's content height drops from ~6×4 slider-rows to ~6×1 rows — a large
win for the "occupies too much space" complaint. `appearance_grid`/`AppearanceControls`
(`src/config_window.rs`) need their row-height math updated accordingly; when any one
picker is open, the section's effective content height grows by one popover-height for
that row — the surrounding layout (other pickers below it, opacity slider) must shift
down while it's open, same as how `Dropdown`'s open list already displaces nothing today
(it draws over the content below it, since it's the last row) — **note this is a genuine
new interaction case `Dropdown` didn't have to solve** (a `RgbaPicker` popover opening
in the *middle* of a list of 6 pickers, not at the bottom) and needs explicit vertical
reflow of the rows below the opened one. Implementer should design this carefully and
document the chosen approach (e.g. only one popover open at a time, closing any other open
`RgbaPicker` when a different one opens, mirroring `Dropdown`'s existing precedent, so at
most one row of downstream reflow ever needs to be computed).

## 2. Presets: 4 → 24, scrollable grouped grid (`src/presets.rs`, `src/config_window.rs`)

**Schema:** `PresetId` (`src/settings.rs`) grows from 4 to 24 variants. `apply_preset`
(`src/presets.rs`) grows 20 new match arms, each setting `Appearance` (palette
calm/attention/critical, background, text, divider, opacity) + `AnimationSettings`
(fill/shimmer/glow/fade/reduce_motion) — same contract as the existing 4: mutates ONLY
appearance + animation, never geometry/typography/position. Concrete RGBA/animation
values for the 20 are the implementer's judgment, guided by each reference's known
character (see §4 for the list and brief per-theme direction); they do not need to be
byte-exact reproductions of the real product's palette — an evocative, on-brand mapping
into this app's existing calm/attention/critical/background/text/divider slots is the
goal, not pixel-perfect fidelity.

**UI:** the Presets section's card grid becomes the settings window's **first scrollable
region** — no other section has needed scrolling before this. Scope the scroll mechanism
to this section only (don't build a generic scrolling framework for every section):

- Mouse-wheel (`WM_MOUSEWHEEL`) adjusts a new `presets_scroll_offset: i32` in
  `ConfigState` (or wherever per-section transient UI state already lives), clamped so
  the grid can't scroll past its content bounds.
- Painting clips to the Presets content rect (`IntersectClipRect`/`SelectClipRgn`, or
  equivalent) so scrolled-out cards don't bleed into the sidebar/button bar; the fix
  already landed for whole-window flicker (double-buffered `WM_PAINT`, commit `2b6d4f3`)
  means this clip only has to be correct within the pre-rendered off-screen bitmap, not
  fight flicker separately.
- A passive (non-interactive, v1) scrollbar-position indicator — a thin filled rect
  whose height/position reflects the visible fraction and scroll offset — for user
  orientation. Dragging it is explicitly out of scope for this pass; wheel-only is enough
  for 24 items in a 4-column grid (~6 rows).
- Cards are grouped under two header rows: **"Built-in"** (the existing Default, Glass,
  Neon, Minimal — unchanged) and then the 20 new ones grouped by category (e.g. "Code
  editors", "Apps" — implementer's judgment on exact category split per §4's list, keep
  it to 2-4 categories, not one per theme). Category header strings are new, localized
  `Strings` fields (all 10 locales) — the 20 theme *names themselves* are NOT translated
  (kept in English in every locale, as proper nouns — e.g. "Dracula" stays "Dracula" in
  every locale file, same treatment as how font family names are already left
  un-localized elsewhere in this codebase).
- Clicking a card applies the preset exactly like today (`apply_preset` + rebuild
  `state.controls` from the new draft + repaint preview) — no change to that mechanism,
  only to how many cards exist and how they're laid out/scrolled/grouped.

## 3. Quick-swatch palette (RgbaPicker popover content)

Fixed, curated, ~10-color constant palette (not derived from presets, not
locale-dependent) — implementer picks concrete hex values consistent with the existing
app accent (`#D97757` Claude orange) plus a spread of complementary hues and a couple of
neutrals (near-white, near-black/gray), matching the spirit of the mockup shown during
design (warm orange, amber, red, green, blue, purple, near-white, dark gray — 8-10 total
is enough; exact count/values are implementation judgment, document the chosen palette in
the implementer's report so a reviewer can sanity-check it against "curated, not
arbitrary").

## 4. The 20 new presets

Names (English, untranslated across all locales) and each one's intended visual
direction — implementer maps these into concrete `Appearance`/`AnimationSettings` values,
using the existing 4 presets' code as the pattern to follow (see `src/presets.rs`'s
existing `apply_preset` arms and their doc-comment convention of documenting exact chosen
values next to the code):

| # | Name | Direction |
|---|------|-----------|
| 1 | Dracula | Purple/pink accents over a dark blue-ish background |
| 2 | Nord | Cool, muted Nordic blue-gray, sober |
| 3 | Solarized Dark | Classic orange/cyan over dark teal |
| 4 | Solarized Light | Same accent family, light background |
| 5 | Gruvbox | Warm retro earth tones (burnt orange/olive) |
| 6 | Catppuccin | Soft pastels (lavender/pink/mint) |
| 7 | Tokyo Night | Blue-purple neon over near-black |
| 8 | One Dark | Classic Atom/VS Code blue/green/purple |
| 9 | Monokai | Vibrant lime/pink over dark gray |
| 10 | Material | Material Design blue/amber |
| 11 | GitHub Dark | Understated blue/green, very neutral |
| 12 | Discord | Blurple over slate gray |
| 13 | Spotify | Vibrant green over pure black |
| 14 | Rosé Pine | Rosé/gold over muted purple-gray |
| 15 | Everforest | Soft forest green, low contrast, calm |
| 16 | Kanagawa | Japanese palette (indigo/earth red) |
| 17 | Synthwave '84 | Retrowave pink/purple neon, strong glow |
| 18 | Ayu | Warm orange over navy blue |
| 19 | Palenight | Sophisticated gray-purple |
| 20 | Cyberpunk | Neon yellow/cyan over black, strong glow |

Suggested category grouping for the section headers (implementer may adjust): **Code
editors** (Dracula, Nord, Solarized Dark, Solarized Light, Gruvbox, Catppuccin, Tokyo
Night, One Dark, Monokai, GitHub Dark, Rosé Pine, Everforest, Kanagawa, Ayu, Palenight —
i.e. most of them), **Apps** (Discord, Spotify), **Neon/retro** (Synthwave '84,
Cyberpunk), **Material** (Material). Collapse to fewer, larger groups if 4 categories of
very uneven size looks awkward in the UI — implementer's call.

## 5. i18n

New `Strings` fields needed (all 10 locales): the "Custom…" label in the RgbaPicker
popover, and 2-4 category header labels for the grouped preset grid (exact count depends
on §4's final category split). The 20 theme names are NOT new `Strings` fields with
per-locale translations — they're a single constant list of English names shared across
all locales (consistent with the "proper noun, not translated" decision).

## 6. Testing

- `presets.rs`: one test per new preset (20 total, or table-driven equivalent) asserting
  each mutates only `appearance`/`animation` and leaves `geometry`/`typography` untouched
  — same shape as the existing 4 presets' tests (`apply_preset_never_touches_geometry`
  parameterized test already exists and should extend naturally to cover all 24).
- `controls.rs`: `RgbaPicker`'s new popover open/close/hit-test logic — pure math, same
  testing style as `Slider::pos_to_value`/`Dropdown`'s existing tests (construct a
  `RgbaPicker`, a `RECT`, and assert hit-test/open-state transitions without an `HDC`).
- Manual: open the real settings window, verify Appearance's 6 pickers default closed and
  visibly shrink the section, open one and confirm the swatch grid + Custom flow both
  work and update the live preview; open Presets, scroll through all 24, confirm each
  visibly applies a distinct look to the preview.

## 7. Explicitly out of scope

- Draggable scrollbar thumb (wheel-only for this pass).
- Per-model theme overrides (already deferred at the whole-feature level).
- Any change to the *existing* 4 presets' values/behavior — purely additive.
- Recent-colors / custom-swatch persistence in the RgbaPicker (only the fixed curated
  palette + manual Custom sliders — no "remember my last 5 custom colors").
