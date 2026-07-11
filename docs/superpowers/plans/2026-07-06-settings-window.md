# Settings Window, Appearance & Animations — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a bespoke GDI settings window with live WYSIWYG preview that customizes the taskbar widget's colors (RGBA), font, geometry, four selectable animation families, update frequency, and style presets.

**Architecture:** Extract settings into `settings.rs` with a versioned, defaulted schema (indolent migration). The layered-window renderer (`render_layered`/`paint_content`) reads colors/font/geometry from the in-memory `Settings` plus an `AnimationFrame` produced each tick by `animation.rs`. A new `config_window.rs` (built from reusable GDI `controls.rs`) edits a *draft* copy of settings and previews it by calling the same `paint_content`; Save commits the draft and re-renders the real widget.

**Tech Stack:** Rust, `windows` crate (raw Win32/GDI), `serde`/`serde_json`. Zero new dependencies. Build via `.\dev.ps1`.

## Global Constraints

- Zero new crate dependencies (keep binary ~0.8 MB, "lightweight native").
- All rendering is GDI into a 32bpp top-down DIB composed with `UpdateLayeredWindow`. No GPU.
- Every new `settings.json` field is `#[serde(default)]` so existing files migrate without loss.
- Behavior-preserving foundation: default settings must reproduce today's exact look.
- Animations run on a timer only while animation is pending; idle → timer stops (0% CPU).
- New user-facing strings added to all 10 locales via the existing `strings()` mechanism.
- Build check after each task: `.\dev.ps1 build`. Widget smoke-check via `.\dev.ps1 run` where noted.
- Commit messages end with the repo's Co-Authored-By / Claude-Session trailers.

---

## File Structure

- **Create `src/settings.rs`** — `Settings` struct (existing fields + `appearance`/`typography`/`geometry`/`animation`), `Rgba`, enums (`Weight`, `Easing`, `PresetId`), `load()`/`save()`, `migrate()`. Owns `settings.json`.
- **Create `src/animation.rs`** — easing fns, `AnimationClock`, per-family state, `tick(now) -> AnimationFrame`. No GDI.
- **Create `src/controls.rs`** — reusable GDI controls: `Slider`, `RgbaPicker`, `Dropdown`, `Segmented`, `Toggle`. Each: `draw(hdc, rect)`, `on_mouse(msg, x, y) -> Option<ControlEvent>`.
- **Create `src/config_window.rs`** — the settings window: class registration, creation, section nav, layout, preview panel, Save/Cancel/Reset. Consumes `controls`, `settings`, `animation`, `paint_content`.
- **Create `src/presets.rs`** — the 4 built-in presets as `Settings` fragments + `apply(preset, &mut Settings)`.
- **Modify `src/theme.rs`** — `Color` gains alpha + `#RRGGBBAA` parsing/formatting.
- **Modify `src/window.rs`** — read `Settings`+`AnimationFrame` in render; animation timer; menu item "Settings…"; geometry clamp.
- **Modify `src/main.rs`** — `mod settings; mod animation; mod controls; mod config_window; mod presets;`
- **Modify `src/localization/*.rs`** — new UI strings in all 10 locales.

---

## Phase 0 — Foundation (settings + color alpha)

### Task 1: Alpha in `Color` + RGBA/hex conversions

**Files:**
- Modify: `src/theme.rs`
- Test: inline `#[cfg(test)]` in `src/theme.rs`

**Interfaces:**
- Produces: `Color { r: u8, g: u8, b: u8, a: u8 }`, `Color::from_hex(&str) -> Color` (accepts `#RGB`,`#RRGGBB`,`#RRGGBBAA`), `Color::to_hex(&self) -> String` (`#RRGGBBAA`), `Color::to_colorref(&self) -> u32` (0x00BBGGRR, ignores alpha), `Color::with_alpha(u8) -> Color`, `Color::lerp(&self, other: &Color, t: f32) -> Color`.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_rrggbbaa() {
        let c = Color::from_hex("#1C2A3Bff");
        assert_eq!((c.r, c.g, c.b, c.a), (0x1C, 0x2A, 0x3B, 0xff));
    }
    #[test]
    fn parses_rrggbb_defaults_opaque() {
        let c = Color::from_hex("#1C2A3B");
        assert_eq!(c.a, 0xff);
    }
    #[test]
    fn to_colorref_is_bgr_without_alpha() {
        let c = Color { r: 0x12, g: 0x34, b: 0x56, a: 0x80 };
        assert_eq!(c.to_colorref(), 0x00_56_34_12);
    }
    #[test]
    fn roundtrips_hex() {
        assert_eq!(Color::from_hex("#aabbccdd").to_hex(), "#AABBCCDD");
    }
    #[test]
    fn lerp_midpoint() {
        let a = Color { r: 0, g: 0, b: 0, a: 0 };
        let b = Color { r: 100, g: 200, b: 50, a: 255 };
        let m = a.lerp(&b, 0.5);
        assert_eq!((m.r, m.g, m.b, m.a), (50, 100, 25, 127));
    }
}
```

- [ ] **Step 2: Run tests, verify they fail** — `.\dev.ps1 test theme` → FAIL (alpha field / methods missing).
- [ ] **Step 3: Implement.** Add `a: u8` to `Color`; update `from_hex` to parse 3/6/8 hex digits (default `a=255`); add `to_hex`, `with_alpha`, `lerp` (per-channel `a + (b-a)*t`, round). Keep `to_colorref` ignoring alpha. Update all existing `Color { .. }` literals in the codebase to include `a: 0xff` (grep `Color {`); `from_hex` callers unaffected.
- [ ] **Step 4: Run tests + build** — `.\dev.ps1 test theme` PASS; `.\dev.ps1 build` OK.
- [ ] **Step 5: Commit** — `feat: add alpha channel and RGBA/hex helpers to Color`.

### Task 2: `settings.rs` — schema, defaults, migration

**Files:**
- Create: `src/settings.rs`
- Modify: `src/window.rs` (remove the old `SettingsFile`/`load_settings`/`save_settings`; re-export or call `settings::*`)
- Modify: `src/main.rs` (`mod settings;`)
- Test: inline `#[cfg(test)]` in `src/settings.rs`

**Interfaces:**
- Produces:
  - `struct Rgba { r: u8, g: u8, b: u8, a: u8 }` (serde) with `fn to_color(&self) -> crate::theme::Color`.
  - `enum Weight { Regular, SemiBold, Bold }`, `enum Easing { Linear, Cubic, Spring }`, `enum PresetId { Default, Glass, Neon, Minimal }` (all serde, `Default` derive where sensible).
  - `struct Appearance { palette_calm/attention/critical: Rgba, background: Rgba, text: Rgba, divider: Rgba, opacity: f32 }`
  - `struct Typography { family: String, size_pt: f32, weight: Weight }`
  - `struct Geometry { width: i32, height: i32, corner_radius: i32, bar_thickness: i32, spacing: i32 }`
  - `struct FillAnim { on: bool, easing: Easing, speed: f32 }`, `ShimmerAnim { on: bool, speed: f32, intensity: f32 }`, `AlertGlowAnim { on: bool, threshold: f32, intensity: f32 }`, `FadeSlideAnim { on: bool, duration_ms: u32 }`
  - `struct AnimationSettings { reduce_motion: bool, fill: FillAnim, shimmer: ShimmerAnim, alert_glow: AlertGlowAnim, fade_slide: FadeSlideAnim, preset: Option<PresetId> }`
  - `struct Settings { version: u32, /* existing fields */, appearance: Appearance, typography: Typography, geometry: Geometry, animation: AnimationSettings }`
  - `fn defaults_dark() -> Appearance` / `defaults_light()` reproducing current hardcoded colors.
  - `fn load() -> Settings`, `fn save(&Settings)`, `fn settings_path() -> PathBuf`.
- Consumes: `theme::Color` (Task 1).

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_json_gets_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.version, CURRENT_VERSION);
        assert_eq!(s.animation.fill.on, true);
        assert!(s.geometry.width > 0);
    }
    #[test]
    fn legacy_file_preserves_known_fields() {
        let legacy = r#"{"poll_interval_ms":900000,"show_claude_code":true,"widget_visible":false}"#;
        let s: Settings = serde_json::from_str(legacy).unwrap();
        assert_eq!(s.poll_interval_ms, 900000);
        assert_eq!(s.widget_visible, false);
        assert_eq!(s.show_claude_code, true);
        // new sections defaulted:
        assert_eq!(s.appearance.opacity, 1.0);
    }
    #[test]
    fn roundtrip() {
        let s = Settings::default();
        let js = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&js).unwrap();
        assert_eq!(back.geometry.height, s.geometry.height);
    }
}
```

- [ ] **Step 2: Run, verify fail** — `.\dev.ps1 test settings` → FAIL (module absent).
- [ ] **Step 3: Implement `settings.rs`.** Port the existing `SettingsFile` fields verbatim (see `window.rs:300-345`) keeping their `#[serde(default = ...)]` defaults. Add `version: u32` (`#[serde(default)] `, `CURRENT_VERSION = 1`). Add the new structs above, each field `#[serde(default)]` with a `Default` impl reproducing the current dark look (`background #1C1C1C`, `text #888888`, `divider`/track `#444444`, palette from the existing warm accent stops — copy from `claude_accent_color`). `Geometry` defaults from current constants (`width` = current `total_widget_width()` base, `height` = `WIDGET_HEIGHT`, etc.). `load()`/`save()` = move the current impls, bumping/writing `version`. Delete old copies from `window.rs` and point call sites at `settings::load/save`.
- [ ] **Step 4: Run + build** — `.\dev.ps1 test settings` PASS; `.\dev.ps1 build` OK.
- [ ] **Step 5: Commit** — `feat: extract settings module with appearance/animation schema + migration`.

### Task 3: Render reads Settings (behavior-preserving)

**Files:**
- Modify: `src/window.rs` (`render_layered`, `paint_content`, `draw_usage_bar`, font creation, geometry constants)
- Modify: `src/window.rs` global `Settings` accessor

**Interfaces:**
- Produces: `fn current_settings() -> Settings` (clone of a `static SETTINGS: Mutex<Settings>`), `fn set_settings(Settings)` (stores + triggers `render_layered`).
- Consumes: `settings::*`, `theme::Color`.

- [ ] **Step 1: Add `static SETTINGS: Mutex<Settings>`** initialized from `settings::load()` at startup (in `run()`), with `current_settings()`/`set_settings()` accessors.
- [ ] **Step 2:** In `render_layered`, replace hardcoded `bg_color/text_color/track` and accent stops with values from `current_settings().appearance` (dark/light picks the stored palette; usage→color interpolation uses `palette_calm/attention/critical` via `Color::lerp`). Replace `total_widget_width()`/`WIDGET_HEIGHT` with `geometry` (clamped — see Task 19).
- [ ] **Step 3:** In `paint_content`, build the font from `typography` (`CreateFontW` with family/size/weight) instead of the hardcoded font.
- [ ] **Step 4: Verify no visual change** — `.\dev.ps1 run`; confirm the widget looks identical to before (defaults reproduce current look). Kill the instance after.
- [ ] **Step 5: Commit** — `refactor: drive widget render from Settings (defaults unchanged)`.

---

## Phase 1 — Animation engine

### Task 4: Easing functions

**Files:** Create `src/animation.rs`; Modify `src/main.rs` (`mod animation;`); Test inline.

**Interfaces:** Produces `fn ease(kind: Easing, t: f32) -> f32` (`t` clamped 0..1) for `Linear`, `Cubic` (ease-in-out cubic), `Spring` (critically-damped-ish overshoot bounded to ~[0,1.05]).

- [ ] **Step 1: Failing tests**

```rust
#[test] fn linear_identity() { assert!((ease(Easing::Linear, 0.5) - 0.5).abs() < 1e-6); }
#[test] fn cubic_endpoints() { assert!(ease(Easing::Cubic,0.0).abs()<1e-6); assert!((ease(Easing::Cubic,1.0)-1.0).abs()<1e-6); }
#[test] fn cubic_monotone_midpoint() { assert!(ease(Easing::Cubic,0.25) < 0.25); }
#[test] fn spring_settles_at_one() { assert!((ease(Easing::Spring,1.0)-1.0).abs()<1e-3); }
```

- [ ] **Step 2: Run, fail.** `.\dev.ps1 test animation`
- [ ] **Step 3: Implement** the three easings (cubic in-out: `t<0.5 ? 4t³ : 1-(-2t+2)³/2`; spring: damped sine `1 - e^{-6t}·cos(6t)` clamped).
- [ ] **Step 4: Pass.** `.\dev.ps1 test animation`
- [ ] **Step 5: Commit** — `feat: easing functions for animation engine`.

### Task 5: AnimationClock + per-family state

**Files:** Modify `src/animation.rs`; Test inline.

**Interfaces:**
- Produces:
  - `struct AnimationFrame { fill_pcts: Vec<f32>, shimmer_phase: f32, glow_intensity: f32, fade_alpha: f32 }`
  - `struct AnimationClock { /* private */ }` with `new(&AnimationSettings) -> Self`, `set_targets(&[f32])`, `trigger_fade_in()`, `trigger_fade_out()`, `tick(&mut self, dt: Duration, usage_max: f32) -> (AnimationFrame, bool /* active */)`, `apply_settings(&AnimationSettings)`.
- Consumes: `settings::AnimationSettings`, `ease`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn fill_converges_to_target() {
    let mut c = AnimationClock::new(&AnimationSettings::default());
    c.set_targets(&[0.8]);
    let mut active = true; let mut f = AnimationFrame::default();
    for _ in 0..600 { let r = c.tick(Duration::from_millis(16), 0.8); f = r.0; active = r.1; if !active { break; } }
    assert!((f.fill_pcts[0]-0.8).abs() < 0.01);
    assert!(!active, "clock should go idle once settled");
}
#[test]
fn glow_zero_below_threshold() {
    let mut s = AnimationSettings::default(); s.alert_glow.on = true; s.alert_glow.threshold = 0.9;
    let mut c = AnimationClock::new(&s);
    let (f,_) = c.tick(Duration::from_millis(16), 0.5);
    assert_eq!(f.glow_intensity, 0.0);
}
#[test]
fn shimmer_phase_wraps_0_1() {
    let mut s = AnimationSettings::default(); s.shimmer.on = true;
    let mut c = AnimationClock::new(&s);
    let (f,_) = c.tick(Duration::from_millis(16), 0.5);
    assert!(f.shimmer_phase >= 0.0 && f.shimmer_phase < 1.0);
}
```

- [ ] **Step 2: Run, fail.**
- [ ] **Step 3: Implement.** Fill: advance each current→target by easing over `speed`-scaled time; "active" if any not settled OR shimmer/glow on OR fade in progress. Shimmer: `phase = (phase + dt*speed) % 1`. Glow: if `usage_max >= threshold` pulse `intensity*(0.5+0.5*sin)`, else 0. Fade: alpha ramps 0↔1 over `duration_ms`. `reduce_motion` → snap everything to final, always idle.
- [ ] **Step 4: Pass.**
- [ ] **Step 5: Commit** — `feat: animation clock with per-family interpolation`.

### Task 6: Integrate animation into the layered render

**Files:** Modify `src/window.rs` (add `static ANIM: Mutex<AnimationClock>`, timer, feed frame into `paint_content`; extend `paint_content`/`draw_usage_bar` params with animated fill/shimmer/glow/alpha).

**Interfaces:** Consumes `AnimationClock`, `AnimationFrame`. A `WM_TIMER` (id `IDT_ANIM`) at ~16 ms; started when `set_settings`/data update marks animation active; `render_layered` calls `ANIM.tick(...)`, uses `fill_pcts` for bar widths, draws a shimmer highlight band at `shimmer_phase`, adds a glow ring when `glow_intensity>0`, and multiplies the DIB alpha by `fade_alpha`. When `tick` returns `active=false`, `KillTimer`.

- [ ] **Step 1:** Add the timer plumbing + `ANIM` static; on data change call `ANIM.set_targets(...)` and `SetTimer`.
- [ ] **Step 2:** Thread animated values into `paint_content`/`draw_usage_bar` (bar fill uses `fill_pcts`; add shimmer/glow drawing; apply `fade_alpha` to the DIB before `UpdateLayeredWindow`).
- [ ] **Step 3: Manual verify** — `.\dev.ps1 run`; change frequency/force a poll and confirm the bar animates smoothly to the new value, shimmer sweeps, and glow appears when a bar is near the limit; confirm CPU returns to ~0 when idle (Task Manager).
- [ ] **Step 4: Commit** — `feat: animate bars (fill/shimmer/glow/fade) in layered render`.

---

## Phase 2 — Reusable GDI controls

### Task 7: `controls.rs` + `Slider`

**Files:** Create `src/controls.rs`; Modify `src/main.rs`; Test inline (value-mapping math only).

**Interfaces:**
- Produces:
  - `enum ControlEvent { Changed, CommitPreview }`
  - `trait Control { fn draw(&self, hdc: HDC, rect: RECT, dark: bool); fn on_mouse(&mut self, msg: u32, x: i32, y: i32, rect: RECT) -> Option<ControlEvent>; }`
  - `struct Slider { pub value: f32, pub min: f32, pub max: f32, dragging: bool }` with `fn pos_to_value(&self, x: i32, rect: RECT) -> f32` and `fn value_to_x(&self, rect: RECT) -> i32`.
- Consumes: `windows` GDI.

- [ ] **Step 1: Failing test** for `pos_to_value`/`value_to_x` roundtrip and clamping (pure math; construct `Slider` and a `RECT`, no HDC).

```rust
#[test]
fn slider_maps_and_clamps() {
    let s = Slider { value: 0.0, min: 0.0, max: 100.0, dragging: false };
    let r = RECT { left: 10, top: 0, right: 110, bottom: 20 };
    assert_eq!(s.pos_to_value(60, r).round(), 50.0);
    assert_eq!(s.pos_to_value(-999, r), 0.0);
    assert_eq!(s.pos_to_value(9999, r), 100.0);
}
```

- [ ] **Step 2: Fail.** `.\dev.ps1 test controls`
- [ ] **Step 3: Implement** `Slider` draw (track + filled portion + knob, rounded, dark-aware colors) + `on_mouse` (LBUTTONDOWN starts drag, MOUSEMOVE while dragging updates value → `Changed`, LBUTTONUP → `CommitPreview`) + the two math fns.
- [ ] **Step 4: Pass.**
- [ ] **Step 5: Commit** — `feat: reusable Slider control`.

### Task 8: `RgbaPicker`

**Files:** Modify `src/controls.rs`.

**Interfaces:** Produces `struct RgbaPicker { pub value: Rgba, /* 4 Sliders + hex edit state */ }` implementing `Control`; emits `Changed` on any channel move. Draws 4 labeled sliders (R/G/B/A), a swatch (alpha over checkerboard), and a `#RRGGBBAA` hex readout.

- [ ] **Step 1:** Implement composed from four `Slider`s (0..255) laid out in `draw`; `on_mouse` dispatches to the sub-slider under the cursor and recomputes `value`.
- [ ] **Step 2: Build** `.\dev.ps1 build` OK.
- [ ] **Step 3: Commit** — `feat: RGBA color picker control`.

### Task 9: `Dropdown` + font enumeration

**Files:** Modify `src/controls.rs`; add `fn enumerate_font_families() -> Vec<String>` (via `EnumFontFamiliesExW`).

**Interfaces:** Produces `struct Dropdown { pub items: Vec<String>, pub selected: usize, open: bool }` implementing `Control`; emits `Changed` on selection. `enumerate_font_families()` returns sorted unique family names.

- [ ] **Step 1:** Implement font enumeration (dedup, sort) + `Dropdown` (closed shows selected; click opens a scrollable list; click item selects/closes).
- [ ] **Step 2: Manual check** the list is populated (temporary log of count) — build OK.
- [ ] **Step 3: Commit** — `feat: Dropdown control + installed-font enumeration`.

### Task 10: `Segmented` + `Toggle`

**Files:** Modify `src/controls.rs`.

**Interfaces:** Produces `struct Segmented { pub options: Vec<String>, pub selected: usize }` and `struct Toggle { pub on: bool }`, both `Control`, emitting `Changed`.

- [ ] **Step 1:** Implement both (segmented = row of pill buttons, highlighted selection; toggle = animated switch or simple on/off pill).
- [ ] **Step 2: Build** OK.
- [ ] **Step 3: Commit** — `feat: Segmented and Toggle controls`.

---

## Phase 3 — Settings window

### Task 11: `config_window.rs` — window shell + section nav

**Files:** Create `src/config_window.rs`; Modify `src/main.rs`; Modify `src/window.rs` (function to open it).

**Interfaces:** Produces `fn open_config_window()` (idempotent: focuses existing if already open). Registers a top-level window class `CcumConfig`, dark chrome, left section list (`Appearance/Font/Size/Animations/Update/Presets`), right content panel, bottom button bar. Holds a `draft: Settings` (clone of `current_settings()`).

- [ ] **Step 1:** Register class, create window (centered, DPI-aware, dark titlebar via `DwmSetWindowAttribute` `USE_IMMERSIVE_DARK_MODE`), paint the frame + section list + empty content/button areas. Section click switches active section (repaint).
- [ ] **Step 2: Manual** — temporarily call `open_config_window()` at startup; `.\dev.ps1 run` shows the shell; section nav highlights. Remove the temp call.
- [ ] **Step 3: Commit** — `feat: settings window shell with section navigation`.

### Task 12: Live preview panel

**Files:** Modify `src/config_window.rs`; Modify `src/window.rs` (make `paint_content` callable with an explicit `&Settings` + `&AnimationFrame` + target HDC/size — extract a `fn paint_widget(hdc, w, h, &Settings, &AnimationFrame, ...usage data...)`).

**Interfaces:** Produces `paint_widget(...)` reused by both the real widget and the preview. Preview renders `draft` into a DIB and blits it into the preview rect, with a small demo dataset when live usage is unavailable.

- [ ] **Step 1:** Refactor `render_layered`/`paint_content` to funnel through `paint_widget(hdc,w,h,&settings,&frame,&usage)`.
- [ ] **Step 2:** In config window, on a ~16 ms timer, tick a local `AnimationClock` and draw `paint_widget` with `draft` into the preview rect.
- [ ] **Step 3: Manual** — open window; preview shows the widget with sample data and running animations.
- [ ] **Step 4: Commit** — `feat: WYSIWYG live preview in settings window`.

### Task 13: Wire section controls → draft

**Files:** Modify `src/config_window.rs`.

**Interfaces:** Each section instantiates the relevant controls bound to `draft` fields: Appearance = 6× `RgbaPicker` (calm/attention/critical/background/text/divider) + opacity `Slider`; Font = family `Dropdown` + size `Slider` + weight `Segmented`; Size = 5× `Slider` (width/height/radius/bar/spacing); Animations = 4 groups of `Toggle`+`Slider`(s) + reduce-motion `Toggle`; Update = frequency `Segmented` (+ custom `Slider`); Presets = 4 buttons (Task 16).

- [ ] **Step 1:** Route control `Changed` events into the matching `draft` field, then request preview repaint.
- [ ] **Step 2: Manual** — dragging any control updates the preview live (real widget unchanged).
- [ ] **Step 3: Commit** — `feat: bind settings controls to draft with live preview`.

### Task 14: Save / Cancel / Reset

**Files:** Modify `src/config_window.rs`; Modify `src/window.rs` (`set_settings` triggers re-render + re-embed).

**Interfaces:** `Save` → `settings::save(&draft)` + `window::set_settings(draft.clone())` (applies to the real widget, re-embeds if geometry changed) + close. `Cancel` → close, discard. `Reset` → `draft = Settings::default_preserving_position(current)` (keeps tray_offset/taskbar_index/model toggles; resets appearance/typography/geometry/animation).

- [ ] **Step 1:** Implement the three buttons + `default_preserving_position`.
- [ ] **Step 2: Manual** — change colors, Save → real widget updates and persists across restart; Cancel discards; Reset restores defaults in the preview.
- [ ] **Step 3: Commit** — `feat: save/cancel/reset in settings window`.

### Task 15: Open from menu + tray

**Files:** Modify `src/window.rs` (context menu build + `WM_COMMAND` handler; tray double-click).

**Interfaces:** New menu id `IDM_OPEN_SETTINGS`; item "Settings…" (localized) opens `config_window::open_config_window()`.

- [ ] **Step 1:** Add the menu item near the top of the right-click menu; handle its command; add tray double-click to open too.
- [ ] **Step 2: Manual** — right-click widget → Settings… opens the window.
- [ ] **Step 3: Commit** — `feat: open settings window from menu and tray`.

---

## Phase 4 — Presets, frequency sync, i18n, clamp

### Task 16: Presets

**Files:** Create `src/presets.rs`; Modify `src/main.rs`, `src/config_window.rs`.

**Interfaces:** Produces `fn apply_preset(id: PresetId, s: &mut Settings)` mutating only appearance+animation (not geometry/position). Four presets:
- **Default** — current look, all four animations on, subtle.
- **Glass** — translucent background (`opacity ~0.8`), soft shimmer strong, cool palette.
- **Neon** — saturated palette, strong glow, fast shimmer, dark background.
- **Minimal** — flat palette, animations off except gentle fill.

- [ ] **Step 1: Failing test** — `apply_preset(PresetId::Minimal, &mut s)` sets `s.animation.shimmer.on == false` and leaves `s.geometry` untouched.
- [ ] **Step 2: Fail → implement → pass.**
- [ ] **Step 3:** Wire 4 preset buttons in the Presets section → `apply_preset` on `draft` + repaint.
- [ ] **Step 4: Commit** — `feat: built-in style presets (Default/Glass/Neon/Minimal)`.

### Task 17: Frequency sync (menu ⇄ window)

**Files:** Modify `src/window.rs`, `src/config_window.rs`.

**Interfaces:** Both surfaces read/write `settings.poll_interval_ms`. Changing it in the menu updates the window if open (and vice-versa) via `current_settings()`/`set_settings()` + the existing poll-interval application path.

- [ ] **Step 1:** Ensure the menu's frequency handler routes through `set_settings` (single source of truth); the window's Update segment maps to the same presets + a custom minutes slider.
- [ ] **Step 2: Manual** — change frequency in the window → menu check-marks match; change in menu → window reflects it.
- [ ] **Step 3: Commit** — `feat: sync update frequency between menu and settings window`.

### Task 18: i18n

**Files:** Modify `src/localization/mod.rs` (+ the `Strings` struct) and all 10 locale files.

**Interfaces:** Add fields to `Strings` for every new UI label (section names, control labels, button labels, preset names, "reduce motion", etc.). Fill translations in `english.rs`, `portuguese_brazil.rs`, `spanish.rs`, `french.rs`, `german.rs`, `dutch.rs`, `japanese.rs`, `korean.rs`, `traditional_chinese.rs`, `russian.rs`.

- [ ] **Step 1:** Add the fields to `Strings`; add English + Portuguese first (compile).
- [ ] **Step 2:** Fill the remaining 8 locales.
- [ ] **Step 3: Build** — `.\dev.ps1 build` OK (missing field = compile error, so this is enforced).
- [ ] **Step 4: Commit** — `feat: localize settings window strings across all locales`.

### Task 19: Geometry clamp to taskbar

**Files:** Modify `src/window.rs` (geometry application path).

**Interfaces:** `fn clamp_geometry(g: Geometry, taskbar_rect: RECT) -> Geometry` — height ≤ taskbar usable height; width ≤ a max (e.g. `min(taskbar_width/2, 800px @96dpi)`); radius/bar/spacing bounded to sane ranges. Applied in `render_layered` and on Save.

- [ ] **Step 1: Failing test** for `clamp_geometry` (oversized height clamps to taskbar height).
- [ ] **Step 2: Fail → implement → pass.**
- [ ] **Step 3: Manual** — set a huge height in the window, Save → widget stays within the taskbar.
- [ ] **Step 4: Commit** — `feat: clamp widget geometry to the taskbar`.

---

## Phase 5 — Polish & verification

### Task 20: Final QA + release

- [ ] **Step 1:** Full run-through: each section, each preset, Save/Cancel/Reset, restart persistence, multi-poll animation, reduce-motion, DPI change if possible.
- [ ] **Step 2:** `.\dev.ps1 test` (all unit tests) PASS; `.\dev.ps1 release` builds.
- [ ] **Step 3:** Update `README.md` (new "Configurações" section + screenshot placeholder) and CHANGELOG if present.
- [ ] **Step 4: Commit** — `docs: document settings window` and (optional) bump version.

---

## Self-Review

**Spec coverage:** bespoke GDI window (T11-15) ✓; RGBA palette+bg/text/divider+opacity (T1,T13) ✓; usage semantics kept via palette lerp (T3) ✓; any installed font+size+weight (T3,T9,T13) ✓; geometry+clamp (T3,T19) ✓; 4 animation families (T4-6) ✓; live WYSIWYG preview (T12) ✓; Save/Cancel/Reset (T14) ✓; global scope, per-model deferred (schema leaves room) ✓; presets (T16) ✓; frequency both places (T17) ✓; i18n 10 locales (T18) ✓; testing (unit tasks throughout) ✓; migration (T2) ✓.

**Placeholders:** none (`preset` UI, custom-frequency slider, and demo dataset are all specified).

**Type consistency:** `Settings`/`Rgba`/`Weight`/`Easing`/`PresetId` defined in T2 and used consistently; `AnimationFrame`/`AnimationClock` from T5 used in T6/T12; `paint_widget` introduced in T12 and reused; `Control` trait from T7 implemented by T8-10 and consumed by T13.
