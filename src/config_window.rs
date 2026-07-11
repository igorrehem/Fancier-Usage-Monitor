//! Settings window shell (Task 11) plus live preview (Task 12) plus per-section controls
//! (Task 13) plus Save/Cancel/Reset (Task 14): a top-level, titled `CcumConfig` window with a
//! left section-nav list, and a right content panel split into a top preview strip -- a
//! WYSIWYG live preview of `draft` (via `window::paint_widget`, the same paint path the real
//! widget uses) with a running demo animation -- and, below it, the active section's controls,
//! all bound directly to `draft` fields. Dragging/clicking a control updates `draft` and
//! repaints the preview immediately; nothing here touches the real widget's global settings/
//! animation state until the bottom button bar's Save is clicked (`window::set_settings`) --
//! Cancel discards `draft`, Reset snaps it back to the real widget's current settings (see
//! `handle_button_action`).
//!
//! Appearance/Font/Size/Animations/Update/Presets sections all get real controls. Presets
//! (Task 16) is a 2x2 grid of clickable cards, one per `crate::presets::apply_preset` built-in
//! (Default/Glass/Neon/Minimal): clicking one mutates `draft`'s appearance+animation in place,
//! rebuilds `state.controls` from it (so Appearance/Animations' own cached widget values don't
//! go stale), and repaints the preview -- same as any other control, and same rebuild-on-
//! external-mutation pattern `ButtonAction::Reset` uses below.
//!
//! # Threading / message-loop architecture
//!
//! This window is created on the *same* UI thread as the main widget window and is pumped
//! by the *same, single* `GetMessageW` loop in `window::run()` (that loop passes
//! `HWND::default()` for its `hWnd` filter, which means "any window owned by this thread's
//! queue" -- so a second top-level window on this thread is picked up automatically). This
//! module must therefore never call `GetMessageW`/`DispatchMessageW`/`TranslateMessage`
//! itself, and its `WM_DESTROY` handler must never call `PostQuitMessage` (that would tear
//! down the whole app's message loop, not just this window).

// The only public entry point, `open_config_window`, isn't called from anywhere yet -- Task
// 15 wires it up to a menu/tray item. Until then the whole module is unreachable from
// `main`, which would otherwise flag every item here as dead code (matching the
// `#[allow(dead_code)]` markers already used the same way throughout `controls.rs`).
#![allow(dead_code)]

use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::animation::{AnimationClock, AnimationFrame};
use crate::controls::{Control, Dropdown, RgbaPicker, Segmented, Slider, Toggle};
use crate::diagnose;
use crate::localization::{self, LanguageId, Strings};
use crate::native_interop::{self, Color, IDT_PREVIEW_ANIM};
use crate::settings::{PaletteStops, PresetId, Rgba, Settings, Weight};
use crate::theme;
use crate::window::{self, UsageData};

const CLASS_NAME: &str = "CcumConfig";

/// Number of section-nav items (Appearance/Font/Size/Animations/Update/Presets); fixed
/// regardless of locale, unlike the labels themselves (see `sections`).
const SECTION_COUNT: usize = 6;

/// Section-nav labels, localized (Task 18). Order matches `ConfigState::active_section`'s
/// indices (0=Appearance .. 5=Presets) throughout this file.
fn sections(strings: &Strings) -> [&'static str; SECTION_COUNT] {
    [
        strings.section_appearance,
        strings.section_font,
        strings.section_size,
        strings.section_animations,
        strings.section_update,
        strings.section_presets,
    ]
}

// Layout constants, all in "baseline" (96 DPI) pixels; scaled at paint/hit-test time via
// `scale()` using this window's own live per-monitor DPI (see `effective_dpi`). This
// mirrors window.rs's `sc()` scaling formula, but is queried per-window rather than off the
// shared `CURRENT_DPI` global (which tracks the main widget window specifically) since the
// config window is a separate top-level window that can live on a different monitor.
// Widened/heightened from the Task 11/12 shell's 640x460 to fit Task 13's per-section
// controls (the Animations section in particular: 4 groups of toggle+slider rows) below the
// preview strip without scrolling (not implemented) -- see the report for the row-budget math
// that drove this choice.
const WINDOW_W: i32 = 700;
const WINDOW_H: i32 = 700;
const SIDEBAR_W: i32 = 170;
const SECTION_ITEM_H: i32 = 40;
const SECTION_TOP_PAD: i32 = 16;
const SECTION_TEXT_INSET: i32 = 20;
const BUTTON_BAR_H: i32 = 56;
const ACCENT_BAR_W: i32 = 3;

// --- Save/Cancel/Reset button bar (Task 14) ---
const BTN_W: i32 = 84;
const BTN_H: i32 = 30;
const BTN_GAP: i32 = 10;
const BTN_BAR_PAD: i32 = 16;

// --- Per-section control layout (Task 13) ---
//
// The content panel (right of the sidebar, above the button bar) is split top-to-bottom into
// a fixed-height preview strip (`PREVIEW_STRIP_H`) and, below it, the active section's
// controls, laid out via `RowCursor` (simple rows: a left label + a control to its right) or,
// for Appearance's six color pickers, a small 2-column grid (`appearance_grid`). `draw_*` and
// `dispatch_*` for a given section always call the *same* `*_layout` function so painting and
// hit-testing can never drift apart -- the same discipline `controls.rs` itself uses for e.g.
// `RgbaPicker::row_rect`.
const PREVIEW_STRIP_H: i32 = 110;
const CTRL_PAD: i32 = 16;
const CTRL_LABEL_W: i32 = 96;
const CTRL_CONTROL_GAP: i32 = 10;
const CTRL_ROW_H: i32 = 22;
const CTRL_ROW_GAP: i32 = 8;
const CTRL_HEADER_H: i32 = 18;
const CTRL_HEADER_GAP: i32 = 4;
const CTRL_GROUP_GAP: i32 = 12;

// RgbaPicker cells (Appearance section): a small text label above each picker's body. The
// picker's own *closed* row uses `CTRL_ROW_H`, the same single-row height every other control
// in this file uses (Slider/Toggle/Dropdown/Segmented rows) -- before the popover (Task 1),
// this used to be a dedicated `RGBA_PICKER_H = 84` tall enough for all four always-visible R/G/
// B/A sliders; now that's however tall the popover itself needs, computed per-picker via
// `RgbaPicker::popover_height()` and folded into the grid's row offsets (`appearance_row_offset`)
// only while that picker is open.
const RGBA_LABEL_H: i32 = 16;
const RGBA_LABEL_GAP: i32 = 4;
const RGBA_ROW_GAP: i32 = 12;
const RGBA_COL_GAP: i32 = 16;

// Presets section (Task 16, grown to a scrollable 24-preset grid in Task 5): a 2-column grid
// of large clickable "cards", one per `presets::ALL_PRESET_IDS` entry, grouped under 3 category
// headers (Built-in/Code editors/Apps -- see `preset_grid_order`). Still 2 columns rather than
// widening to 3+: at the window's default 700px width the card labels (several two-word names
// like "Solarized Dark"/"Tokyo Night"/"GitHub Dark") need the wider single-column card
// `presets_layout` already computes to stay legible without shrinking the font; 2 columns keeps
// that same card width from the original 4-preset grid instead of squeezing a 3rd column in.
const PRESET_CARD_H: i32 = 56;
const PRESET_CARD_GAP: i32 = 12;
/// Height reserved for the Presets section's intro sentence, which is long enough (especially
/// in several non-English locales) to need word-wrapping across multiple lines rather than the
/// single-line `CTRL_HEADER_H` row every other section header uses. Sized to comfortably fit
/// ~3 wrapped lines at the section-controls font (12px Segoe UI) within the controls area's
/// width at the window's default size -- generous rather than tight, since with 24 presets the
/// section now scrolls rather than needing to fit everything above the fold.
const PRESETS_INTRO_H: i32 = 48;
/// Width reserved on the right edge of the Presets content area for the passive scrollbar
/// indicator (Task 5), subtracted from the card grid's available width so cards never render
/// underneath it -- reserved unconditionally (not just when `max_offset > 0`) so the grid's
/// column width doesn't jump between "scrollbar visible" and "not visible" states as the window
/// is resized.
const PRESET_SCROLLBAR_GUTTER: i32 = 10;
/// Minimum on-screen width of the passive scrollbar indicator itself, inset within
/// `PRESET_SCROLLBAR_GUTTER`.
const PRESET_SCROLLBAR_W: i32 = 4;
/// Mouse-wheel scroll step, in baseline (96 DPI) pixels, applied per wheel notch (Windows'
/// `WHEEL_DELTA` = 120 units) to `ConfigState::presets_scroll_offset`. One card row (card height
/// + the gap above the next row) so a single notch moves the grid by a visually whole "step"
/// rather than a partial card, matching how most list/grid UIs scroll by a discrete unit instead
/// of an arbitrary pixel amount.
const PRESET_SCROLL_STEP: i32 = PRESET_CARD_H + PRESET_CARD_GAP;

/// Per-window state for the config window. Only one instance can exist at a time (enforced
/// by `open_config_window`'s idempotency check), so this is held in a global rather than via
/// `GWLP_USERDATA` -- matching how `window.rs` holds its own single-instance `AppState`.
struct ConfigState {
    /// Working copy of settings edited by this window. Cloned from `current_settings()` when
    /// the window opens; the live preview (this task) renders it, Task 13 wires controls to
    /// edit it, Task 14 adds Save/Cancel.
    draft: Settings,
    active_section: usize,
    /// Drives the live preview's bar-fill/shimmer/glow/fade animation. Constructed fresh for
    /// this window and ticked by `IDT_PREVIEW_ANIM` -- entirely independent of the main
    /// widget's global `ANIM` clock in `window.rs` (see that module's `with_anim`).
    preview_clock: AnimationClock,
    /// Most recent frame produced by `preview_clock.tick`, consumed by `draw_preview` on the
    /// next `WM_PAINT`.
    preview_frame: AnimationFrame,
    /// Wall-clock timestamp of the previous preview tick, mirroring `window.rs`'s
    /// `LAST_ANIM_TICK`: `None` both before the first tick and whenever the preview timer has
    /// been stopped (idle), so the next tick after a gap assumes one frame's worth (16ms)
    /// rather than a huge `dt`.
    preview_last_tick: Option<Instant>,
    /// Live `controls.rs` widgets for every wired section (Appearance/Font/Size/Animations/
    /// Update), seeded from `draft` when the window opens. These own their own drag/open
    /// state (see `Control::on_mouse`), so they're kept here across paints/mouse messages
    /// rather than rebuilt each time -- rebuilding from `draft` on every message would lose an
    /// in-progress drag or an open `Dropdown` list.
    controls: SectionControls,
    /// `draft.poll_interval_ms` as of the last time it was set by *this window* (window-open,
    /// Reset, or a prior `resync_external_frequency` refresh) -- never by the user dragging the
    /// Update section's own controls, which write `draft.poll_interval_ms` directly without
    /// touching this field. `resync_external_frequency` compares the two to tell "the user has
    /// an unsaved local frequency edit in progress" (fields differ) from "frequency in `draft`
    /// is still whatever this window last saw" (fields match, so it's safe to pull in an
    /// external change, e.g. from the main widget's right-click menu, without clobbering the
    /// user's own edit).
    last_synced_poll_interval_ms: u32,
    /// Vertical scroll position of the Presets section's card grid (Task 5), in pixels of
    /// content scrolled past the top of the section's content area -- `0` means scrolled all
    /// the way to the top (the initial/default state). Lives here rather than as a local in
    /// `draw_presets_controls`/`dispatch_presets` because it must persist across repaints and
    /// mouse messages (the same reason `controls`/`active_section` live here), and it's
    /// section-local rather than folded into `SectionControls` because it's not bound to any
    /// one `Settings` field the way every `SectionControls` member is -- it's pure transient UI
    /// state for one section's scroll position, reset only implicitly (never explicitly) since
    /// re-opening the window rebuilds `ConfigState` from scratch. Clamped to
    /// `[0, presets_max_scroll_offset(..)]` on every mutation (`WM_MOUSEWHEEL`), never negative.
    presets_scroll_offset: i32,
}

/// Display labels for `AppearanceControls::pickers`, in the same order as the array itself,
/// localized (Task 18).
fn appearance_picker_labels(strings: &Strings) -> [&'static str; 6] {
    [
        strings.color_calm,
        strings.color_attention,
        strings.color_critical,
        strings.color_background,
        strings.color_text,
        strings.color_divider,
    ]
}

/// Six `RgbaPicker`s bound to `Appearance`'s color fields, plus the opacity `Slider`.
/// `pickers` order matches `APPEARANCE_PICKER_LABELS`: [calm, attention, critical, background,
/// text, divider] -- the first three compose `draft.appearance.palette` (an
/// all-or-nothing `Option<PaletteStops>`; touching any one of them sets all three), the last
/// three are each their own independent `Option<Rgba>` override.
struct AppearanceControls {
    pickers: [RgbaPicker; 6],
    opacity: Slider,
}

struct FontControls {
    family: Dropdown,
    size: Slider,
    weight: Segmented,
}

/// One `Slider` per `Geometry` field. The brief's interfaces line lists "5x Slider
/// (width/height/radius/bar/spacing)", but `Geometry` (`settings.rs`) actually has six tunable
/// fields and no field literally named "width" (the widget's total width is *derived* from
/// these, not stored directly) -- rather than arbitrarily drop `label_width` or `text_width` to
/// force the count to 5, this binds all six real fields. See the task report for the full
/// reasoning.
struct SizeControls {
    height: Slider,
    corner_radius: Slider,
    bar_thickness: Slider,
    label_width: Slider,
    text_width: Slider,
    spacing: Slider,
}

/// Four groups (fill/shimmer/alert_glow/fade_slide), each an on/off `Toggle` plus its numeric
/// `Slider`(s), plus a standalone reduce-motion `Toggle`. Matches the brief's interfaces line
/// exactly ("4 groups of Toggle+Slider(s) + reduce-motion Toggle") -- `FillAnim::easing` etc.
/// are left unbound for this task since the brief doesn't call for an easing control here.
struct AnimationControls {
    fill_on: Toggle,
    fill_speed: Slider,
    shimmer_on: Toggle,
    shimmer_speed: Slider,
    shimmer_intensity: Slider,
    glow_on: Toggle,
    glow_threshold: Slider,
    glow_intensity: Slider,
    fade_on: Toggle,
    fade_duration: Slider,
    reduce_motion: Toggle,
}

/// Preset frequencies mirroring `window.rs`'s menu options (`IDM_FREQ_1MIN`/`_5MIN`/`_15MIN`/
/// `_1HOUR`), duplicated here as plain values rather than importing those private consts since
/// this task's file scope is `config_window.rs` only; full menu<->window sync is Task 17.
const FREQ_PRESETS_MS: [u32; 4] = [60_000, 300_000, 900_000, 3_600_000];

/// Frequency segment labels, localized (Task 18): the four presets (reusing the same
/// `Strings` fields the main widget's right-click menu already uses for these exact same
/// durations -- `one_minute`/`five_minutes`/`fifteen_minutes`/`one_hour`) plus "Custom".
fn frequency_labels(strings: &Strings) -> [&'static str; 5] {
    [
        strings.one_minute,
        strings.five_minutes,
        strings.fifteen_minutes,
        strings.one_hour,
        strings.frequency_custom,
    ]
}

/// Display name for preset card `id`, in the Presets section's card grid (Task 16, grown to 24
/// presets in Task 5). The original 4 built-ins (Default/Glass/Neon/Minimal) predate Task 4's
/// `presets::theme_display_name` and stay localized via `Strings` -- "Glass"/"Neon"/"Minimal"
/// read as descriptive style adjectives rather than proper brand names (unlike e.g. a font
/// family name), so they're translated like any other label, and existing locale files already
/// carry that translation work. The 20 names added in Task 3 are deliberately English-only
/// proper nouns (see `presets::theme_display_name`'s doc comment) with no `Strings` entry to
/// fall back to, so they always go through `theme_display_name`.
fn preset_display_name(id: PresetId, strings: &Strings) -> &'static str {
    match id {
        PresetId::Default => strings.preset_default,
        PresetId::Glass => strings.preset_glass,
        PresetId::Neon => strings.preset_neon,
        PresetId::Minimal => strings.preset_minimal,
        other => crate::presets::theme_display_name(other),
    }
}

/// All 24 presets (`presets::ALL_PRESET_IDS`), reordered into the Presets section's actual
/// on-screen grid order: grouped by `presets::theme_category` into the 3 fixed groups (Built-in,
/// Code editors, Apps, in that order -- matching `PresetCategory`'s own declaration order and
/// `strings.preset_category_*`), preserving each preset's original relative order *within* its
/// group. Recomputed on every call (draw and dispatch alike) rather than cached, mirroring this
/// file's existing `*_layout` discipline of never persisting derived layout state -- 3 groups x
/// 24 candidates is a trivial ~72-comparison cost, not worth a global/lazy_static for a settings
/// window that repaints at most a few dozen times a second.
fn preset_grid_order() -> [PresetId; 24] {
    let mut order = [PresetId::Default; 24];
    let mut i = 0;
    for &category in &[
        crate::presets::PresetCategory::Builtin,
        crate::presets::PresetCategory::Editors,
        crate::presets::PresetCategory::Apps,
    ] {
        for &id in crate::presets::ALL_PRESET_IDS.iter() {
            if crate::presets::theme_category(id) == category {
                order[i] = id;
                i += 1;
            }
        }
    }
    debug_assert_eq!(i, 24, "preset_grid_order must place all 24 presets exactly once");
    order
}

/// Frequency `Segmented` (presets + "Custom") plus a custom `Slider` in whole minutes, both
/// bound to `draft.poll_interval_ms`. Selecting a preset segment sets the interval directly;
/// dragging the custom slider sets it to that many minutes and re-syncs the segment selection
/// (snapping back to a preset's pill if the dragged value happens to land on one).
struct UpdateControls {
    frequency: Segmented,
    /// Whole minutes, not milliseconds -- friendlier to drag than a 60,000-wide range.
    custom_minutes: Slider,
}

struct SectionControls {
    appearance: AppearanceControls,
    font: FontControls,
    size: SizeControls,
    animations: AnimationControls,
    update: UpdateControls,
}

/// Fallback background/text/divider colors mirroring `window.rs::derive_colors`'s hardcoded
/// adaptive defaults (duplicated here, not exported, since this task's file scope is
/// `config_window.rs` only) -- used purely to seed the RgbaPicker's starting swatch when the
/// corresponding `Appearance` field is `None` (no override set yet), so the picker opens
/// showing what the widget currently actually renders instead of an arbitrary color.
fn adaptive_default_colors(is_dark: bool) -> (Color, Color, Color) {
    let bg = if is_dark {
        Color::from_hex("#1C1C1C")
    } else {
        Color::from_hex("#F3F3F3")
    };
    let text = if is_dark {
        Color::from_hex("#888888")
    } else {
        Color::from_hex("#404040")
    };
    let divider = if is_dark {
        Color::from_hex("#444444")
    } else {
        Color::from_hex("#AAAAAA")
    };
    (bg, text, divider)
}

/// Seed values for `draft.appearance.palette`'s calm/attention/critical colors when it's
/// `None` (no palette override set yet). Unlike background/text/divider there's no "current
/// adaptive" gradient to fall back to -- a `None` palette means the renderer keeps each
/// model's own accent color rather than interpolating a gradient at all (see
/// `window.rs::palette_color`'s call site) -- so these are just a reasonable starting
/// green/amber/red usage-severity gradient for the picker to open with.
fn default_palette_seed() -> PaletteStops {
    PaletteStops {
        calm: Rgba::from_color(Color::from_hex("#4CAF7A")),
        attention: Rgba::from_color(Color::from_hex("#E8A33D")),
        critical: Rgba::from_color(Color::from_hex("#D9534F")),
    }
}

/// Resolves this window's active `Strings` from `settings.language` (falling back to the
/// system default when unset/invalid) -- the single place every section of this file goes to
/// answer "what language is the settings window showing right now" (Task 18).
fn strings_for(settings: &Settings) -> Strings {
    localization::resolve_language(settings.language.as_deref().and_then(LanguageId::from_code))
        .strings()
}

/// Maps `poll_interval_ms` to (segment index, custom-slider minutes). Falls back to the
/// "Custom" segment (index 4) with the equivalent minute count whenever the value doesn't
/// exactly match one of `FREQ_PRESETS_MS`.
fn frequency_selection(poll_interval_ms: u32) -> (usize, f32) {
    let minutes = (poll_interval_ms as f32 / 60_000.0).max(1.0);
    match FREQ_PRESETS_MS.iter().position(|&p| p == poll_interval_ms) {
        Some(idx) => (idx, minutes),
        None => (4, minutes),
    }
}

impl SectionControls {
    fn from_settings(s: &Settings, is_dark: bool, strings: &Strings) -> Self {
        let (bg_default, text_default, divider_default) = adaptive_default_colors(is_dark);
        let palette = s.appearance.palette.unwrap_or_else(default_palette_seed);
        let background = s
            .appearance
            .background
            .unwrap_or_else(|| Rgba::from_color(bg_default));
        let text = s
            .appearance
            .text
            .unwrap_or_else(|| Rgba::from_color(text_default));
        let divider = s
            .appearance
            .divider
            .unwrap_or_else(|| Rgba::from_color(divider_default));
        let appearance = AppearanceControls {
            pickers: [
                RgbaPicker::new(palette.calm, strings.rgba_custom.to_string()),
                RgbaPicker::new(palette.attention, strings.rgba_custom.to_string()),
                RgbaPicker::new(palette.critical, strings.rgba_custom.to_string()),
                RgbaPicker::new(background, strings.rgba_custom.to_string()),
                RgbaPicker::new(text, strings.rgba_custom.to_string()),
                RgbaPicker::new(divider, strings.rgba_custom.to_string()),
            ],
            opacity: Slider::new(s.appearance.opacity, 0.0, 1.0),
        };

        let mut families = crate::controls::enumerate_font_families();
        if families.is_empty() {
            // No installed fonts enumerated (shouldn't happen on a real machine, but keeps the
            // Dropdown's invariant of never being empty-but-still-showing-a-selection intact).
            families.push(s.typography.family.clone());
        }
        let family_idx = families
            .iter()
            .position(|f| f == &s.typography.family)
            .unwrap_or(0);
        let weight_idx = match s.typography.weight {
            Weight::Regular => 0,
            Weight::SemiBold => 1,
            Weight::Bold => 2,
        };
        let font = FontControls {
            family: Dropdown::new(families, family_idx),
            // 6..18pt: comfortably brackets the 9.0pt default (see settings.rs's
            // `default_font_size` doc comment) without allowing degenerate/illegible sizes.
            size: Slider::new(s.typography.size_pt, 6.0, 18.0),
            weight: Segmented::new(
                vec![
                    strings.weight_regular.to_string(),
                    strings.weight_semibold.to_string(),
                    strings.weight_bold.to_string(),
                ],
                weight_idx,
            ),
        };

        let g = &s.geometry;
        let size = SizeControls {
            height: Slider::new(g.height as f32, 30.0, 80.0),
            corner_radius: Slider::new(g.corner_radius as f32, 0.0, 12.0),
            bar_thickness: Slider::new(g.bar_thickness as f32, 6.0, 24.0),
            label_width: Slider::new(g.label_width as f32, 8.0, 40.0),
            text_width: Slider::new(g.text_width as f32, 30.0, 100.0),
            spacing: Slider::new(g.spacing as f32, 0.0, 6.0),
        };

        let a = &s.animation;
        let animations = AnimationControls {
            fill_on: Toggle::new(a.fill.on),
            fill_speed: Slider::new(a.fill.speed, 0.1, 5.0),
            shimmer_on: Toggle::new(a.shimmer.on),
            shimmer_speed: Slider::new(a.shimmer.speed, 0.1, 3.0),
            shimmer_intensity: Slider::new(a.shimmer.intensity, 0.0, 1.0),
            glow_on: Toggle::new(a.alert_glow.on),
            glow_threshold: Slider::new(a.alert_glow.threshold, 0.0, 1.0),
            glow_intensity: Slider::new(a.alert_glow.intensity, 0.0, 1.0),
            fade_on: Toggle::new(a.fade_slide.on),
            fade_duration: Slider::new(a.fade_slide.duration_ms as f32, 50.0, 1000.0),
            reduce_motion: Toggle::new(a.reduce_motion),
        };

        let (freq_idx, custom_minutes) = frequency_selection(s.poll_interval_ms);
        let update = UpdateControls {
            frequency: Segmented::new(
                frequency_labels(strings).iter().map(|s| s.to_string()).collect(),
                freq_idx,
            ),
            custom_minutes: Slider::new(custom_minutes, 1.0, 240.0),
        };

        Self {
            appearance,
            font,
            size,
            animations,
            update,
        }
    }
}

/// Hardcoded placeholder usage numbers for the live preview. The settings window has no live
/// poll data (it isn't polling), so the preview always shows the same plausible Claude Code
/// session/weekly figures regardless of what the real widget is currently displaying -- just
/// enough to prove `paint_widget` renders end to end with `draft`'s appearance/geometry/
/// typography/animation settings applied.
fn demo_usage_data() -> UsageData {
    UsageData {
        session_pct: 62.0,
        session_text: "62% \u{00b7} 3h".to_string(),
        weekly_pct: 34.0,
        weekly_text: "34% \u{00b7} 4d".to_string(),
        codex_session_pct: 0.0,
        codex_session_text: String::new(),
        codex_weekly_pct: 0.0,
        codex_weekly_text: String::new(),
        antigravity_session_pct: 0.0,
        antigravity_session_text: String::new(),
        antigravity_weekly_pct: 0.0,
        antigravity_weekly_text: String::new(),
        show_claude_code: true,
        show_codex: false,
        show_antigravity: false,
    }
}

/// Fill-animation targets (0.0..=1.0) for `demo_usage_data`'s two visible bars, in
/// `[claude.session, claude.weekly]` order. This mirrors `window.rs`'s private
/// `ordered_bar_slots` contract (Claude Code's session/weekly pair always comes first, and
/// the demo dataset never shows Codex/Antigravity, so this pair is always the complete
/// order) -- `paint_widget` maps `AnimationFrame::fill_pcts` back onto bars using that same
/// index contract.
const DEMO_TARGETS: [f32; 2] = [0.62, 0.34];

static CONFIG_STATE: Mutex<Option<ConfigState>> = Mutex::new(None);

/// Wrapper to make HWND sendable across threads/storable in a `static`, mirroring
/// `window.rs`'s `SendHwnd` (HWND itself is a raw pointer newtype and isn't `Send`).
#[derive(Clone, Copy)]
struct SendHwnd(isize);

unsafe impl Send for SendHwnd {}

impl SendHwnd {
    fn from_hwnd(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }
    fn to_hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }
}

/// Tracks the single live config window instance, if any, so `open_config_window` can be
/// idempotent (focus instead of creating a second window).
static CONFIG_HWND: Mutex<Option<SendHwnd>> = Mutex::new(None);

static CLASS_REGISTERED: Once = std::sync::Once::new();

/// Open the settings window, or focus it if it's already open. Safe to call repeatedly.
pub fn open_config_window() {
    unsafe {
        let existing = *CONFIG_HWND.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = existing {
            let hwnd = h.to_hwnd();
            if IsWindow(hwnd).as_bool() {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
                return;
            }
            // Stale handle (window was destroyed without going through our WM_DESTROY
            // cleanup, e.g. killed externally) -- clear it and fall through to recreate.
            *CONFIG_HWND.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }

        create_window();
    }
}

unsafe fn create_window() {
    let hinstance = match GetModuleHandleW(PCWSTR::null()) {
        Ok(h) => h,
        Err(error) => {
            diagnose::log_error("config_window: GetModuleHandleW failed", error);
            return;
        }
    };

    let class_wide = native_interop::wide_str(CLASS_NAME);

    CLASS_REGISTERED.call_once(|| {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(config_wnd_proc),
            hInstance: HINSTANCE(hinstance.0),
            hCursor: LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszClassName: PCWSTR::from_raw(class_wide.as_ptr()),
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            diagnose::log("config_window: RegisterClassExW returned 0");
        }
    });

    // Seed the draft state *before* the window exists so a synchronous WM_PAINT arriving
    // during/just after CreateWindowExW always finds state populated.
    let draft = crate::window::current_settings();
    let mut preview_clock = AnimationClock::new(&draft.animation);
    // Seed at zero, then set the real demo targets, so the first few preview ticks animate
    // the bars growing in from empty -- a visible "this is live" cue when the settings
    // window opens, mirroring how the real widget looks on its first poll.
    preview_clock.set_targets(&[0.0; DEMO_TARGETS.len()]);
    preview_clock.set_targets(&DEMO_TARGETS);
    let usage_max = DEMO_TARGETS.iter().cloned().fold(0.0f32, f32::max);
    let (preview_frame, _) = preview_clock.tick(Duration::ZERO, usage_max);

    let strings = strings_for(&draft);
    let controls = SectionControls::from_settings(&draft, theme::is_dark_mode(), &strings);
    let last_synced_poll_interval_ms = draft.poll_interval_ms;

    *CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(ConfigState {
        draft,
        active_section: 0,
        preview_clock,
        preview_frame,
        preview_last_tick: None,
        controls,
        last_synced_poll_interval_ms,
        presets_scroll_offset: 0,
    });

    // Size/position using the primary monitor's current DPI; refined per-monitor via
    // GetDpiForWindow once the window exists (and kept in sync via WM_DPICHANGED).
    let dpi = GetDpiForSystem();
    let width = scale(WINDOW_W, dpi);
    let height = scale(WINDOW_H, dpi);
    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);
    let x = ((screen_w - width) / 2).max(0);
    let y = ((screen_h - height) / 2).max(0);

    // Reuses `strings.settings` (also the main context menu's "Settings" submenu label) as
    // this window's title bar text rather than adding a dedicated field -- both mean the same
    // bare noun "Settings", just in different chrome (`open_settings`'s "Settings…" has a
    // trailing ellipsis implying a further action, which doesn't fit a title bar).
    let title_wide = native_interop::wide_str(strings.settings);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR::from_raw(class_wide.as_ptr()),
        PCWSTR::from_raw(title_wide.as_ptr()),
        WS_OVERLAPPEDWINDOW,
        x,
        y,
        width,
        height,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    );

    let hwnd = match hwnd {
        Ok(h) => h,
        Err(error) => {
            diagnose::log_error("config_window: CreateWindowExW failed", error);
            *CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return;
        }
    };

    apply_dark_titlebar(hwnd, theme::is_dark_mode());

    *CONFIG_HWND.lock().unwrap_or_else(|e| e.into_inner()) = Some(SendHwnd::from_hwnd(hwnd));

    let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
    let _ = SetForegroundWindow(hwnd);
    let _ = UpdateWindow(hwnd);

    // `dpi` above (GetDpiForSystem) sized the CreateWindowExW call since there's no HWND yet
    // to ask for the *monitor's* actual DPI, and (empirically) `GetDpiForWindow` doesn't
    // reliably reflect the window's real per-monitor DPI until after it's actually been shown
    // -- querying it immediately post-creation, before ShowWindow, can still read back the
    // system DPI even on a monitor whose real scale differs. So this check runs here, right
    // after `UpdateWindow`, once the window is actually on-screen. If the two DPIs disagree
    // (system DPI setting lagging/differing from the primary monitor's actual scale -- not
    // unusual on multi-monitor or freshly-changed-scaling setups), the window would otherwise
    // end up physically smaller than every subsequent layout calculation (which all use this
    // same per-window `effective_dpi`/`GetDpiForWindow`) assumes, clipping Task 13's
    // per-section controls -- so resize it now, exactly like `WM_DPICHANGED` already does for
    // a *later* monitor change.
    let real_dpi = effective_dpi(hwnd);
    if real_dpi != dpi {
        let real_width = scale(WINDOW_W, real_dpi);
        let real_height = scale(WINDOW_H, real_dpi);
        let real_x = ((screen_w - real_width) / 2).max(0);
        let real_y = ((screen_h - real_height) / 2).max(0);
        let _ = SetWindowPos(
            hwnd,
            HWND::default(),
            real_x,
            real_y,
            real_width,
            real_height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }

    // Drives the live preview's animation while it has active work (fill grow-in, and
    // shimmer/glow if enabled in `draft.animation`); stopped by `tick_preview` once settled,
    // matching window.rs's IDT_ANIM idle-stop pattern.
    let _ = SetTimer(hwnd, IDT_PREVIEW_ANIM, 16, None);

    diagnose::log(format!("config window created hwnd={:?}", hwnd));
}

/// Applies (or removes) the native dark titlebar/frame via DWM. Best-effort: on older
/// Windows builds without `DWMWA_USE_IMMERSIVE_DARK_MODE` support this is a harmless no-op
/// (the call fails and is ignored), leaving a light titlebar over our dark client area.
unsafe fn apply_dark_titlebar(hwnd: HWND, dark: bool) {
    let value: BOOL = if dark { BOOL(1) } else { BOOL(0) };
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &value as *const BOOL as *const std::ffi::c_void,
        std::mem::size_of::<BOOL>() as u32,
    );
}

/// This window's own live per-monitor DPI (falls back to 96 = 100% if the query fails).
fn effective_dpi(hwnd: HWND) -> u32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi > 0 {
        dpi
    } else {
        96
    }
}

/// Scale a baseline (96 DPI) pixel value to `dpi`, mirroring window.rs's `sc()` formula.
fn scale(px: i32, dpi: u32) -> i32 {
    (px as f64 * dpi as f64 / 96.0).round() as i32
}

/// Client rects for each section-nav item, top to bottom, spanning the sidebar's width.
fn section_rects(dpi: u32) -> Vec<RECT> {
    let item_h = scale(SECTION_ITEM_H, dpi);
    let top_pad = scale(SECTION_TOP_PAD, dpi);
    let sidebar_w = scale(SIDEBAR_W, dpi);
    (0..SECTION_COUNT)
        .map(|i| {
            let top = top_pad + i as i32 * item_h;
            RECT {
                left: 0,
                top,
                right: sidebar_w,
                bottom: top + item_h,
            }
        })
        .collect()
}

/// Index of the section-nav item containing client point `(x, y)`, if any.
fn section_at(x: i32, y: i32, dpi: u32) -> Option<usize> {
    section_rects(dpi)
        .into_iter()
        .position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
}

fn fill_rect(hdc: HDC, rect: RECT, color: &Color) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color.to_colorref()));
        FillRect(hdc, &rect, brush);
        let _ = DeleteObject(brush);
    }
}

unsafe fn paint(hdc: HDC, hwnd: HWND) {
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let client_w = client.right - client.left;
    let client_h = client.bottom - client.top;
    let dpi = effective_dpi(hwnd);
    let dark = theme::is_dark_mode();

    let (active_section, strings) = {
        let guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(s) => (s.active_section, strings_for(&s.draft)),
            None => (0, strings_for(&Settings::default())),
        }
    };

    let bg = if dark {
        Color::new(0x1e, 0x1e, 0x1e)
    } else {
        Color::new(0xf5, 0xf5, 0xf5)
    };
    let sidebar_bg = if dark {
        Color::new(0x25, 0x25, 0x26)
    } else {
        Color::new(0xea, 0xea, 0xea)
    };
    let divider = if dark {
        Color::new(0x3a, 0x3a, 0x3a)
    } else {
        Color::new(0xd0, 0xd0, 0xd0)
    };
    let text_color = if dark {
        Color::new(0xf0, 0xf0, 0xf0)
    } else {
        Color::new(0x20, 0x20, 0x20)
    };
    let muted_text = if dark {
        Color::new(0xb0, 0xb0, 0xb0)
    } else {
        Color::new(0x50, 0x50, 0x50)
    };
    let active_tint = if dark {
        Color::new(0x33, 0x2a, 0x26)
    } else {
        Color::new(0xf3, 0xe2, 0xdb)
    };
    let accent = Color::new(0xd9, 0x77, 0x57);

    let sidebar_w = scale(SIDEBAR_W, dpi);
    let button_bar_h = scale(BUTTON_BAR_H, dpi);
    let body_bottom = client_h - button_bar_h;

    // Whole-window background (covers the content panel and button bar; sidebar painted
    // over next).
    fill_rect(hdc, client, &bg);

    // Sidebar.
    let sidebar_rect = RECT {
        left: 0,
        top: 0,
        right: sidebar_w,
        bottom: body_bottom,
    };
    fill_rect(hdc, sidebar_rect, &sidebar_bg);

    // Divider between sidebar and content panel.
    fill_rect(
        hdc,
        RECT {
            left: sidebar_w,
            top: 0,
            right: sidebar_w + 1,
            bottom: body_bottom,
        },
        &divider,
    );

    // Divider above the button bar.
    fill_rect(
        hdc,
        RECT {
            left: 0,
            top: body_bottom,
            right: client_w,
            bottom: body_bottom + 1,
        },
        &divider,
    );

    draw_button_bar(hdc, client_w, client_h, dpi, dark, &strings);

    let _ = SetBkMode(hdc, TRANSPARENT);
    let font_name = native_interop::wide_str("Segoe UI");
    let font = CreateFontW(
        -scale(14, dpi),
        0,
        0,
        0,
        FW_MEDIUM.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_TT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        PCWSTR::from_raw(font_name.as_ptr()),
    );
    let old_font = SelectObject(hdc, font);

    let section_labels = sections(&strings);
    for (i, item_rect) in section_rects(dpi).into_iter().enumerate() {
        if i == active_section {
            fill_rect(hdc, item_rect, &active_tint);
            fill_rect(
                hdc,
                RECT {
                    left: 0,
                    top: item_rect.top,
                    right: scale(ACCENT_BAR_W, dpi),
                    bottom: item_rect.bottom,
                },
                &accent,
            );
            let _ = SetTextColor(hdc, COLORREF(text_color.to_colorref()));
        } else {
            let _ = SetTextColor(hdc, COLORREF(muted_text.to_colorref()));
        }

        let mut label_rect = RECT {
            left: item_rect.left + scale(SECTION_TEXT_INSET, dpi),
            top: item_rect.top,
            right: item_rect.right - scale(8, dpi),
            bottom: item_rect.bottom,
        };
        let mut label_wide: Vec<u16> = section_labels[i].encode_utf16().collect();
        let _ = DrawTextW(
            hdc,
            &mut label_wide,
            &mut label_rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }

    SelectObject(hdc, old_font);
    let _ = DeleteObject(font);

    // Content panel (right of the sidebar divider, above the button bar): a fixed-height live
    // preview strip on top, and the active section's controls below it.
    let content_rect = RECT {
        left: sidebar_w + 1,
        top: 0,
        right: client_w,
        bottom: body_bottom,
    };
    let (preview_rect, controls_area) = split_content(content_rect, dpi);
    draw_preview(hdc, preview_rect, dpi);
    fill_rect(
        hdc,
        RECT {
            left: content_rect.left,
            top: preview_rect.bottom,
            right: content_rect.right,
            bottom: preview_rect.bottom + 1,
        },
        &divider,
    );
    draw_section_controls(hdc, controls_area, dpi, dark, active_section, &strings);
}

/// Client rect of the content panel (right of the sidebar divider, spanning down to the
/// button bar), plus this window's live DPI. Computed fresh from `hwnd`'s current client rect
/// each call so it always matches what `paint` just drew, whether called from `WM_PAINT` or
/// (via `split_content`) from a mouse-message handler routing a click/drag to a control.
fn content_rect_for(hwnd: HWND) -> (RECT, u32) {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    let dpi = effective_dpi(hwnd);
    let sidebar_w = scale(SIDEBAR_W, dpi);
    let button_bar_h = scale(BUTTON_BAR_H, dpi);
    let client_w = client.right - client.left;
    let client_h = client.bottom - client.top;
    let body_bottom = client_h - button_bar_h;
    let content_rect = RECT {
        left: sidebar_w + 1,
        top: 0,
        right: client_w,
        bottom: body_bottom,
    };
    (content_rect, dpi)
}

/// Splits a content-panel rect into (preview strip, controls area), padding the latter by
/// `CTRL_PAD` on all sides. Both halves are clamped so they never invert (bottom < top) even
/// if the window is shrunk below the preview strip's own fixed height.
fn split_content(content_rect: RECT, dpi: u32) -> (RECT, RECT) {
    let preview_h = scale(PREVIEW_STRIP_H, dpi);
    let preview_rect = RECT {
        left: content_rect.left,
        top: content_rect.top,
        right: content_rect.right,
        bottom: (content_rect.top + preview_h).min(content_rect.bottom),
    };
    let pad = scale(CTRL_PAD, dpi);
    let controls_top = (preview_rect.bottom + 1 + pad).min(content_rect.bottom);
    let controls_area = RECT {
        left: content_rect.left + pad,
        top: controls_top,
        right: (content_rect.right - pad).max(content_rect.left + pad),
        bottom: (content_rect.bottom - pad).max(controls_top),
    };
    (preview_rect, controls_area)
}

// --- Save/Cancel/Reset button bar (Task 14) ---

/// Which button (if any) a click landed on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ButtonAction {
    Save,
    Cancel,
    Reset,
}

/// Rects for the three button-bar buttons, given the window's current client size: `Reset`
/// pinned to the left edge, `Cancel`/`Save` pinned to the right edge (`Save` rightmost,
/// matching the common OK-is-rightmost convention), all vertically centered in the bar.
/// Shared by `paint` (drawing) and the `WM_LBUTTONDOWN` handler (hit-testing) so the two can
/// never drift apart, the same discipline used by `split_content`/`content_rect_for` above.
fn button_rects(client_w: i32, client_h: i32, dpi: u32) -> (RECT, RECT, RECT) {
    let bar_h = scale(BUTTON_BAR_H, dpi);
    let body_bottom = client_h - bar_h;
    let btn_w = scale(BTN_W, dpi);
    let btn_h = scale(BTN_H, dpi);
    let gap = scale(BTN_GAP, dpi);
    let pad = scale(BTN_BAR_PAD, dpi);

    let top = body_bottom + (bar_h - btn_h) / 2;
    let bottom = top + btn_h;

    let save_right = client_w - pad;
    let save_left = save_right - btn_w;
    let save = RECT { left: save_left, top, right: save_right, bottom };

    let cancel_right = save_left - gap;
    let cancel_left = cancel_right - btn_w;
    let cancel = RECT { left: cancel_left, top, right: cancel_right, bottom };

    let reset_left = pad;
    let reset_right = reset_left + btn_w;
    let reset = RECT { left: reset_left, top, right: reset_right, bottom };

    (reset, cancel, save)
}

/// `button_rects` computed fresh from `hwnd`'s live client size/DPI, mirroring
/// `content_rect_for`.
fn button_rects_for(hwnd: HWND) -> (RECT, RECT, RECT) {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    let dpi = effective_dpi(hwnd);
    button_rects(client.right - client.left, client.bottom - client.top, dpi)
}

/// Which button (if any) contains client point `(x, y)`.
fn button_at(x: i32, y: i32, reset: RECT, cancel: RECT, save: RECT) -> Option<ButtonAction> {
    let hit = |r: RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    if hit(save) {
        Some(ButtonAction::Save)
    } else if hit(cancel) {
        Some(ButtonAction::Cancel)
    } else if hit(reset) {
        Some(ButtonAction::Reset)
    } else {
        None
    }
}

/// Draws one button: `Save` as a filled accent "primary" button, `Cancel`/`Reset` as bordered
/// "secondary" buttons -- the same accent color and dark-mode-aware neutral tones `paint` (the
/// sidebar's active-section highlight) already uses elsewhere in this window, rather than
/// inventing a new palette just for these three buttons.
unsafe fn draw_button(hdc: HDC, rect: RECT, dark: bool, label: &str, primary: bool) {
    let accent = Color::new(0xd9, 0x77, 0x57);
    let (bg, text_color, border) = if primary {
        (accent, Color::new(0xff, 0xff, 0xff), None)
    } else {
        let bg = if dark {
            Color::new(0x2a, 0x2a, 0x2a)
        } else {
            Color::new(0xff, 0xff, 0xff)
        };
        let text = if dark {
            Color::new(0xf0, 0xf0, 0xf0)
        } else {
            Color::new(0x20, 0x20, 0x20)
        };
        let border = if dark {
            Color::new(0x45, 0x45, 0x45)
        } else {
            Color::new(0xc0, 0xc0, 0xc0)
        };
        (bg, text, Some(border))
    };

    fill_rect(hdc, rect, &bg);
    if let Some(border) = border {
        let pen = CreatePen(PS_SOLID, 1, COLORREF(border.to_colorref()));
        let old_pen = SelectObject(hdc, pen);
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen);
    }

    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, COLORREF(text_color.to_colorref()));
    let mut wide: Vec<u16> = label.encode_utf16().collect();
    let mut r = rect;
    let _ = DrawTextW(hdc, &mut wide, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}

/// Draws the three buttons into the button bar, given the window's client size/DPI. Called
/// from `paint` after the divider above the bar.
unsafe fn draw_button_bar(hdc: HDC, client_w: i32, client_h: i32, dpi: u32, dark: bool, strings: &Strings) {
    let (reset, cancel, save) = button_rects(client_w, client_h, dpi);

    let font_name = native_interop::wide_str("Segoe UI");
    let font = CreateFontW(
        -scale(13, dpi),
        0,
        0,
        0,
        FW_MEDIUM.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_TT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        PCWSTR::from_raw(font_name.as_ptr()),
    );
    let old_font = SelectObject(hdc, font);

    draw_button(hdc, reset, dark, strings.button_reset, false);
    draw_button(hdc, cancel, dark, strings.button_cancel, false);
    draw_button(hdc, save, dark, strings.button_save, true);

    SelectObject(hdc, old_font);
    let _ = DeleteObject(font);
}

/// Runs a button's effect. `Save` persists `draft` to disk and applies it to the real widget
/// (via `window::set_settings`, which also repaints the real widget immediately), then closes
/// the window. `Cancel` closes the window without touching disk or the real widget at all --
/// `draft` is simply dropped along with the rest of `ConfigState` in the `WM_DESTROY` handler.
/// `Reset` replaces `draft` with `Settings::default_preserving_position` of the *real widget's
/// current* settings (not `draft` -- resetting should snap all the way back to what's on disk
/// today, undoing any in-progress edits too) and rebuilds `state.controls` from it so the
/// section controls' own displayed values (each `Slider`/`RgbaPicker`/etc. holds its own copy,
/// not a live view of `draft`) stay in sync; it does not close the window or touch disk/the
/// real widget, so the user can keep editing or still Cancel afterward.
unsafe fn handle_button_action(hwnd: HWND, action: ButtonAction) {
    match action {
        ButtonAction::Save => {
            let draft = {
                let guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
                guard.as_ref().map(|s| s.draft.clone())
            };
            if let Some(mut draft) = draft {
                // Clamp against the real taskbar before persisting, so a user can't save an
                // oversized/off-taskbar geometry from the editor in the first place (see
                // `window::clamp_geometry_to_current_taskbar`'s doc comment).
                draft.geometry = window::clamp_geometry_to_current_taskbar(draft.geometry);
                crate::settings::save(&draft);
                window::set_settings(draft);
            }
            let _ = ReleaseCapture();
            let _ = DestroyWindow(hwnd);
        }
        ButtonAction::Cancel => {
            let _ = ReleaseCapture();
            let _ = DestroyWindow(hwnd);
        }
        ButtonAction::Reset => {
            let dark = theme::is_dark_mode();
            {
                let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(state) = guard.as_mut() {
                    let current = window::current_settings();
                    state.draft = Settings::default_preserving_position(current);
                    let strings = strings_for(&state.draft);
                    state.controls = SectionControls::from_settings(&state.draft, dark, &strings);
                    state.preview_clock.apply_settings(&state.draft.animation);
                    state.preview_last_tick = None;
                    state.last_synced_poll_interval_ms = state.draft.poll_interval_ms;
                }
            }
            let _ = SetTimer(hwnd, IDT_PREVIEW_ANIM, 16, None);
            let _ = InvalidateRect(hwnd, None, false);
        }
    }
}

/// Render `draft` via the same `paint_widget` the real widget uses (with a small hardcoded
/// demo `UsageData`, since this window has no live poll data) into an off-screen bitmap sized
/// to the widget's natural (unclamped) dimensions, then blit it into the top-left corner of
/// `content_rect` with a small margin. If the panel is too small to fit the natural size, the
/// preview is clamped down to whatever room is available rather than overflowing into the
/// sidebar or button bar.
unsafe fn draw_preview(hdc: HDC, content_rect: RECT, dpi: u32) {
    let (draft, frame) = {
        let guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(s) => (s.draft.clone(), s.preview_frame.clone()),
            None => return,
        }
    };
    let is_dark = theme::is_dark_mode();
    let usage = demo_usage_data();
    let strings = strings_for(&draft);

    // Natural widget size, computed with window.rs's own layout math (`sc`/
    // `active_model_count`/`total_widget_width_for`) so the preview's internal proportions
    // (which `paint_content` lays out entirely in terms of that same `sc()`) stay consistent
    // with the width/height we pass it.
    let active_models = window::active_model_count(
        usage.show_claude_code,
        usage.show_codex,
        usage.show_antigravity,
    );
    let natural_w = window::total_widget_width_for(active_models, &draft.geometry);
    let natural_h = window::sc(draft.geometry.height);

    let pad = scale(16, dpi);
    let avail_w = (content_rect.right - content_rect.left - pad * 2).max(1);
    let avail_h = (content_rect.bottom - content_rect.top - pad * 2).max(1);
    let w = natural_w.clamp(1, avail_w);
    let h = natural_h.clamp(1, avail_h);

    let mem_dc = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, w, h);
    if bmp.is_invalid() {
        let _ = DeleteDC(mem_dc);
        return;
    }
    let old_bmp = SelectObject(mem_dc, bmp);

    window::paint_widget(mem_dc, w, h, &draft, &frame, &usage, is_dark, strings);

    let _ = BitBlt(
        hdc,
        content_rect.left + pad,
        content_rect.top + pad,
        w,
        h,
        mem_dc,
        0,
        0,
        SRCCOPY,
    );

    SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(bmp);
    let _ = DeleteDC(mem_dc);
}

// --- Control layout/draw/dispatch (Task 13) ---

/// Walks top-down through a control area, handing out label+control row rects (and section
/// header rects) at consistent baseline heights, shared by every section's `*_layout`
/// function. Reconstructed fresh from `area`/`dpi` on every call (draw and dispatch alike), so
/// there's no persistent cursor state to drift out of sync between painting and hit-testing.
struct RowCursor {
    left: i32,
    right: i32,
    y: i32,
    dpi: u32,
}

impl RowCursor {
    fn new(area: RECT, dpi: u32) -> Self {
        Self {
            left: area.left,
            right: area.right,
            y: area.top,
            dpi,
        }
    }

    /// A section header row (e.g. "Fill"); returns its text rect and advances past it.
    fn header(&mut self) -> RECT {
        let h = scale(CTRL_HEADER_H, self.dpi);
        let r = RECT {
            left: self.left,
            top: self.y,
            right: self.right,
            bottom: self.y + h,
        };
        self.y += h + scale(CTRL_HEADER_GAP, self.dpi);
        r
    }

    /// A label+control row; returns `(label_rect, control_rect)` and advances past it.
    fn row(&mut self) -> (RECT, RECT) {
        let h = scale(CTRL_ROW_H, self.dpi);
        let label_w = scale(CTRL_LABEL_W, self.dpi);
        let gap = scale(CTRL_CONTROL_GAP, self.dpi);
        let label = RECT {
            left: self.left,
            top: self.y,
            right: self.left + label_w,
            bottom: self.y + h,
        };
        let control = RECT {
            left: self.left + label_w + gap,
            top: self.y,
            right: self.right,
            bottom: self.y + h,
        };
        self.y += h + scale(CTRL_ROW_GAP, self.dpi);
        (label, control)
    }

    fn group_gap(&mut self) {
        self.y += scale(CTRL_GROUP_GAP, self.dpi);
    }
}

/// Gates a dispatch call so a control only *starts* an interaction (`WM_LBUTTONDOWN`) when the
/// click actually landed within its own `rect`. This matters because `controls.rs`'s bare
/// `Slider` deliberately ignores `y` in `on_mouse` (`let _ = y;` -- ignoring `y` is what lets a
/// composite control like `RgbaPicker` keep tracking a channel slider as the cursor drifts
/// outside that channel's exact row mid-drag), trusting whatever embeds it to have already
/// confirmed the click's row. Every section here embeds several bare `Slider`s directly (not
/// through `RgbaPicker`), so without this gate a `WM_LBUTTONDOWN` anywhere in the section --
/// even far outside every control's row, e.g. still within the label column, or a stray click
/// while a section-nav switch's matching `WM_LBUTTONUP` gets routed here -- would register as a
/// hit on *any* slider whose horizontal range happens to contain that x. `WM_MOUSEMOVE`/
/// `WM_LBUTTONUP` always pass through unfiltered so an in-progress drag keeps tracking even
/// once the cursor strays outside its row (the normal, desired slider-dragging UX).
fn dispatch_hit(msg: u32, x: i32, y: i32, rect: RECT) -> bool {
    if msg != WM_LBUTTONDOWN {
        return true;
    }
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

/// Draws a left-aligned field label (e.g. "Opacity") in `rect`, matching the muted/foreground
/// text color convention used elsewhere in this window.
unsafe fn draw_field_label(hdc: HDC, rect: RECT, dark: bool, text: &str) {
    let color = if dark {
        Color::new(0xd0, 0xd0, 0xd0)
    } else {
        Color::new(0x30, 0x30, 0x30)
    };
    let _ = SetTextColor(hdc, COLORREF(color.to_colorref()));
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut r = rect;
    let _ = DrawTextW(
        hdc,
        &mut wide,
        &mut r,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

/// Extra vertical offset applied to grid row `row` (0-based: rows 0..3 are the picker grid's
/// own rows, row 3 is the "virtual" row just below the grid -- the opacity row) when picker row
/// `open_row` (if any) has its popover open, `popover_h` tall. Rows at or above `open_row` are
/// unaffected (`row <= open_row` -- notably including `open_row` itself: the open picker's own
/// closed-row rect doesn't grow, only what comes *after* it does); every row strictly below it
/// shifts down by the full popover height to make room. Reflow always moves a whole grid row
/// (both columns together, not just the column the popover opened in) so the 2-column grid
/// stays vertically aligned rather than going jagged.
///
/// Pure function, split out of `appearance_grid` so the reflow math has direct unit-test
/// coverage independent of DPI scaling / rect construction (see `appearance_row_offset_tests`
/// below).
fn appearance_row_offset(row: i32, open_row: Option<i32>, popover_h: i32) -> i32 {
    match open_row {
        Some(open_row) if row > open_row => popover_h,
        _ => 0,
    }
}

/// Which `AppearanceControls::pickers` index (if any) currently has its popover open, plus that
/// picker's current popover height (0 if none is open). Single source of truth for the "which
/// row needs reflow room" question, shared by `draw_appearance_controls` and
/// `dispatch_appearance` via `appearance_layout` so painting and hit-testing can't drift apart.
/// At most one picker is ever open at a time (enforced in `dispatch_appearance`), so the first
/// match is the only one that matters.
fn appearance_open_state(c: &AppearanceControls) -> (Option<usize>, i32) {
    match c.pickers.iter().position(|p| p.is_open()) {
        Some(i) => (Some(i), c.pickers[i].popover_height()),
        None => (None, 0),
    }
}

/// The six `RgbaPicker` cell rects (2 columns x 3 rows), each split into a label strip and a
/// picker body, plus the y just below the grid (where the opacity row starts). Order matches
/// `AppearanceControls::pickers`/`APPEARANCE_PICKER_LABELS`. `open_index`/`popover_h` (from
/// `appearance_open_state`) describe whichever picker currently has its popover open, if any:
/// every grid row below `open_index`'s row -- and the opacity row past the grid entirely --
/// shifts down by `popover_h` to make room (see `appearance_row_offset`).
fn appearance_grid(area: RECT, dpi: u32, open_index: Option<usize>, popover_h: i32) -> ([RECT; 6], [RECT; 6], i32) {
    let cols = 2;
    let col_gap = scale(RGBA_COL_GAP, dpi);
    let row_gap = scale(RGBA_ROW_GAP, dpi);
    let cell_w = ((area.right - area.left) - col_gap * (cols - 1)).max(cols) / cols;
    let label_h = scale(RGBA_LABEL_H, dpi);
    let label_gap = scale(RGBA_LABEL_GAP, dpi);
    // Closed-row height: the same single-row height every other control in this file uses (see
    // `RGBA_LABEL_H`'s doc comment above for why this replaced the old always-4-slider height).
    let picker_h = scale(CTRL_ROW_H, dpi);
    let cell_h = label_h + label_gap + picker_h;
    let open_row = open_index.map(|i| i as i32 / cols);

    let mut labels = [RECT::default(); 6];
    let mut bodies = [RECT::default(); 6];
    for i in 0..6i32 {
        let col = i % cols;
        let row = i / cols;
        let offset = appearance_row_offset(row, open_row, popover_h);
        let left = area.left + col * (cell_w + col_gap);
        let top = area.top + row * (cell_h + row_gap) + offset;
        labels[i as usize] = RECT {
            left,
            top,
            right: left + cell_w,
            bottom: top + label_h,
        };
        bodies[i as usize] = RECT {
            left,
            top: top + label_h + label_gap,
            right: left + cell_w,
            bottom: top + cell_h,
        };
    }
    let rows = 3;
    let grid_bottom =
        area.top + rows * cell_h + (rows - 1) * row_gap + appearance_row_offset(rows, open_row, popover_h);
    (labels, bodies, grid_bottom)
}

/// Full Appearance section layout: the six picker cells plus the opacity row below them.
fn appearance_layout(
    area: RECT,
    dpi: u32,
    open_index: Option<usize>,
    popover_h: i32,
) -> ([RECT; 6], [RECT; 6], RECT, RECT) {
    let (labels, bodies, grid_bottom) = appearance_grid(area, dpi, open_index, popover_h);
    let mut cursor = RowCursor {
        left: area.left,
        right: area.right,
        y: grid_bottom + scale(CTRL_GROUP_GAP, dpi),
        dpi,
    };
    let (opacity_label, opacity_control) = cursor.row();
    (labels, bodies, opacity_label, opacity_control)
}

unsafe fn draw_appearance_controls(hdc: HDC, area: RECT, dpi: u32, dark: bool, c: &AppearanceControls, strings: &Strings) {
    let (open_index, popover_h) = appearance_open_state(c);
    let (labels, bodies, opacity_label, opacity_control) = appearance_layout(area, dpi, open_index, popover_h);
    let picker_labels = appearance_picker_labels(strings);
    for i in 0..6 {
        draw_field_label(hdc, labels[i], dark, picker_labels[i]);
        // `bodies[i]` is only ever the picker's own closed row -- `RgbaPicker::draw` anchors its
        // popover to `bodies[i].bottom` itself (via `popover_rect`), so the space the reflow
        // above reserved for it is exactly where the popover lands; no taller rect needed here.
        c.pickers[i].draw(hdc, bodies[i], dark);
    }
    draw_field_label(hdc, opacity_label, dark, strings.field_opacity);
    c.opacity.draw(hdc, opacity_control, dark);
}

fn dispatch_appearance(
    c: &mut AppearanceControls,
    draft: &mut Settings,
    area: RECT,
    dpi: u32,
    msg: u32,
    x: i32,
    y: i32,
) -> bool {
    let (open_index, popover_h) = appearance_open_state(c);
    let (_labels, bodies, _opacity_label, opacity_control) = appearance_layout(area, dpi, open_index, popover_h);
    let mut changed = false;
    for i in 0..6 {
        // No `dispatch_hit` gate here: like `Dropdown` (see `dispatch_font`'s identical
        // reasoning below), `RgbaPicker` already fully self-validates every hit (`point_in`/
        // `swatch_at`/`popover_slider_row_at`), and -- unlike a bare `Slider` -- its valid area
        // legitimately extends *below* `bodies[i]` while open (the quick-swatch grid / "Custom…"
        // sliders), so gating on `bodies[i]`'s own bounds would wrongly reject a click on an
        // open popover. This also gives us "outside click closes the popover" for free: since
        // grid cells never overlap, clicking a *different* picker's row is always outside the
        // currently-open picker's own rect+popover, so it self-closes via its own `on_mouse`
        // right here in this same loop.
        if c.pickers[i].on_mouse(msg, x, y, bodies[i]).is_some() {
            changed = true;
        }
    }
    // Enforce at most one open popover at a time as a defensive backstop (normal single-click
    // interactions already end up with at most one open, per the self-close behavior noted
    // above): if more than one picker somehow ended up open, keep only the first and force-close
    // the rest. Keeps the reflow above bounded to "at most one open popover" and avoids
    // overlapping popovers.
    if let Some(open_i) = c.pickers.iter().position(|p| p.is_open()) {
        for (j, p) in c.pickers.iter_mut().enumerate() {
            if j != open_i {
                p.close();
            }
        }
    }
    if dispatch_hit(msg, x, y, opacity_control) && c.opacity.on_mouse(msg, x, y, opacity_control).is_some() {
        changed = true;
    }
    if changed {
        draft.appearance.palette = Some(PaletteStops {
            calm: c.pickers[0].value,
            attention: c.pickers[1].value,
            critical: c.pickers[2].value,
        });
        draft.appearance.background = Some(c.pickers[3].value);
        draft.appearance.text = Some(c.pickers[4].value);
        draft.appearance.divider = Some(c.pickers[5].value);
        draft.appearance.opacity = c.opacity.value.clamp(0.0, 1.0);
    }
    changed
}

fn font_layout(area: RECT, dpi: u32) -> (RECT, RECT, RECT, RECT, RECT, RECT) {
    let mut cursor = RowCursor::new(area, dpi);
    let (family_label, family_control) = cursor.row();
    let (size_label, size_control) = cursor.row();
    let (weight_label, weight_control) = cursor.row();
    (
        family_label,
        family_control,
        size_label,
        size_control,
        weight_label,
        weight_control,
    )
}

unsafe fn draw_font_controls(hdc: HDC, area: RECT, dpi: u32, dark: bool, c: &FontControls, strings: &Strings) {
    let (fl, fc, sl, sctrl, wl, wc) = font_layout(area, dpi);
    draw_field_label(hdc, fl, dark, strings.field_family);
    c.family.draw(hdc, fc, dark);
    draw_field_label(hdc, sl, dark, strings.field_size);
    c.size.draw(hdc, sctrl, dark);
    draw_field_label(hdc, wl, dark, strings.field_weight);
    c.weight.draw(hdc, wc, dark);
}

fn dispatch_font(
    c: &mut FontControls,
    draft: &mut Settings,
    area: RECT,
    dpi: u32,
    msg: u32,
    x: i32,
    y: i32,
) -> bool {
    let (_fl, fc, _sl, sctrl, _wl, wc) = font_layout(area, dpi);
    let mut changed = false;
    // No `dispatch_hit` gate here: `Dropdown` already fully self-validates (`point_in`/
    // `item_at`), and -- unlike every other control here -- its valid area legitimately
    // extends *below* `fc` while open (the dropped-down item list), so gating on `fc`'s own
    // bounds would wrongly reject a click on an open list item.
    if c.family.on_mouse(msg, x, y, fc).is_some() {
        changed = true;
        draft.typography.family = c.family.items[c.family.selected].clone();
    }
    if dispatch_hit(msg, x, y, sctrl) && c.size.on_mouse(msg, x, y, sctrl).is_some() {
        changed = true;
        draft.typography.size_pt = c.size.value;
    }
    if dispatch_hit(msg, x, y, wc) && c.weight.on_mouse(msg, x, y, wc).is_some() {
        changed = true;
        draft.typography.weight = match c.weight.selected {
            1 => Weight::SemiBold,
            2 => Weight::Bold,
            _ => Weight::Regular,
        };
    }
    changed
}

/// Row rects for the six `SizeControls` sliders, in field order: height, corner_radius,
/// bar_thickness, label_width, text_width, spacing.
fn size_layout(area: RECT, dpi: u32) -> [(RECT, RECT); 6] {
    let mut cursor = RowCursor::new(area, dpi);
    [
        cursor.row(),
        cursor.row(),
        cursor.row(),
        cursor.row(),
        cursor.row(),
        cursor.row(),
    ]
}

/// Localized (Task 18) labels for the six `SizeControls` sliders, in the same field order as
/// `size_layout`/`SizeControls` itself.
fn size_labels(strings: &Strings) -> [&'static str; 6] {
    [
        strings.geometry_height,
        strings.geometry_corner_radius,
        strings.geometry_bar_thickness,
        strings.geometry_label_width,
        strings.geometry_text_width,
        strings.geometry_spacing,
    ]
}

unsafe fn draw_size_controls(hdc: HDC, area: RECT, dpi: u32, dark: bool, c: &SizeControls, strings: &Strings) {
    let rows = size_layout(area, dpi);
    let sliders = [
        &c.height,
        &c.corner_radius,
        &c.bar_thickness,
        &c.label_width,
        &c.text_width,
        &c.spacing,
    ];
    let labels = size_labels(strings);
    for i in 0..6 {
        draw_field_label(hdc, rows[i].0, dark, labels[i]);
        sliders[i].draw(hdc, rows[i].1, dark);
    }
}

fn dispatch_size(
    c: &mut SizeControls,
    draft: &mut Settings,
    area: RECT,
    dpi: u32,
    msg: u32,
    x: i32,
    y: i32,
) -> bool {
    let rows = size_layout(area, dpi);
    let sliders = [
        &mut c.height,
        &mut c.corner_radius,
        &mut c.bar_thickness,
        &mut c.label_width,
        &mut c.text_width,
        &mut c.spacing,
    ];
    let mut changed = false;
    for i in 0..6 {
        if dispatch_hit(msg, x, y, rows[i].1) && sliders[i].on_mouse(msg, x, y, rows[i].1).is_some() {
            changed = true;
        }
    }
    if changed {
        // Rounded to i32 and floored at 1 -- Geometry's fields drive widget layout math
        // (segment widths, divider positions) that assumes strictly positive sizes.
        draft.geometry.height = (c.height.value.round() as i32).max(1);
        draft.geometry.corner_radius = (c.corner_radius.value.round() as i32).max(0);
        draft.geometry.bar_thickness = (c.bar_thickness.value.round() as i32).max(1);
        draft.geometry.label_width = (c.label_width.value.round() as i32).max(1);
        draft.geometry.text_width = (c.text_width.value.round() as i32).max(1);
        draft.geometry.spacing = (c.spacing.value.round() as i32).max(0);
    }
    changed
}

struct AnimationRects {
    fill_header: RECT,
    fill_on: (RECT, RECT),
    fill_speed: (RECT, RECT),
    shimmer_header: RECT,
    shimmer_on: (RECT, RECT),
    shimmer_speed: (RECT, RECT),
    shimmer_intensity: (RECT, RECT),
    glow_header: RECT,
    glow_on: (RECT, RECT),
    glow_threshold: (RECT, RECT),
    glow_intensity: (RECT, RECT),
    fade_header: RECT,
    fade_on: (RECT, RECT),
    fade_duration: (RECT, RECT),
    reduce_motion: (RECT, RECT),
}

fn animation_layout(area: RECT, dpi: u32) -> AnimationRects {
    let mut cursor = RowCursor::new(area, dpi);
    let fill_header = cursor.header();
    let fill_on = cursor.row();
    let fill_speed = cursor.row();
    cursor.group_gap();
    let shimmer_header = cursor.header();
    let shimmer_on = cursor.row();
    let shimmer_speed = cursor.row();
    let shimmer_intensity = cursor.row();
    cursor.group_gap();
    let glow_header = cursor.header();
    let glow_on = cursor.row();
    let glow_threshold = cursor.row();
    let glow_intensity = cursor.row();
    cursor.group_gap();
    let fade_header = cursor.header();
    let fade_on = cursor.row();
    let fade_duration = cursor.row();
    cursor.group_gap();
    let reduce_motion = cursor.row();
    AnimationRects {
        fill_header,
        fill_on,
        fill_speed,
        shimmer_header,
        shimmer_on,
        shimmer_speed,
        shimmer_intensity,
        glow_header,
        glow_on,
        glow_threshold,
        glow_intensity,
        fade_header,
        fade_on,
        fade_duration,
        reduce_motion,
    }
}

/// A group header (e.g. "Fill") within a section's controls. Currently just the muted field
/// label style -- kept as its own function (rather than calling `draw_field_label` directly at
/// each call site) so a future pass can give headers their own look (bold/larger) without
/// touching every group.
unsafe fn draw_group_header(hdc: HDC, rect: RECT, dark: bool, text: &str) {
    draw_field_label(hdc, rect, dark, text);
}

unsafe fn draw_animation_controls(hdc: HDC, area: RECT, dpi: u32, dark: bool, c: &AnimationControls, strings: &Strings) {
    let r = animation_layout(area, dpi);

    draw_group_header(hdc, r.fill_header, dark, strings.group_fill);
    draw_field_label(hdc, r.fill_on.0, dark, strings.field_on);
    c.fill_on.draw(hdc, r.fill_on.1, dark);
    draw_field_label(hdc, r.fill_speed.0, dark, strings.field_speed);
    c.fill_speed.draw(hdc, r.fill_speed.1, dark);

    draw_group_header(hdc, r.shimmer_header, dark, strings.group_shimmer);
    draw_field_label(hdc, r.shimmer_on.0, dark, strings.field_on);
    c.shimmer_on.draw(hdc, r.shimmer_on.1, dark);
    draw_field_label(hdc, r.shimmer_speed.0, dark, strings.field_speed);
    c.shimmer_speed.draw(hdc, r.shimmer_speed.1, dark);
    draw_field_label(hdc, r.shimmer_intensity.0, dark, strings.field_intensity);
    c.shimmer_intensity.draw(hdc, r.shimmer_intensity.1, dark);

    draw_group_header(hdc, r.glow_header, dark, strings.group_alert_glow);
    draw_field_label(hdc, r.glow_on.0, dark, strings.field_on);
    c.glow_on.draw(hdc, r.glow_on.1, dark);
    draw_field_label(hdc, r.glow_threshold.0, dark, strings.field_threshold);
    c.glow_threshold.draw(hdc, r.glow_threshold.1, dark);
    draw_field_label(hdc, r.glow_intensity.0, dark, strings.field_intensity);
    c.glow_intensity.draw(hdc, r.glow_intensity.1, dark);

    draw_group_header(hdc, r.fade_header, dark, strings.group_fade_slide);
    draw_field_label(hdc, r.fade_on.0, dark, strings.field_on);
    c.fade_on.draw(hdc, r.fade_on.1, dark);
    draw_field_label(hdc, r.fade_duration.0, dark, strings.field_duration);
    c.fade_duration.draw(hdc, r.fade_duration.1, dark);

    draw_field_label(hdc, r.reduce_motion.0, dark, strings.field_reduce_motion);
    c.reduce_motion.draw(hdc, r.reduce_motion.1, dark);
}

fn dispatch_animations(
    c: &mut AnimationControls,
    draft: &mut Settings,
    area: RECT,
    dpi: u32,
    msg: u32,
    x: i32,
    y: i32,
) -> bool {
    let r = animation_layout(area, dpi);
    let mut changed = false;

    if dispatch_hit(msg, x, y, r.fill_on.1) && c.fill_on.on_mouse(msg, x, y, r.fill_on.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.fill_speed.1)
        && c.fill_speed.on_mouse(msg, x, y, r.fill_speed.1).is_some()
    {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.shimmer_on.1)
        && c.shimmer_on.on_mouse(msg, x, y, r.shimmer_on.1).is_some()
    {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.shimmer_speed.1)
        && c
            .shimmer_speed
            .on_mouse(msg, x, y, r.shimmer_speed.1)
            .is_some()
    {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.shimmer_intensity.1)
        && c
            .shimmer_intensity
            .on_mouse(msg, x, y, r.shimmer_intensity.1)
            .is_some()
    {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.glow_on.1) && c.glow_on.on_mouse(msg, x, y, r.glow_on.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.glow_threshold.1)
        && c
            .glow_threshold
            .on_mouse(msg, x, y, r.glow_threshold.1)
            .is_some()
    {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.glow_intensity.1)
        && c
            .glow_intensity
            .on_mouse(msg, x, y, r.glow_intensity.1)
            .is_some()
    {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.fade_on.1) && c.fade_on.on_mouse(msg, x, y, r.fade_on.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.fade_duration.1)
        && c
            .fade_duration
            .on_mouse(msg, x, y, r.fade_duration.1)
            .is_some()
    {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.reduce_motion.1)
        && c
            .reduce_motion
            .on_mouse(msg, x, y, r.reduce_motion.1)
            .is_some()
    {
        changed = true;
    }

    if changed {
        draft.animation.fill.on = c.fill_on.on;
        draft.animation.fill.speed = c.fill_speed.value;
        draft.animation.shimmer.on = c.shimmer_on.on;
        draft.animation.shimmer.speed = c.shimmer_speed.value;
        draft.animation.shimmer.intensity = c.shimmer_intensity.value;
        draft.animation.alert_glow.on = c.glow_on.on;
        draft.animation.alert_glow.threshold = c.glow_threshold.value;
        draft.animation.alert_glow.intensity = c.glow_intensity.value;
        draft.animation.fade_slide.on = c.fade_on.on;
        draft.animation.fade_slide.duration_ms = c.fade_duration.value.round().max(1.0) as u32;
        draft.animation.reduce_motion = c.reduce_motion.on;
    }
    changed
}

fn update_layout(area: RECT, dpi: u32) -> (RECT, RECT, RECT, RECT) {
    let mut cursor = RowCursor::new(area, dpi);
    let (freq_label, freq_control) = cursor.row();
    let (custom_label, custom_control) = cursor.row();
    (freq_label, freq_control, custom_label, custom_control)
}

unsafe fn draw_update_controls(hdc: HDC, area: RECT, dpi: u32, dark: bool, c: &UpdateControls, strings: &Strings) {
    let (fl, fc, cl, cc) = update_layout(area, dpi);
    draw_field_label(hdc, fl, dark, strings.field_frequency);
    c.frequency.draw(hdc, fc, dark);
    draw_field_label(hdc, cl, dark, strings.field_custom_minutes);
    c.custom_minutes.draw(hdc, cc, dark);
}

fn dispatch_update(
    c: &mut UpdateControls,
    draft: &mut Settings,
    area: RECT,
    dpi: u32,
    msg: u32,
    x: i32,
    y: i32,
) -> bool {
    let (_fl, fc, _cl, cc) = update_layout(area, dpi);
    let mut changed = false;
    if dispatch_hit(msg, x, y, fc) && c.frequency.on_mouse(msg, x, y, fc).is_some() {
        changed = true;
        if let Some(&ms) = FREQ_PRESETS_MS.get(c.frequency.selected) {
            draft.poll_interval_ms = ms;
            c.custom_minutes.value = (ms as f32 / 60_000.0).max(1.0);
        }
        // Selecting "Custom" (index 4, past FREQ_PRESETS_MS's end) leaves poll_interval_ms
        // untouched until the custom slider itself is dragged.
    }
    if dispatch_hit(msg, x, y, cc) && c.custom_minutes.on_mouse(msg, x, y, cc).is_some() {
        changed = true;
        let minutes = c.custom_minutes.value.round().max(1.0);
        draft.poll_interval_ms = (minutes as u32).saturating_mul(60_000);
        c.frequency.selected = frequency_selection(draft.poll_interval_ms).0;
    }
    changed
}

/// Layout of the Presets section's scrollable content: the word-wrapped intro sentence, the 3
/// category-group header rects (Built-in/Code editors/Apps, matching `preset_grid_order`'s
/// order), and all 24 preset card rects (in `preset_grid_order()`'s order, i.e. `cards[i]` is
/// `preset_grid_order()[i]`'s card) -- all as *unscrolled* rects, positioned as if
/// `presets_scroll_offset` were `0`. Both `draw_presets_controls` (which subtracts the scroll
/// offset from each rect before painting) and `dispatch_presets` (which adds the scroll offset
/// to the click position before comparing) build this from the same `area`/`dpi`, the same
/// shared-layout-function discipline every other section's `*_layout` follows, so painting and
/// hit-testing can never drift apart even as the scroll offset changes independently of either.
struct PresetsLayout {
    intro: RECT,
    headers: [RECT; 3],
    cards: [RECT; 24],
    /// Full unscrolled content height, from `area.top` to the bottom of the last card/group-gap
    /// -- the basis for `presets_max_scroll_offset`.
    content_height: i32,
}

/// Presets section layout (Task 5): a word-wrapped intro sentence, then 3 category groups
/// (Built-in/Code editors/Apps, `preset_grid_order`'s order), each a small header label
/// followed by a 2-column grid of large clickable preset cards. A fixed-width gutter
/// (`PRESET_SCROLLBAR_GUTTER`) is reserved on the right of `area` for the passive scrollbar
/// indicator *unconditionally* -- subtracted from the grid's available width up front so the
/// card grid's column width is identical whether or not the indicator ends up drawn, instead of
/// jumping as content height crosses the scrollable threshold (e.g. on a window resize).
fn presets_layout(area: RECT, dpi: u32) -> PresetsLayout {
    let mut cursor = RowCursor::new(area, dpi);
    // Not `cursor.header()`: the intro is a full sentence (longer still in several non-English
    // locales) that needs to word-wrap across multiple lines, not the single-line 18px header
    // row every other section uses -- see `PRESETS_INTRO_H`.
    let intro_h = scale(PRESETS_INTRO_H, dpi);
    let intro = RECT {
        left: cursor.left,
        top: cursor.y,
        right: cursor.right,
        bottom: cursor.y + intro_h,
    };
    cursor.y += intro_h + scale(CTRL_HEADER_GAP, dpi);
    cursor.group_gap();

    let grid_right = (area.right - scale(PRESET_SCROLLBAR_GUTTER, dpi)).max(area.left);
    let cols = 2i32;
    let col_gap = scale(PRESET_CARD_GAP, dpi);
    let row_gap = scale(PRESET_CARD_GAP, dpi);
    let card_w = ((grid_right - area.left) - col_gap * (cols - 1)).max(cols) / cols;
    let card_h = scale(PRESET_CARD_H, dpi);
    let header_h = scale(CTRL_HEADER_H, dpi);
    let header_gap = scale(CTRL_HEADER_GAP, dpi);

    let order = preset_grid_order();
    let categories = [
        crate::presets::PresetCategory::Builtin,
        crate::presets::PresetCategory::Editors,
        crate::presets::PresetCategory::Apps,
    ];
    let mut headers = [RECT::default(); 3];
    let mut cards = [RECT::default(); 24];
    let mut idx = 0usize;
    for (ci, &category) in categories.iter().enumerate() {
        headers[ci] = RECT {
            left: cursor.left,
            top: cursor.y,
            right: grid_right,
            bottom: cursor.y + header_h,
        };
        cursor.y += header_h + header_gap;

        let count = order
            .iter()
            .filter(|id| crate::presets::theme_category(**id) == category)
            .count() as i32;
        let rows = (count + cols - 1) / cols;
        let grid_top = cursor.y;
        for r in 0..count {
            let col = r % cols;
            let row = r / cols;
            let left = area.left + col * (card_w + col_gap);
            let top = grid_top + row * (card_h + row_gap);
            cards[idx] = RECT {
                left,
                top,
                right: left + card_w,
                bottom: top + card_h,
            };
            idx += 1;
        }
        cursor.y = if rows > 0 {
            grid_top + rows * card_h + (rows - 1) * row_gap
        } else {
            grid_top
        };
        cursor.group_gap();
    }
    debug_assert_eq!(idx, 24, "presets_layout must place all 24 cards exactly once");

    PresetsLayout {
        intro,
        headers,
        cards,
        content_height: (cursor.y - area.top).max(0),
    }
}

/// Max valid `ConfigState::presets_scroll_offset` for the Presets section at `area`'s current
/// size/DPI: how far the content can scroll before its bottom edge reaches `area`'s own bottom
/// edge, `0` when everything already fits without scrolling. Pure function of
/// `presets_layout`'s `content_height` and `area`'s own height -- no GDI/window state -- so
/// it's directly unit-testable and is exactly what `WM_MOUSEWHEEL` clamps against.
fn presets_max_scroll_offset(area: RECT, dpi: u32) -> i32 {
    let visible_h = (area.bottom - area.top).max(0);
    (presets_layout(area, dpi).content_height - visible_h).max(0)
}

/// Clamps a candidate `presets_scroll_offset` to `[0, max_offset]`. `max_offset` itself is
/// floored at `0` first so a caller passing a stale/negative value (shouldn't happen --
/// `presets_max_scroll_offset` always returns `>= 0` -- but this keeps the function total and
/// panic-free on `i32::clamp`'s `min <= max` requirement) can't produce an inverted range.
fn clamp_scroll_offset(offset: i32, max_offset: i32) -> i32 {
    offset.clamp(0, max_offset.max(0))
}

/// Passive scrollbar-thumb geometry for the Presets section (Task 5): given the visible track's
/// height, the visible content height, and the section's full unscrolled content height,
/// returns `(thumb_top, thumb_height)` as pixel offsets from the track's own top -- pure
/// arithmetic, no GDI, so it's unit-testable without a live HDC. `thumb_height` is proportional
/// to how much of the content is visible (`visible_height / content_height * track_height`),
/// floored at `track_height / 8` so it never shrinks to an unusable sliver against a very long
/// list. `thumb_top` is proportional to how far scrolled (`scroll_offset / max_offset`) within
/// whatever track space is left over after the thumb's own height.
fn presets_scrollbar_thumb(
    track_height: i32,
    visible_height: i32,
    content_height: i32,
    scroll_offset: i32,
    max_offset: i32,
) -> (i32, i32) {
    if track_height <= 0 || content_height <= 0 {
        return (0, 0);
    }
    let min_thumb = (track_height / 8).max(1);
    let raw_h = ((visible_height as i64 * track_height as i64) / content_height as i64) as i32;
    let thumb_h = raw_h.clamp(min_thumb, track_height);
    let thumb_top = if max_offset > 0 {
        let room = (track_height - thumb_h).max(0);
        ((scroll_offset.clamp(0, max_offset) as i64 * room as i64) / max_offset as i64) as i32
    } else {
        0
    };
    (thumb_top, thumb_h)
}

/// Offsets `rect` vertically by `dy` -- used both to apply the Presets section's scroll offset
/// when drawing (`dy = -scroll_offset`, so scrolling *down* moves content *up* on screen, same
/// as every scrollable list) and, symmetrically, could be used to pre-offset rects before hit-
/// testing; `dispatch_presets` instead applies the inverse directly to the click's `y` against
/// *unscrolled* rects (see that function's own comment), but both directions are the same linear
/// relationship, just applied to opposite sides of the comparison.
fn offset_rect_y(rect: RECT, dy: i32) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top + dy,
        right: rect.right,
        bottom: rect.bottom + dy,
    }
}

/// Draws the Presets section's intro sentence, word-wrapping it across `rect` (sized by
/// `PRESETS_INTRO_H` in `presets_layout`) instead of truncating it to a single ellipsized line
/// like `draw_field_label` does for the short one-word/one-phrase labels used elsewhere in this
/// window. Deliberately a separate function rather than a `draw_field_label` flag: that helper's
/// `DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS` combination is correct for every other call
/// site (short labels in a fixed-height row) and `DT_VCENTER` is incompatible with `DT_WORDBREAK`
/// (Windows silently ignores `DT_WORDBREAK` when `DT_SINGLELINE` is set, and `DT_VCENTER`/
/// `DT_BOTTOM` are only meaningful in single-line mode), so this needed its own flag set anyway.
unsafe fn draw_presets_intro(hdc: HDC, rect: RECT, dark: bool, text: &str) {
    let color = if dark {
        Color::new(0xd0, 0xd0, 0xd0)
    } else {
        Color::new(0x30, 0x30, 0x30)
    };
    let _ = SetTextColor(hdc, COLORREF(color.to_colorref()));
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut r = rect;
    let _ = DrawTextW(hdc, &mut wide, &mut r, DT_LEFT | DT_TOP | DT_WORDBREAK);
}

/// Draws the intro line, the 3 category headers, and all 24 preset cards, highlighting
/// whichever preset (if any) `draft.animation.preset` currently records as last-applied --
/// purely a visual "this one's active" cue, not a control with its own state (clicking any
/// card, including the highlighted one, always re-applies it). Scrolls the whole block by
/// `scroll_offset` (see `offset_rect_y`) and clips all of it to `area` so scrolled-out content
/// never bleeds into the preview strip above or the sidebar/button bar beside/below it, using
/// the same `CreateRectRgn`/`SelectClipRgn` clipping pattern `window.rs` already uses for its
/// own progress-bar shimmer/fill clipping (just a plain rectangle here, not a rounded region).
/// The clip is reset to "no clip" (`HRGN::default()`), not restored from a saved prior region,
/// which is safe because this is always the last thing drawn in a `WM_PAINT` pass (see `paint`)
/// -- there's nothing drawn afterward in the same pass that a leftover clip could corrupt.
/// Also draws a passive (non-interactive) scrollbar indicator on the right edge when the
/// content overflows `area`'s height.
unsafe fn draw_presets_controls(
    hdc: HDC,
    area: RECT,
    dpi: u32,
    dark: bool,
    active: Option<PresetId>,
    strings: &Strings,
    scroll_offset: i32,
) {
    let layout = presets_layout(area, dpi);
    let order = preset_grid_order();

    let clip_rgn = CreateRectRgn(area.left, area.top, area.right, area.bottom);
    let _ = SelectClipRgn(hdc, clip_rgn);

    draw_presets_intro(
        hdc,
        offset_rect_y(layout.intro, -scroll_offset),
        dark,
        strings.presets_intro,
    );

    let category_labels = [
        strings.preset_category_builtin,
        strings.preset_category_editors,
        strings.preset_category_apps,
    ];
    for (i, &header) in layout.headers.iter().enumerate() {
        draw_group_header(hdc, offset_rect_y(header, -scroll_offset), dark, category_labels[i]);
    }

    for (i, &id) in order.iter().enumerate() {
        let primary = active == Some(id);
        let label = preset_display_name(id, strings);
        draw_button(hdc, offset_rect_y(layout.cards[i], -scroll_offset), dark, label, primary);
    }

    let visible_h = (area.bottom - area.top).max(0);
    let max_offset = presets_max_scroll_offset(area, dpi);
    if max_offset > 0 {
        let (thumb_top, thumb_h) =
            presets_scrollbar_thumb(visible_h, visible_h, layout.content_height, scroll_offset, max_offset);
        let track_left = area.right - scale(PRESET_SCROLLBAR_W, dpi);
        let thumb_color = if dark {
            Color::new(0x60, 0x60, 0x60)
        } else {
            Color::new(0xb0, 0xb0, 0xb0)
        };
        fill_rect(
            hdc,
            RECT {
                left: track_left,
                top: area.top + thumb_top,
                right: area.right,
                bottom: area.top + thumb_top + thumb_h,
            },
            &thumb_color,
        );
    }

    let _ = SelectClipRgn(hdc, HRGN::default());
    let _ = DeleteObject(clip_rgn);
}

/// Hit-tests a click against the preset card grid, accounting for the section's current scroll
/// offset: adds `state.presets_scroll_offset` to the click's `y` before comparing against
/// `presets_layout`'s *unscrolled* card rects -- the exact inverse of `draw_presets_controls`'s
/// `-scroll_offset` applied when drawing (`offset_rect_y`), so a card's clickable area always
/// matches where it was actually painted no matter how far the grid has been scrolled. This
/// direction was verified two ways: algebraically (`offset_rect_y(rect, -scroll_offset)` drawn
/// at screen `y0 = rect.top - scroll_offset`; a click at that same screen `y0` must satisfy
/// `y0 + scroll_offset == rect.top`, i.e. adding `scroll_offset` back recovers the unscrolled
/// rect -- see the `scroll_offset_directions_are_mutual_inverses` unit test below) and manually
/// against the running app (task report: clicking cards at the top, middle, and bottom of the
/// scrolled 24-card list each applied that exact card's preset, not a neighbor's).
///
/// Also gates on `(x, y)` falling within the visible `area` itself, not just on landing inside
/// *some* unscrolled card rect: `dispatch_controls` routes every click here unconditionally
/// (including ones that land in the preview strip or button bar, exactly like every other
/// section's dispatch function), and without this guard a click far outside `area` could still
/// spuriously land on a scrolled-out card once `scroll_offset` is added to its `y` -- the
/// unscrolled content is much taller than the visible area, so "effective y" can range far
/// beyond what's actually on screen.
fn dispatch_presets(state: &mut ConfigState, area: RECT, dpi: u32, msg: u32, x: i32, y: i32) -> bool {
    if msg != WM_LBUTTONDOWN {
        return false;
    }
    if x < area.left || x >= area.right || y < area.top || y >= area.bottom {
        return false;
    }
    let layout = presets_layout(area, dpi);
    let order = preset_grid_order();
    let effective_y = y + state.presets_scroll_offset;
    let Some(i) = layout
        .cards
        .iter()
        .position(|r| x >= r.left && x < r.right && effective_y >= r.top && effective_y < r.bottom)
    else {
        return false;
    };
    crate::presets::apply_preset(order[i], &mut state.draft);
    let strings = strings_for(&state.draft);
    state.controls = SectionControls::from_settings(&state.draft, theme::is_dark_mode(), &strings);
    true
}

/// Draws the active section's controls into `area`. Selects a smaller/lighter font than the
/// sidebar's (controls need to fit a lot of rows) for the duration of the call.
unsafe fn draw_section_controls(hdc: HDC, area: RECT, dpi: u32, dark: bool, section: usize, strings: &Strings) {
    let guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = guard.as_ref() else {
        return;
    };

    let _ = SetBkMode(hdc, TRANSPARENT);
    let font_name = native_interop::wide_str("Segoe UI");
    let font = CreateFontW(
        -scale(12, dpi),
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_TT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        PCWSTR::from_raw(font_name.as_ptr()),
    );
    let old_font = SelectObject(hdc, font);

    match section {
        0 => draw_appearance_controls(hdc, area, dpi, dark, &state.controls.appearance, strings),
        1 => draw_font_controls(hdc, area, dpi, dark, &state.controls.font, strings),
        2 => draw_size_controls(hdc, area, dpi, dark, &state.controls.size, strings),
        3 => draw_animation_controls(hdc, area, dpi, dark, &state.controls.animations, strings),
        4 => draw_update_controls(hdc, area, dpi, dark, &state.controls.update, strings),
        _ => draw_presets_controls(
            hdc,
            area,
            dpi,
            dark,
            state.draft.animation.preset,
            strings,
            state.presets_scroll_offset,
        ),
    }

    SelectObject(hdc, old_font);
    let _ = DeleteObject(font);
}

/// Routes a mouse message to the active section's controls and, if any control reports a
/// value change, syncs the corresponding `draft` field(s). Returns whether anything changed
/// (callers use this to decide whether to repaint/restart the preview's animation timer).
fn dispatch_controls(controls_area: RECT, dpi: u32, msg: u32, x: i32, y: i32) -> bool {
    let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = guard.as_mut() else {
        return false;
    };
    match state.active_section {
        0 => dispatch_appearance(
            &mut state.controls.appearance,
            &mut state.draft,
            controls_area,
            dpi,
            msg,
            x,
            y,
        ),
        1 => dispatch_font(
            &mut state.controls.font,
            &mut state.draft,
            controls_area,
            dpi,
            msg,
            x,
            y,
        ),
        2 => dispatch_size(
            &mut state.controls.size,
            &mut state.draft,
            controls_area,
            dpi,
            msg,
            x,
            y,
        ),
        3 => dispatch_animations(
            &mut state.controls.animations,
            &mut state.draft,
            controls_area,
            dpi,
            msg,
            x,
            y,
        ),
        4 => dispatch_update(
            &mut state.controls.update,
            &mut state.draft,
            controls_area,
            dpi,
            msg,
            x,
            y,
        ),
        5 => dispatch_presets(state, controls_area, dpi, msg, x, y),
        _ => false,
    }
}

/// After a control changed `draft`: re-applies `draft.animation` to the preview's own
/// `AnimationClock` (so e.g. toggling shimmer off actually stops it, not just on the next
/// unrelated tick), makes sure the preview's tick timer is running (a control change can
/// happen after the clock went idle -- e.g. re-enabling an effect that had settled), and
/// requests a repaint. Never touches the real widget/global settings (that's Task 14/15's
/// Save button, via `window::set_settings`).
unsafe fn on_controls_changed(hwnd: HWND) {
    {
        let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = guard.as_mut() {
            state.preview_clock.apply_settings(&state.draft.animation);
            state.preview_last_tick = None;
        }
    }
    let _ = SetTimer(hwnd, IDT_PREVIEW_ANIM, 16, None);
    let _ = InvalidateRect(hwnd, None, false);
}

/// Advance the preview's local animation clock by one tick and repaint. Mirrors
/// `window.rs`'s `render_layered`/`IDT_ANIM` dt-measurement and idle-stop pattern, but against
/// this window's own `ConfigState.preview_clock` -- entirely independent of the main widget's
/// global `ANIM` clock.
unsafe fn tick_preview(hwnd: HWND) {
    let active = {
        let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
        let Some(state) = guard.as_mut() else {
            return;
        };
        let now = Instant::now();
        let dt = match state.preview_last_tick {
            Some(prev) => now.duration_since(prev),
            None => Duration::from_millis(16),
        };
        state.preview_last_tick = Some(now);
        let usage_max = DEMO_TARGETS.iter().cloned().fold(0.0f32, f32::max);
        let (frame, active) = state.preview_clock.tick(dt, usage_max);
        state.preview_frame = frame;
        active
    };

    let _ = InvalidateRect(hwnd, None, false);

    // The clock has nothing left to animate (fill settled, no shimmer/glow pulsing, fade
    // complete): stop the timer so idle CPU returns to ~0% instead of repainting every 16ms
    // forever, and clear the last-tick timestamp so a future kick (e.g. Task 13 changing
    // `draft.animation`) starts clean.
    if !active {
        let _ = KillTimer(hwnd, IDT_PREVIEW_ANIM);
        if let Some(state) = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            state.preview_last_tick = None;
        }
    }
}

/// Pulls a frequency change made via the main widget's right-click menu (`window.rs`'s
/// `IDM_FREQ_*` handler, which updates `AppState` and -- via `save_state_settings` --
/// `window::current_settings()`/disk) into this window's Update section, for when the settings
/// window was already open at the time of that menu change (Task 17). Without this, only the
/// *next* window-open (or Reset) would pick up the new value, since `draft` is otherwise just a
/// point-in-time snapshot taken when the window was created.
///
/// Only refreshes when `draft.poll_interval_ms` still equals `last_synced_poll_interval_ms`,
/// i.e. the user hasn't started editing frequency in this window since the last sync -- an
/// unsaved local edit always wins over an external change rather than being silently
/// overwritten. Called on `WM_ACTIVATE` (this window regaining focus), which is cheap and
/// avoids running a dedicated poll timer for this one field.
unsafe fn resync_external_frequency(hwnd: HWND) {
    let refreshed = {
        let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
        let Some(state) = guard.as_mut() else {
            return;
        };
        let external = window::current_settings().poll_interval_ms;
        if external == state.draft.poll_interval_ms
            || state.draft.poll_interval_ms != state.last_synced_poll_interval_ms
        {
            false
        } else {
            state.draft.poll_interval_ms = external;
            let (idx, minutes) = frequency_selection(external);
            state.controls.update.frequency.selected = idx;
            state.controls.update.custom_minutes.value = minutes;
            state.last_synced_poll_interval_ms = external;
            true
        }
    };
    if refreshed {
        let _ = InvalidateRect(hwnd, None, false);
    }
}

unsafe extern "system" fn config_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            // `paint` issues dozens of separate GDI calls (fills, text, per-control draws, a
            // BitBlt for the live preview). Run all of that against an off-screen bitmap sized
            // to the *current* client rect, then blit the finished frame to the real surface in
            // one shot -- otherwise each of those calls lands directly on the visible window in
            // sequence, which is the classic GDI flicker pattern (worst during the preview's
            // ~60fps grow-in/shimmer/glow animation, which invalidates the whole window on every
            // tick via `tick_preview`). Mirrors the double-buffering `draw_preview` already does
            // for its own sub-area, just lifted to cover the whole window.
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            let w = (client.right - client.left).max(1);
            let h = (client.bottom - client.top).max(1);

            let mem_dc = CreateCompatibleDC(hdc);
            let bmp = CreateCompatibleBitmap(hdc, w, h);
            if bmp.is_invalid() {
                // Off-screen surface unavailable (e.g. exhausted GDI handles): fall back to
                // painting straight to the window rather than dropping the frame entirely.
                let _ = DeleteDC(mem_dc);
                paint(hdc, hwnd);
            } else {
                let old_bmp = SelectObject(mem_dc, bmp);
                paint(mem_dc, hwnd);
                let _ = BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
                SelectObject(mem_dc, old_bmp);
                let _ = DeleteObject(bmp);
                let _ = DeleteDC(mem_dc);
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let dpi = effective_dpi(hwnd);
            // Button-bar clicks act immediately on down (matching the section-nav click below)
            // and are handled before capture/dispatch since Save/Cancel destroy the window.
            let (reset_rect, cancel_rect, save_rect) = button_rects_for(hwnd);
            if let Some(action) = button_at(x, y, reset_rect, cancel_rect, save_rect) {
                handle_button_action(hwnd, action);
                return LRESULT(0);
            }
            // Capture the mouse for the duration of the click/drag so WM_MOUSEMOVE keeps
            // arriving (and WM_LBUTTONUP is guaranteed to arrive) even if the cursor leaves
            // the client area mid-drag -- harmless to call even for a plain section-nav click
            // or a click that hits no control at all (ReleaseCapture on WM_LBUTTONUP is just
            // as unconditional).
            let _ = SetCapture(hwnd);
            if let Some(idx) = section_at(x, y, dpi) {
                let changed = {
                    let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
                    match guard.as_mut() {
                        Some(state) if state.active_section != idx => {
                            state.active_section = idx;
                            // Force-close the Font section's family dropdown, and any open
                            // Appearance `RgbaPicker` popover, so navigating away while one is
                            // open (the only WM_LBUTTONDOWN path that bypasses dispatch_controls,
                            // which is what normally closes either one on an outside click) never
                            // leaves it rendering pre-expanded on return.
                            state.controls.font.family.close();
                            for picker in &mut state.controls.appearance.pickers {
                                picker.close();
                            }
                            true
                        }
                        _ => false,
                    }
                };
                if changed {
                    let _ = InvalidateRect(hwnd, None, false);
                }
                return LRESULT(0);
            }
            let (content_rect, dpi) = content_rect_for(hwnd);
            let (_preview_rect, controls_area) = split_content(content_rect, dpi);
            if dispatch_controls(controls_area, dpi, msg, x, y) {
                on_controls_changed(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let (content_rect, dpi) = content_rect_for(hwnd);
            let (_preview_rect, controls_area) = split_content(content_rect, dpi);
            // Broadcast unconditionally: only a control mid-drag (Slider/RgbaPicker's own
            // internal `dragging`/`active` state) reacts to WM_MOUSEMOVE at all -- plain hover
            // is a no-op for every control type here (Dropdown/Segmented/Toggle only handle
            // WM_LBUTTONDOWN), so this is cheap even at full mouse-move rates.
            if dispatch_controls(controls_area, dpi, msg, x, y) {
                on_controls_changed(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let (content_rect, dpi) = content_rect_for(hwnd);
            let (_preview_rect, controls_area) = split_content(content_rect, dpi);
            let changed = dispatch_controls(controls_area, dpi, msg, x, y);
            let _ = ReleaseCapture();
            if changed {
                on_controls_changed(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            // Only scrolls the Presets section's card grid, and only while it's the active
            // section (index 5) -- gated purely on `active_section`, not on cursor position.
            // `lParam` carries *screen* coordinates for this message (unlike WM_LBUTTONDOWN's
            // client coordinates), so gating on cursor position would require a `ScreenToClient`
            // conversion; skipping that is a deliberate simplification, valid because the whole
            // visible content area *is* the scrollable region whenever Presets is active -- there
            // is no other scrollable sub-area on this section competing for wheel events, so
            // "cursor is somewhere over this window" and "cursor is over the scrollable content"
            // coincide in practice for this one section.
            let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
            let Some(state) = guard.as_mut() else {
                return LRESULT(0);
            };
            if state.active_section != 5 {
                return LRESULT(0);
            }
            // GET_WHEEL_DELTA_WPARAM(wParam): the signed high 16 bits of wParam's low 32 bits,
            // typically +/-120 (WHEEL_DELTA) per notch on a standard wheel, more per notch on a
            // high-resolution wheel/trackpad.
            let raw_delta = (((wparam.0 as u32) >> 16) & 0xFFFF) as i16 as i32;
            let notches = raw_delta / WHEEL_DELTA as i32;
            if notches == 0 {
                return LRESULT(0);
            }
            let dpi = effective_dpi(hwnd);
            let (content_rect, _) = content_rect_for(hwnd);
            let (_preview_rect, controls_area) = split_content(content_rect, dpi);
            let max_offset = presets_max_scroll_offset(controls_area, dpi);
            // One card row (card height + row gap) per notch, scaled to this window's live DPI
            // like every other layout constant here -- a whole visually distinct "step" per
            // notch rather than an arbitrary pixel amount. Positive `notches` (wheel rotated
            // away from the user, the traditional "scroll up/toward the top" direction) decreases
            // the offset.
            let step = scale(PRESET_SCROLL_STEP, dpi);
            let new_offset = clamp_scroll_offset(state.presets_scroll_offset - notches * step, max_offset);
            let changed = new_offset != state.presets_scroll_offset;
            state.presets_scroll_offset = new_offset;
            drop(guard);
            if changed {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // Standard DPI-change handling: lParam points at the RECT the system suggests
            // for the new monitor's DPI; resize/reposition to it so content stays crisp
            // instead of just scaled-up.
            let suggested = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                HWND::default(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            apply_dark_titlebar(hwnd, theme::is_dark_mode());
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // Low word of wParam is WA_INACTIVE (0) / WA_ACTIVE (1) / WA_CLICKACTIVE (2); only
            // the latter two (window gaining focus) should trigger a resync.
            if (wparam.0 & 0xFFFF) != 0 {
                resync_external_frequency(hwnd);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_TIMER => {
            if wparam.0 == IDT_PREVIEW_ANIM {
                tick_preview(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // Deliberately does NOT call PostQuitMessage: this window shares the process's
            // single `GetMessageW` loop (window::run) with the main widget window, so
            // posting WM_QUIT here would tear down the whole app, not just this window.
            let _ = KillTimer(hwnd, IDT_PREVIEW_ANIM);
            *CONFIG_HWND.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_selection_matches_known_presets() {
        assert_eq!(frequency_selection(60_000), (0, 1.0));
        assert_eq!(frequency_selection(300_000), (1, 5.0));
        assert_eq!(frequency_selection(900_000), (2, 15.0));
        assert_eq!(frequency_selection(3_600_000), (3, 60.0));
    }

    #[test]
    fn frequency_selection_falls_back_to_custom_for_unknown_values() {
        let (idx, minutes) = frequency_selection(450_000); // 7.5 min, not a preset
        assert_eq!(idx, 4);
        assert!((minutes - 7.5).abs() < 1e-6);
    }

    #[test]
    fn frequency_selection_floors_sub_minute_values_to_one_minute() {
        // Below 60_000ms (the minimum realistic interval) the custom-slider minutes must
        // never go below 1.0 -- the Slider's own range floor -- even if poll_interval_ms were
        // somehow smaller.
        let (idx, minutes) = frequency_selection(1_000);
        assert_eq!(idx, 4);
        assert_eq!(minutes, 1.0);
    }

    /// `RowCursor::row` must lay out non-overlapping rows with the label to the left of the
    /// control, both advancing `y` by the same amount each call (so `draw`/`dispatch` -- which
    /// each reconstruct a fresh cursor from the same `area`/`dpi` -- always agree on where a
    /// given row lives).
    #[test]
    fn row_cursor_produces_sequential_non_overlapping_rows() {
        let area = RECT {
            left: 0,
            top: 0,
            right: 300,
            bottom: 1000,
        };
        let mut cursor = RowCursor::new(area, 96);
        let (label1, control1) = cursor.row();
        let (label2, control2) = cursor.row();

        assert_eq!(label1.left, area.left);
        assert!(control1.left > label1.right); // control sits to the right of its label
        assert!(control1.right <= area.right);
        assert!(label2.top >= label1.bottom); // second row starts at/after the first row's end
        assert_eq!(label2.left, label1.left);
        assert_eq!(control2.left, control1.left);
    }

    #[test]
    fn appearance_grid_produces_six_non_overlapping_cells_within_area() {
        let area = RECT {
            left: 0,
            top: 0,
            right: 500,
            bottom: 800,
        };
        let (labels, bodies, grid_bottom) = appearance_grid(area, 96, None, 0);
        for i in 0..6 {
            assert!(labels[i].bottom <= bodies[i].top); // label sits above its picker body
            assert!(bodies[i].right <= area.right);
            assert!(bodies[i].left >= area.left);
        }
        // 2 columns: even/odd indices must be in different columns (right of the previous).
        assert!(labels[1].left > labels[0].left);
        // 3 rows: index 2 (start of the second row) must be below index 0.
        assert!(labels[2].top > labels[0].top);
        assert!(grid_bottom > area.top);
        assert!(grid_bottom <= area.bottom);
    }

    #[test]
    fn appearance_grid_reflows_rows_below_an_open_picker() {
        let area = RECT { left: 0, top: 0, right: 500, bottom: 800 };
        let popover_h = 150;
        // Open index 2: row 1, col 0 (index / 2 == 1). Rows 0 (indices 0,1) sit above it and
        // must be untouched; row 1 (indices 2,3, the open picker's own row) and row 2 (indices
        // 4,5) sit at/below it.
        let (closed_labels, closed_bodies, closed_grid_bottom) = appearance_grid(area, 96, None, 0);
        let (open_labels, open_bodies, open_grid_bottom) = appearance_grid(area, 96, Some(2), popover_h);

        // Row 0 (indices 0, 1): unaffected by an open picker in row 1.
        assert_eq!(open_labels[0].top, closed_labels[0].top);
        assert_eq!(open_labels[1].top, closed_labels[1].top);
        assert_eq!(open_bodies[0].top, closed_bodies[0].top);
        assert_eq!(open_bodies[1].top, closed_bodies[1].top);

        // Row 1 (indices 2, 3, the open picker's own row): also unaffected -- the open picker's
        // own closed-row rect doesn't grow, only what comes after it does.
        assert_eq!(open_labels[2].top, closed_labels[2].top);
        assert_eq!(open_labels[3].top, closed_labels[3].top);

        // Row 2 (indices 4, 5) and the opacity row past the grid: shifted down by exactly
        // `popover_h`, and both columns move together (grid stays aligned).
        assert_eq!(open_labels[4].top, closed_labels[4].top + popover_h);
        assert_eq!(open_labels[5].top, closed_labels[5].top + popover_h);
        assert_eq!(open_bodies[4].top, closed_bodies[4].top + popover_h);
        assert_eq!(open_bodies[5].top, closed_bodies[5].top + popover_h);
        assert_eq!(open_grid_bottom, closed_grid_bottom + popover_h);
    }

    #[test]
    fn appearance_row_offset_pure_math() {
        // No picker open: every row is at its base position.
        assert_eq!(appearance_row_offset(0, None, 150), 0);
        assert_eq!(appearance_row_offset(2, None, 150), 0);

        // Open row 1: rows at/above it (0, 1) are unaffected; rows below it (2, and the
        // "virtual" row 3 past the grid, e.g. the opacity row) shift by the full popover height.
        assert_eq!(appearance_row_offset(0, Some(1), 150), 0);
        assert_eq!(appearance_row_offset(1, Some(1), 150), 0);
        assert_eq!(appearance_row_offset(2, Some(1), 150), 150);
        assert_eq!(appearance_row_offset(3, Some(1), 150), 150);

        // A zero popover height (shouldn't happen in practice -- `popover_height()` is always
        // positive -- but the math should degrade gracefully) offsets by zero even below the
        // open row.
        assert_eq!(appearance_row_offset(2, Some(0), 0), 0);
    }

    // --- Presets section: scrollable grouped grid (Task 5) ---

    #[test]
    fn preset_grid_order_groups_by_category_in_builtin_editors_apps_order() {
        let order = preset_grid_order();

        // All 24 presets, each appearing exactly once. `PresetId` derives `PartialEq` but not
        // `Eq`/`Hash` (see `settings.rs`), so uniqueness is checked pairwise via `Debug` string
        // comparison rather than a `HashSet`.
        for i in 0..order.len() {
            for j in (i + 1)..order.len() {
                assert_ne!(
                    format!("{:?}", order[i]),
                    format!("{:?}", order[j]),
                    "{:?} appears more than once in preset_grid_order",
                    order[i]
                );
            }
        }

        // First 4 entries are Builtin, next 18 are Editors, last 2 are Apps -- matching
        // `theme_category`'s 4/18/2 split (see `presets.rs`'s own
        // `exactly_three_categories_and_counts_match_the_design_spec` test) and the design
        // spec's Built-in/Code editors/Apps display order.
        for id in &order[0..4] {
            assert_eq!(crate::presets::theme_category(*id), crate::presets::PresetCategory::Builtin, "{id:?}");
        }
        for id in &order[4..22] {
            assert_eq!(crate::presets::theme_category(*id), crate::presets::PresetCategory::Editors, "{id:?}");
        }
        for id in &order[22..24] {
            assert_eq!(crate::presets::theme_category(*id), crate::presets::PresetCategory::Apps, "{id:?}");
        }
    }

    #[test]
    fn preset_display_name_uses_localized_strings_for_the_original_four_and_theme_names_for_the_rest() {
        let mut settings = Settings::default();
        settings.language = Some("en".to_string());
        let strings = strings_for(&settings);
        assert_eq!(preset_display_name(PresetId::Default, &strings), strings.preset_default);
        assert_eq!(preset_display_name(PresetId::Glass, &strings), strings.preset_glass);
        assert_eq!(preset_display_name(PresetId::Neon, &strings), strings.preset_neon);
        assert_eq!(preset_display_name(PresetId::Minimal, &strings), strings.preset_minimal);
        // The 20 names added in Task 3 have no `Strings` entry; they always go through
        // `presets::theme_display_name` regardless of locale.
        assert_eq!(preset_display_name(PresetId::Dracula, &strings), "Dracula");
        assert_eq!(preset_display_name(PresetId::SolarizedDark, &strings), "Solarized Dark");
    }

    #[test]
    fn presets_layout_produces_24_non_overlapping_cards_within_area() {
        let area = RECT { left: 0, top: 0, right: 500, bottom: 5000 };
        let layout = presets_layout(area, 96);
        for r in layout.cards.iter() {
            assert!(r.left >= area.left, "{r:?}");
            assert!(r.right <= area.right, "{r:?}");
            assert!(r.right > r.left, "{r:?}");
            assert!(r.bottom > r.top, "{r:?}");
        }
        // 2 columns: the second card of a group sits to the right of the first.
        assert!(layout.cards[1].left > layout.cards[0].left);
        // Every header sits above its group's first card.
        assert!(layout.headers[0].bottom <= layout.cards[0].top);
        assert!(layout.content_height > 0);
    }

    #[test]
    fn presets_max_scroll_offset_is_zero_when_everything_fits() {
        // Absurdly tall area: all 24 cards + 3 headers + intro must fit without scrolling.
        let area = RECT { left: 0, top: 0, right: 500, bottom: 20_000 };
        assert_eq!(presets_max_scroll_offset(area, 96), 0);
    }

    #[test]
    fn presets_max_scroll_offset_matches_content_minus_visible_when_short() {
        let area = RECT { left: 0, top: 0, right: 500, bottom: 300 };
        let content_h = presets_layout(area, 96).content_height;
        let visible_h = area.bottom - area.top;
        assert!(content_h > visible_h, "fixture area must actually need scrolling");
        assert_eq!(presets_max_scroll_offset(area, 96), content_h - visible_h);
    }

    #[test]
    fn clamp_scroll_offset_clamps_to_0_and_max() {
        assert_eq!(clamp_scroll_offset(-50, 200), 0);
        assert_eq!(clamp_scroll_offset(500, 200), 200);
        assert_eq!(clamp_scroll_offset(100, 200), 100);
        // A degenerate (shouldn't-happen) negative max_offset still yields a valid, non-panicking
        // clamp to 0.
        assert_eq!(clamp_scroll_offset(50, -10), 0);
    }

    /// The highest-risk correctness point in this task per the brief: drawing offsets a card's
    /// unscrolled rect by `-scroll_offset` (`offset_rect_y`), while `dispatch_presets` adds
    /// `scroll_offset` back to the click's `y` before comparing against the same unscrolled
    /// rect. These two must be exact inverses, or a click would resolve to the wrong card
    /// whenever the grid is scrolled. Proven here algebraically for an arbitrary unscrolled rect
    /// and scroll offset (see `dispatch_presets`'s doc comment for the manual verification of
    /// the same property against the real running app).
    #[test]
    fn scroll_offset_directions_are_mutual_inverses() {
        let unscrolled = RECT { left: 0, top: 500, right: 100, bottom: 556 };
        for scroll_offset in [0, 1, 137, 4000] {
            let drawn = offset_rect_y(unscrolled, -scroll_offset);
            // A click landing anywhere inside the drawn (scrolled) card...
            for on_screen_y in [drawn.top, drawn.top + 10, drawn.bottom - 1] {
                let effective_y = on_screen_y + scroll_offset;
                // ...must map back inside the original unscrolled rect.
                assert!(
                    effective_y >= unscrolled.top && effective_y < unscrolled.bottom,
                    "scroll_offset={scroll_offset} on_screen_y={on_screen_y} effective_y={effective_y} \
                     unscrolled={unscrolled:?}"
                );
            }
        }
    }

    #[test]
    fn presets_scrollbar_thumb_reaches_track_top_and_bottom_at_scroll_extremes() {
        let (top_at_min, h_min) = presets_scrollbar_thumb(400, 400, 1200, 0, 800);
        assert_eq!(top_at_min, 0);
        let (top_at_max, h_max) = presets_scrollbar_thumb(400, 400, 1200, 800, 800);
        assert_eq!(h_max, h_min, "thumb height shouldn't depend on scroll position");
        assert_eq!(top_at_max, 400 - h_max, "thumb should reach the track's bottom at max scroll");
    }

    #[test]
    fn presets_scrollbar_thumb_is_proportional_to_visible_fraction() {
        // Half the content visible -> thumb roughly half the track height.
        let (_, h) = presets_scrollbar_thumb(400, 400, 800, 0, 400);
        assert!((h - 200).abs() <= 1, "h={h}");
    }

    #[test]
    fn presets_scrollbar_thumb_has_a_floor_height_for_very_long_content() {
        let (_, h) = presets_scrollbar_thumb(400, 400, 1_000_000, 0, 999_600);
        assert!(h >= 400 / 8, "h={h} must not shrink below the floor");
    }

    #[test]
    fn presets_scrollbar_thumb_degrades_gracefully_for_degenerate_inputs() {
        assert_eq!(presets_scrollbar_thumb(0, 0, 1000, 0, 500), (0, 0));
        assert_eq!(presets_scrollbar_thumb(400, 400, 0, 0, 0), (0, 0));
    }

    /// Minimal `ConfigState` for exercising `dispatch_presets` directly, without going through
    /// `open_config_window` (which needs a live `HWND`). `dispatch_presets` itself only reads
    /// `draft`/`presets_scroll_offset` and, on a hit, overwrites `controls` wholesale via
    /// `SectionControls::from_settings` (exactly mirroring `open_config_window`'s own
    /// construction) -- it never reads `preview_clock`/`preview_frame`/`preview_last_tick`/
    /// `active_section`, so those only need to be *some* validly-typed value. `dark` is hardcoded
    /// to `false` (rather than `theme::is_dark_mode()`, a live registry read) to keep the test
    /// hermetic and deterministic regardless of the host machine's theme.
    fn test_config_state(draft: Settings, presets_scroll_offset: i32) -> ConfigState {
        let strings = strings_for(&draft);
        let controls = SectionControls::from_settings(&draft, false, &strings);
        let preview_clock = AnimationClock::new(&draft.animation);
        let last_synced_poll_interval_ms = draft.poll_interval_ms;
        ConfigState {
            draft,
            active_section: 5, // Presets section index (see draw_section_controls's `_ =>` arm).
            preview_clock,
            preview_frame: AnimationFrame::default(),
            preview_last_tick: None,
            controls,
            last_synced_poll_interval_ms,
            presets_scroll_offset,
        }
    }

    /// End-to-end regression test for the finding the mutual-inverses unit test above doesn't
    /// cover: it proves the algebraic relationship, but never actually calls `presets_layout`,
    /// `preset_grid_order`, or `dispatch_presets` together. This test ties them together the way
    /// the real app does: compute a card's true on-screen position the same way
    /// `draw_presets_controls` does (`presets_layout` + `offset_rect_y(.., -scroll_offset)`), then
    /// feed that exact on-screen point into `dispatch_presets` and assert the *correct* preset
    /// (via `draft.animation.preset`, exactly what `apply_preset` sets) was applied -- not a
    /// neighbor, and not what a sign-flipped/omitted scroll-offset would have resolved to.
    #[test]
    fn dispatch_presets_hit_test_matches_the_actual_on_screen_draw_position() {
        // Short area: presets_max_scroll_offset_matches_content_minus_visible_when_short (above)
        // already establishes this fixture requires real scrolling to reach every card.
        let area = RECT { left: 0, top: 0, right: 500, bottom: 300 };
        let dpi = 96;
        let layout = presets_layout(area, dpi);
        let order = preset_grid_order();

        // --- Case 1: scroll_offset = 0, a card near the top (sanity baseline). ---
        {
            let card = layout.cards[0];
            let onscreen = offset_rect_y(card, 0); // -scroll_offset with scroll_offset = 0
            let x = (onscreen.left + onscreen.right) / 2;
            let y = (onscreen.top + onscreen.bottom) / 2;

            let mut state = test_config_state(Settings::default(), 0);
            let handled = dispatch_presets(&mut state, area, dpi, WM_LBUTTONDOWN, x, y);
            assert!(handled, "click on card 0's on-screen position must be handled");
            assert_eq!(
                state.draft.animation.preset,
                Some(order[0]),
                "clicking card 0 at scroll_offset=0 must apply card 0's own preset"
            );
        }

        // --- Case 2: a card only reachable/visible at a non-zero scroll offset -- the case that
        // actually exercises the scroll math, using the real clamp helper (not a made-up value).
        {
            let max_offset = presets_max_scroll_offset(area, dpi);
            assert!(max_offset > 0, "fixture must actually require scrolling");
            let scroll_offset = clamp_scroll_offset(max_offset, max_offset);

            // Find a card that (a) is NOT visible at scroll_offset = 0 (its unscrolled top is
            // below the visible area, i.e. it's genuinely only reachable by scrolling) and (b) IS
            // fully visible once scrolled all the way down by `scroll_offset`.
            let (target_idx, onscreen) = layout
                .cards
                .iter()
                .enumerate()
                .find_map(|(i, &card)| {
                    let not_visible_unscrolled = card.top >= area.bottom;
                    let onscreen = offset_rect_y(card, -scroll_offset);
                    let visible_scrolled =
                        onscreen.top >= area.top && onscreen.bottom <= area.bottom;
                    (not_visible_unscrolled && visible_scrolled).then_some((i, onscreen))
                })
                .expect("fixture must contain a card only reachable by scrolling");

            let x = (onscreen.left + onscreen.right) / 2;
            let y = (onscreen.top + onscreen.bottom) / 2;

            // What a *broken* hit-test (scroll offset ignored entirely) would have resolved to
            // at this same on-screen click point, against the same unscrolled card rects -- used
            // below to prove this test would actually catch that regression, not just happen to
            // pass by coincidence.
            let naive_hit_ignoring_scroll = layout
                .cards
                .iter()
                .position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
                .map(|i| order[i]);

            let mut state = test_config_state(Settings::default(), scroll_offset);
            let handled = dispatch_presets(&mut state, area, dpi, WM_LBUTTONDOWN, x, y);
            assert!(
                handled,
                "click on card {target_idx}'s real on-screen position (scroll_offset={scroll_offset}) must be handled"
            );
            assert_eq!(
                state.draft.animation.preset,
                Some(order[target_idx]),
                "clicking card {target_idx} at its true scrolled on-screen position must apply \
                 that card's own preset, not a neighbor's"
            );
            // Guards against a sign-flip/omitted-offset regression: with scroll ignored, this
            // same screen point either hits nothing (still visible-region-shaped but the wrong
            // rect) or a *different* card than the one actually drawn there.
            assert_ne!(
                state.draft.animation.preset,
                naive_hit_ignoring_scroll,
                "this click must NOT resolve to whatever a scroll-ignoring hit-test would have \
                 picked -- if it does, the scroll offset isn't actually being applied"
            );
        }
    }
}
