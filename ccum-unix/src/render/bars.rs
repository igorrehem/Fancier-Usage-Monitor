//! The usage-bar widget: labeled rows of small pill-shaped segments per active model
//! (Claude Code / Codex / Antigravity), with fill/shimmer/glow/fade animation support.
//!
//! This module is a genuine PORT of `ccum-windows/src/window.rs`'s
//! `paint_widget` -> `paint_content` -> `draw_row` -> `draw_usage_bar` chain (plus its
//! `palette_color` helper) onto `tiny-skia`, read in full before writing a line here. The
//! animation MATH (fill easing, shimmer phase, glow pulse, fade alpha) is untouched -- it
//! lives in the shared, platform-agnostic `ccum_core::animation::AnimationClock`/
//! `AnimationFrame` and is bit-for-bit identical on both platforms. Only the drawing calls
//! change: GDI's `FillRect`/`CreateRoundRectRgn`+`SelectClipRgn`/`DrawTextW` become
//! `Canvas::fill_rounded_rect`/`Canvas::fill_rect_clipped_to_rounded_rect`/
//! `TextRenderer::draw_text`.
//!
//! Faithful-adaptation notes (things that differ from `window.rs` on purpose):
//! - `window.rs`'s local (non-`ccum_core`) `UsageData` struct -- pct + display text per
//!   model/section, plus `show_*` flags -- is re-declared here rather than imported, because
//!   `ccum_core::models::UsageData` (checked directly, see `ccum-core/src/models.rs`) only
//!   carries `{session, weekly}` `UsageSection`s (percentage + reset time) for a *single*
//!   app -- no display text, no cross-model `show_*` visibility flags, no notion of "the
//!   three models side by side". That shape is real-poller-result data (`AppUsageData`
//!   wraps three of them); it isn't the widget's own per-frame render input, which is what
//!   `window.rs`'s local `UsageData` (and this module's copy of it) is for. Task 8 (real
//!   polling integration) is what will build one of these from `ccum_core::poller`'s output.
//! - DPI scaling (`window.rs`'s `sc()` helper, which multiplies every layout constant by
//!   `current_dpi / 96`) is intentionally NOT ported: Task 6 never wired up DPI/scale-factor
//!   handling in `ccum-unix` (winit reports it via `Window::scale_factor`, unused so far), so
//!   every layout constant below is used at its 96-DPI-baseline value directly. Revisit once
//!   DPI-aware sizing is actually plumbed through in a later task.
//! - Per-model accent/text colors and the pixel-layout constants (`SEGMENT_W`,
//!   `LEFT_DIVIDER_W`, margins, etc.) are copied verbatim from `window.rs`'s private consts/
//!   functions -- they have no `settings::Geometry` field (never meant to be user-
//!   configurable) and `ccum-core` deliberately stays UI-toolkit-agnostic, so duplicating them
//!   here is the same shape of decision `ccum-windows` already made for itself.
//! - There is no OS dark/light-mode detection wired up in `ccum-unix` yet, so `draw_bars`
//!   takes `is_dark` as a plain argument (the demo in `main.rs` hardcodes `true`, matching
//!   Task 6's dark placeholder background) instead of querying the OS.

use ccum_core::animation::AnimationFrame;
use ccum_core::localization::LanguageId;
use ccum_core::settings::{self, Settings};

use super::text::TextRenderer;
use super::{Canvas, Color, Rect};

// --- Layout constants, ported verbatim from window.rs's private consts (see that module's
// comment: these have no settings::Geometry field, only geometry.{spacing, bar_thickness,
// corner_radius, label_width, text_width, height} are user-configurable). ---
const SEGMENT_W: f32 = 10.0;
const SEGMENT_COUNT: i32 = 10;
const LEFT_DIVIDER_W: f32 = 3.0;
const DIVIDER_RIGHT_MARGIN: f32 = 10.0;
const LABEL_RIGHT_MARGIN: f32 = 10.0;
const BAR_RIGHT_MARGIN: f32 = 4.0;
const MODEL_RIGHT_MARGIN: f32 = 3.0;

/// The widget's per-frame render input: one pct + display-text pair per model/section, plus
/// which model rows are currently visible. Mirrors `window.rs`'s private `UsageData` struct
/// (see this module's doc comment for why it isn't `ccum_core::models::UsageData`).
#[derive(Clone, Debug, Default)]
pub struct UsageData {
    pub session_pct: f64,
    pub session_text: String,
    pub weekly_pct: f64,
    pub weekly_text: String,
    pub codex_session_pct: f64,
    pub codex_session_text: String,
    pub codex_weekly_pct: f64,
    pub codex_weekly_text: String,
    pub antigravity_session_pct: f64,
    pub antigravity_session_text: String,
    pub antigravity_weekly_pct: f64,
    pub antigravity_weekly_text: String,
    pub show_claude_code: bool,
    pub show_codex: bool,
    pub show_antigravity: bool,
}

/// Hardcoded placeholder usage numbers for `main.rs`'s demo widget, since real poller
/// integration is Task 8. Matches `ccum-windows/src/config_window.rs::demo_usage_data`'s
/// exact numbers (62%/34% Claude Code session/weekly) -- the established convention for
/// "plausible-looking, non-real usage data" in this codebase.
pub fn demo_usage_data() -> UsageData {
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

/// One bar slot in the fixed draw-order contract shared by `ordered_bar_targets` (feeding
/// `AnimationClock::set_targets`) and `draw_bars` (reading `AnimationFrame::fill_pcts` back
/// for drawing) -- mirrors `window.rs`'s private `BarSlot`/`ordered_bar_slots`. Both call
/// sites must agree on what index N means, or the wrong bar animates toward the wrong target.
#[derive(Clone, Copy)]
enum BarSlot {
    ClaudeSession,
    ClaudeWeekly,
    CodexSession,
    CodexWeekly,
    AntigravitySession,
    AntigravityWeekly,
}

fn ordered_bar_slots(usage: &UsageData) -> Vec<BarSlot> {
    let mut slots = Vec::with_capacity(6);
    if usage.show_claude_code {
        slots.push(BarSlot::ClaudeSession);
        slots.push(BarSlot::ClaudeWeekly);
    }
    if usage.show_codex {
        slots.push(BarSlot::CodexSession);
        slots.push(BarSlot::CodexWeekly);
    }
    if usage.show_antigravity {
        slots.push(BarSlot::AntigravitySession);
        slots.push(BarSlot::AntigravityWeekly);
    }
    slots
}

/// The animation-clock fill targets (0.0..=1.0) for `usage`'s currently visible bars, in
/// `ordered_bar_slots` order. Callers use this both to seed/update
/// `AnimationClock::set_targets` and to compute the `usage_max` argument `AnimationClock::tick`
/// needs for its alert-glow threshold check -- mirrors `window.rs`'s `ordered_bar_fracts`.
pub fn ordered_bar_targets(usage: &UsageData) -> Vec<f32> {
    ordered_bar_slots(usage)
        .into_iter()
        .map(|slot| match slot {
            BarSlot::ClaudeSession => (usage.session_pct / 100.0) as f32,
            BarSlot::ClaudeWeekly => (usage.weekly_pct / 100.0) as f32,
            BarSlot::CodexSession => (usage.codex_session_pct / 100.0) as f32,
            BarSlot::CodexWeekly => (usage.codex_weekly_pct / 100.0) as f32,
            BarSlot::AntigravitySession => (usage.antigravity_session_pct / 100.0) as f32,
            BarSlot::AntigravityWeekly => (usage.antigravity_weekly_pct / 100.0) as f32,
        })
        .collect()
}

fn active_model_count(usage: &UsageData) -> i32 {
    (usage.show_claude_code as i32 + usage.show_codex as i32 + usage.show_antigravity as i32).max(1)
}

fn row_bar_segment_count(active_models: i32) -> i32 {
    match active_models {
        1 => SEGMENT_COUNT,
        2 => 5,
        _ => 4,
    }
}

fn model_usage_width(segment_count: i32, geometry: &settings::Geometry) -> f32 {
    (SEGMENT_W + geometry.spacing as f32) * segment_count as f32 - geometry.spacing as f32
        + BAR_RIGHT_MARGIN
        + geometry.text_width as f32
}

/// The widget's natural (unclamped, unscaled) width for `usage`'s current active-model count,
/// mirroring `window.rs`'s `baseline_total_width_for` (the DPI-unscaled variant of
/// `total_widget_width_for`, since this module never applies `sc()` -- see the module doc
/// comment). `main.rs` uses this to size the demo window so the widget isn't stretched or
/// clipped.
pub fn natural_size(settings: &Settings, usage: &UsageData) -> (u32, u32) {
    let active_models = active_model_count(usage);
    let geometry = &settings.geometry;
    let bar_segments = row_bar_segment_count(active_models);
    let model_width = (SEGMENT_W + geometry.spacing as f32) * bar_segments as f32 - geometry.spacing as f32
        + BAR_RIGHT_MARGIN
        + geometry.text_width as f32;

    let width = LEFT_DIVIDER_W
        + DIVIDER_RIGHT_MARGIN
        + geometry.label_width as f32
        + LABEL_RIGHT_MARGIN
        + model_width * active_models as f32
        + MODEL_RIGHT_MARGIN * (active_models - 1) as f32
        + 1.0; // RIGHT_MARGIN

    (width.max(1.0).round() as u32, (geometry.height.max(1)) as u32)
}

// --- Per-model accent/text colors, ported verbatim from window.rs's private functions. ---

fn hex(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
    Color::from_rgba8(r, g, b, 0xFF)
}

fn rgba_to_color(c: settings::Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Per-channel linear interpolation (including alpha) from `a` toward `b`. `tiny_skia::Color`
/// has no built-in `lerp` (unlike `ccum-windows`'s own `native_interop::Color::lerp`), so this
/// is a direct port of that method's math onto `tiny-skia`'s `f32` (0.0..=1.0) channel
/// representation instead of `u8`.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    Color::from_rgba(
        lerp(a.red(), b.red()),
        lerp(a.green(), b.green()),
        lerp(a.blue(), b.blue()),
        lerp(a.alpha(), b.alpha()),
    )
    .unwrap_or(b)
}

fn claude_accent_color() -> Color {
    hex("#D97757")
}

fn codex_accent_color(is_dark: bool) -> Color {
    if is_dark { hex("#F5F5F5") } else { hex("#1F1F1F") }
}

fn antigravity_accent_color() -> Color {
    hex("#4285F4")
}

fn claude_usage_text_color(is_dark: bool) -> Color {
    if is_dark { hex("#F09A7A") } else { hex("#A94F32") }
}

fn codex_usage_text_color(is_dark: bool) -> Color {
    if is_dark { hex("#F5F5F5") } else { hex("#1F1F1F") }
}

fn antigravity_usage_text_color(is_dark: bool) -> Color {
    if is_dark { hex("#8AB4F8") } else { hex("#1967D2") }
}

/// Adaptive dark/light default colors with `appearance`'s optional overrides applied. Returns
/// `(background, text, track/divider)`. Direct port of `window.rs::derive_colors`.
fn derive_colors(appearance: &settings::Appearance, is_dark: bool) -> (Color, Color, Color) {
    let track_default = if is_dark { hex("#444444") } else { hex("#AAAAAA") };
    let text_default = if is_dark { hex("#888888") } else { hex("#404040") };
    let bg_default = if is_dark { hex("#1C1C1C") } else { hex("#F3F3F3") };

    let track = appearance.divider.map(rgba_to_color).unwrap_or(track_default);
    let text_color = appearance.text.map(rgba_to_color).unwrap_or(text_default);
    let bg_color = appearance.background.map(rgba_to_color).unwrap_or(bg_default);
    (bg_color, text_color, track)
}

/// Interpolates a bar-fill color from `stops` based on usage fraction `p` (0.0..=1.0): the
/// lower half blends calm->attention, the upper half blends attention->critical. Direct port
/// of `window.rs::palette_color`.
fn palette_color(stops: &settings::PaletteStops, p: f32) -> Color {
    let p = p.clamp(0.0, 1.0);
    let calm = rgba_to_color(stops.calm);
    let attention = rgba_to_color(stops.attention);
    let critical = rgba_to_color(stops.critical);
    if p < 0.5 {
        lerp_color(calm, attention, p * 2.0)
    } else {
        lerp_color(attention, critical, (p - 0.5) * 2.0)
    }
}

/// Point-size -> pixel-size conversion for `TextRenderer::draw_text` (which wants a pixel
/// `font_size`). Windows converts point size to logical units via GDI's
/// `nHeight = -(point_size * dpi / 72)`; at the 96-DPI baseline this module never scales past
/// (see the module doc comment), that reduces to `px = pt * 96 / 72`, reproducing
/// `settings.rs`'s documented default (9.0pt -> 12px).
fn font_px(typography: &settings::Typography) -> f32 {
    typography.size_pt * 96.0 / 72.0
}

/// Vertically centers a single line of `font_px`-sized text within a `row_height`-tall row,
/// returning the `y` to pass to `TextRenderer::draw_text` (which takes a top-left origin).
/// Approximate (cosmic-text's line-height is `font_size * 1.2`, matching `text.rs`'s own
/// `Metrics::new` call) -- GDI's `DT_VCENTER` is exact, but pixel-perfect vertical centering
/// isn't the bar here (see the Task 7 brief's "does this look like the same idea", not
/// pixel-perfect).
fn vcenter_text_y(y: f32, row_height: f32, size_px: f32) -> f32 {
    y + ((row_height - size_px * 1.2) / 2.0).max(0.0)
}

/// The four animation families this module ports, mirroring `window.rs::draw_usage_bar`'s
/// `shimmer`/`glow` parameters exactly (`None` disables the effect entirely; `Some` for glow
/// just means "some bar in the widget may be pulsing" -- the halo only actually draws once
/// *this* bar's own fraction crosses `threshold`, checked inside `draw_usage_bar` below).
struct BarEffects {
    shimmer: Option<(f32, f32)>, // (phase 0..1, intensity 0..1)
    glow: Option<(f32, f32)>,    // (threshold 0..1, intensity 0..1)
    /// Global fade-in/out alpha (0.0..=1.0) applied to every color this bar draws, blended
    /// toward `bg`. `window.rs` applies `frame.fade_alpha` as a true per-pixel alpha-channel
    /// multiply on its layered window (revealing the real taskbar behind it); `ccum-unix`'s
    /// window is an ordinary opaque surface with no transparency compositor hookup yet, so
    /// the equivalent visible effect -- content fading down to just the background -- is
    /// reproduced by lerping every drawn color toward `bg` by `fade_alpha` instead.
    fade_alpha: f32,
}

/// Draws one usage bar: `segment_count` small rounded-rect segments in a row (a "fill" of
/// discrete pills, not one continuous bar), an optional alert-glow halo behind them, an
/// optional shimmer highlight swept across the filled portion, and the percentage/label text
/// after the bar. Direct port of `window.rs::draw_usage_bar`.
#[allow(clippy::too_many_arguments)]
fn draw_usage_bar(
    canvas: &mut Canvas,
    text_renderer: &mut TextRenderer,
    bar_x: f32,
    y: f32,
    segment_count: i32,
    percent: f64,
    value_text: &str,
    accent: Color,
    track: Color,
    bg: Color,
    text_color: Color,
    geometry: &settings::Geometry,
    palette: Option<settings::PaletteStops>,
    effects: &BarEffects,
    size_px: f32,
) {
    let seg_w = SEGMENT_W;
    let seg_h = geometry.bar_thickness as f32;
    let seg_gap = geometry.spacing as f32;
    let corner_r = geometry.corner_radius as f32;
    let total_w = segment_count as f32 * (seg_w + seg_gap) - seg_gap;

    let percent_clamped = percent.clamp(0.0, 100.0);
    let segment_percent = 100.0 / segment_count as f64;

    // Per-model bar fill: if a palette override is set, interpolate the fill color by this
    // bar's overall usage fraction; otherwise keep the per-model accent passed in.
    let raw_accent = match &palette {
        Some(stops) => palette_color(stops, (percent_clamped / 100.0) as f32),
        None => accent,
    };
    // Apply the fade-in/out blend (see `BarEffects::fade_alpha`'s doc comment) to every color
    // this bar draws below.
    let accent = lerp_color(bg, raw_accent, effects.fade_alpha);
    let track = lerp_color(bg, track, effects.fade_alpha);
    let text_color = lerp_color(bg, text_color, effects.fade_alpha);

    // --- Alert glow: a soft halo drawn BEHIND the bar so the opaque track/fill segments
    // painted below cover its interior, leaving just a subtle tinted ring peeking out around
    // the bar's edges. Only shown once THIS bar's own fraction reaches the threshold.
    if let Some((threshold, glow_intensity)) = effects.glow {
        if glow_intensity > 0.0 && (percent_clamped / 100.0) as f32 >= threshold {
            let pad = 3.0_f32;
            if let Some(halo_rect) = Rect::from_ltrb(
                bar_x - pad,
                y - pad,
                bar_x + total_w + pad,
                y + seg_h + pad,
            ) {
                // Blend mostly toward the background; only a sliver of accent shows through
                // as the "glow" -- a soft wash, not a hard colored box.
                let halo_color = lerp_color(bg, raw_accent, glow_intensity.clamp(0.0, 1.0) * 0.45);
                let halo_color = lerp_color(bg, halo_color, effects.fade_alpha);
                canvas.fill_rounded_rect(halo_rect, corner_r + pad, halo_color);
            }
        }
    }

    for i in 0..segment_count {
        let seg_x = bar_x + i as f32 * (seg_w + seg_gap);
        let seg_start = i as f64 * segment_percent;
        let seg_end = seg_start + segment_percent;

        let Some(seg_rect) = Rect::from_xywh(seg_x, y, seg_w, seg_h) else {
            continue;
        };

        if percent_clamped >= seg_end {
            canvas.fill_rounded_rect(seg_rect, corner_r, accent);
        } else if percent_clamped <= seg_start {
            canvas.fill_rounded_rect(seg_rect, corner_r, track);
        } else {
            canvas.fill_rounded_rect(seg_rect, corner_r, track);
            let fraction = ((percent_clamped - seg_start) / segment_percent) as f32;
            let fill_width = seg_w * fraction;
            if fill_width > 0.0 {
                if let Some(fill_rect) = Rect::from_xywh(seg_x, y, fill_width, seg_h) {
                    // Clipped fill (not a plain FillRect) so the segment's rounded corners
                    // survive a partial fill -- Windows' `CreateRoundRectRgn` + `SelectClipRgn`
                    // equivalent, see `Canvas::fill_rect_clipped_to_rounded_rect`'s doc comment.
                    canvas.fill_rect_clipped_to_rounded_rect(fill_rect, seg_rect, corner_r, accent);
                }
            }
        }
    }

    // --- Shimmer: a thin highlight band sweeping left-to-right across the FILLED portion
    // only, clipped to the bar's rounded outline so it never pokes past the corners. Lightens
    // toward white rather than a true alpha blend, matching window.rs's own "GDI has no cheap
    // per-pixel alpha here" tradeoff (tiny-skia *could* do true alpha, but this keeps the
    // visual output consistent with the Windows build rather than gratuitously diverging).
    if let Some((phase, shimmer_intensity)) = effects.shimmer {
        if shimmer_intensity > 0.0 {
            let fill_w = ((percent_clamped / 100.0) as f32 * total_w).round();
            if fill_w > 2.0 {
                let band_w = (total_w / 10.0).max(4.0);
                let center = bar_x + (total_w * phase).round();
                let band_left = (center - band_w / 2.0).max(bar_x);
                let band_right = (center + band_w / 2.0).min(bar_x + fill_w);
                if band_right > band_left {
                    let highlight = lerp_color(
                        raw_accent,
                        Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF),
                        shimmer_intensity.clamp(0.0, 1.0) * 0.35,
                    );
                    let highlight = lerp_color(bg, highlight, effects.fade_alpha);
                    if let (Some(band_rect), Some(full_bar_rect)) = (
                        Rect::from_xywh(band_left, y, band_right - band_left, seg_h),
                        Rect::from_xywh(bar_x, y, total_w, seg_h),
                    ) {
                        canvas.fill_rect_clipped_to_rounded_rect(
                            band_rect,
                            full_bar_rect,
                            corner_r,
                            highlight,
                        );
                    }
                }
            }
        }
    }

    let text_x = bar_x + segment_count as f32 * (seg_w + seg_gap) - seg_gap + BAR_RIGHT_MARGIN;
    text_renderer.draw_text(
        canvas,
        text_x,
        vcenter_text_y(y, seg_h, size_px),
        value_text,
        size_px,
        text_color,
    );
}

/// Draws one row (either the "session" or "weekly" window): a label, then one usage bar per
/// active model laid out left to right. Direct port of `window.rs::draw_row`.
#[allow(clippy::too_many_arguments)]
fn draw_row(
    canvas: &mut Canvas,
    text_renderer: &mut TextRenderer,
    x: f32,
    y: f32,
    is_dark: bool,
    text_color: Color,
    label: &str,
    claude_percent: f64,
    claude_text: &str,
    codex_percent: f64,
    codex_text: &str,
    antigravity_percent: f64,
    antigravity_text: &str,
    usage: &UsageData,
    track: Color,
    bg: Color,
    geometry: &settings::Geometry,
    palette: Option<settings::PaletteStops>,
    effects: &BarEffects,
    size_px: f32,
) {
    let seg_h = geometry.bar_thickness as f32;
    let active_models = active_model_count(usage);
    let segment_count = row_bar_segment_count(active_models);
    let use_model_text_colors = active_models > 1;

    let claude_value_color = if use_model_text_colors {
        claude_usage_text_color(is_dark)
    } else {
        text_color
    };
    let codex_value_color = if use_model_text_colors {
        codex_usage_text_color(is_dark)
    } else {
        text_color
    };
    let antigravity_value_color = if use_model_text_colors {
        antigravity_usage_text_color(is_dark)
    } else {
        text_color
    };

    let faded_label_color = lerp_color(bg, text_color, effects.fade_alpha);
    text_renderer.draw_text(
        canvas,
        x,
        vcenter_text_y(y, seg_h, size_px),
        label,
        size_px,
        faded_label_color,
    );

    let mut model_x = x + geometry.label_width as f32 + LABEL_RIGHT_MARGIN;
    if usage.show_claude_code {
        draw_usage_bar(
            canvas,
            text_renderer,
            model_x,
            y,
            segment_count,
            claude_percent,
            claude_text,
            claude_accent_color(),
            track,
            bg,
            claude_value_color,
            geometry,
            palette,
            effects,
            size_px,
        );
        model_x += model_usage_width(segment_count, geometry) + MODEL_RIGHT_MARGIN;
    }
    if usage.show_codex {
        draw_usage_bar(
            canvas,
            text_renderer,
            model_x,
            y,
            segment_count,
            codex_percent,
            codex_text,
            codex_accent_color(is_dark),
            track,
            bg,
            codex_value_color,
            geometry,
            palette,
            effects,
            size_px,
        );
        model_x += model_usage_width(segment_count, geometry) + MODEL_RIGHT_MARGIN;
    }
    if usage.show_antigravity {
        draw_usage_bar(
            canvas,
            text_renderer,
            model_x,
            y,
            segment_count,
            antigravity_percent,
            antigravity_text,
            antigravity_accent_color(),
            track,
            bg,
            antigravity_value_color,
            geometry,
            palette,
            effects,
            size_px,
        );
    }
}

/// The "most urgent" percentage across every currently active model + window (session/weekly),
/// used by `draw_tray_icon` to collapse the widget's up-to-six independent bars into the tray
/// icon's single fill level. Picking the MAX (not an average) mirrors the alert-glow feature's
/// own philosophy elsewhere in this module: a glanceable tray icon should always reflect
/// whichever limit is closest to being hit, not smooth that signal away by averaging it against
/// other windows/models that still have plenty of headroom.
fn primary_pct(usage: &UsageData) -> f64 {
    let mut candidates: Vec<f64> = Vec::with_capacity(6);
    if usage.show_claude_code {
        candidates.push(usage.session_pct);
        candidates.push(usage.weekly_pct);
    }
    if usage.show_codex {
        candidates.push(usage.codex_session_pct);
        candidates.push(usage.codex_weekly_pct);
    }
    if usage.show_antigravity {
        candidates.push(usage.antigravity_session_pct);
        candidates.push(usage.antigravity_weekly_pct);
    }
    candidates.into_iter().fold(0.0f64, f64::max)
}

/// The accent color of the first active model in priority order (Claude Code, then Codex, then
/// Antigravity -- the same priority `ordered_bar_slots` draws in), used by `draw_tray_icon` when
/// no palette override is set. Mirrors `draw_usage_bar`'s own `None => accent` fallback, just
/// picking ONE model's accent for the whole icon instead of each bar getting its own.
fn primary_accent_color(usage: &UsageData, is_dark: bool) -> Color {
    if usage.show_claude_code {
        claude_accent_color()
    } else if usage.show_codex {
        codex_accent_color(is_dark)
    } else if usage.show_antigravity {
        antigravity_accent_color()
    } else {
        claude_accent_color()
    }
}

/// Draws a compact, icon-scale usage indicator onto `canvas` (expected to be small and square --
/// `tray.rs` always allocates it at `tray::ICON_SIZE`): a single vertical "fill" bar (like a
/// battery/thermometer level) inside a rounded track, rather than the widget's full
/// multi-segment bars.
///
/// Design choice (Task 8): a literal `draw_bars` reuse at a tiny canvas size was considered and
/// rejected -- `SEGMENT_W` alone is 10px, `LEFT_DIVIDER_W` + `DIVIDER_RIGHT_MARGIN` +
/// `LABEL_RIGHT_MARGIN` add another 23px of fixed chrome before a single bar segment is even
/// drawn, and `TextRenderer::draw_text`'s glyphs are illegible well before a full row (label +
/// bar + percentage text) could fit inside a ~16-22px tray icon. A single proportional fill bar
/// (the "battery level" idiom) reads clearly at icon scale and reuses this module's EXISTING
/// primitives (`Canvas::fill_rounded_rect`/`fill_rect_clipped_to_rounded_rect`) and color logic
/// (`derive_colors`/`palette_color`/per-model accent colors) verbatim -- no new drawing code, no
/// new color rules, just a different (single-number, `primary_pct`) input feeding the same
/// machinery `draw_usage_bar` already uses for its own filled-segment/track colors.
///
/// The background is left fully transparent (`Canvas::clear(Color::TRANSPARENT)`, not the
/// widget's opaque `bg` fill) because a tray/menu-bar icon is composited over the OS's own
/// taskbar/menu-bar chrome -- an opaque square icon would look like a UI bug, not a status
/// indicator.
pub fn draw_tray_icon(canvas: &mut Canvas, settings: &Settings, usage: &UsageData, is_dark: bool) {
    canvas.clear(Color::TRANSPARENT);

    let size = canvas.height() as f32;
    let margin = (size * 0.12).max(1.0);
    let radius = (size * 0.22).max(1.0);
    let Some(track_rect) = Rect::from_ltrb(margin, margin, size - margin, size - margin) else {
        return;
    };

    let (_, _, track_color) = derive_colors(&settings.appearance, is_dark);
    let pct = primary_pct(usage);
    let fill_color = match settings.appearance.palette {
        Some(stops) => palette_color(&stops, (pct / 100.0) as f32),
        None => primary_accent_color(usage, is_dark),
    };

    canvas.fill_rounded_rect(track_rect, radius, track_color);

    let fill_h = (track_rect.height() * (pct.clamp(0.0, 100.0) / 100.0) as f32).round();
    if fill_h > 0.0 {
        let fill_top = track_rect.bottom() - fill_h;
        if let Some(fill_rect) =
            Rect::from_ltrb(track_rect.left(), fill_top, track_rect.right(), track_rect.bottom())
        {
            canvas.fill_rect_clipped_to_rounded_rect(fill_rect, track_rect, radius, fill_color);
        }
    }
}

/// The widget's actual "does this look like a usage bar" render entry point: label + filled
/// bar (fill percentage from `frame.fill_pcts`, color from `settings.appearance`, shimmer/
/// glow effects if enabled) + text, for however many sections are active. Direct port of
/// `window.rs::paint_widget` + `paint_content` combined (that split existed there because
/// `paint_content` was shared between the real widget's layered-window path and the settings
/// window's live preview; `ccum-unix` doesn't have a second caller yet, so one function
/// suffices here -- can be split again if/when a settings-preview equivalent needs it).
pub fn draw_bars(
    canvas: &mut Canvas,
    text_renderer: &mut TextRenderer,
    settings: &Settings,
    frame: &AnimationFrame,
    usage: &UsageData,
    is_dark: bool,
) {
    let (bg, text_color, track) = derive_colors(&settings.appearance, is_dark);
    canvas.clear(bg);

    let height = canvas.height() as f32;

    // Shimmer/glow render params: `None` disables the effect entirely (either turned off in
    // settings, or -- for glow -- not currently pulsing because no bar is over threshold).
    let shimmer = settings
        .animation
        .shimmer
        .on
        .then_some((frame.shimmer_phase, settings.animation.shimmer.intensity));
    let glow = (frame.glow_intensity > 0.0)
        .then_some((settings.animation.alert_glow.threshold, frame.glow_intensity));
    let effects = BarEffects {
        shimmer,
        glow,
        fade_alpha: frame.fade_alpha.clamp(0.0, 1.0),
    };

    // Map the animated fill fractions back onto each bar using the SAME ordered-slot sequence
    // used to feed the clock's `set_targets`, so index N always refers to the same bar here as
    // it did when the target was set. Falls back to the raw (unanimated) percentage for any
    // slot the frame doesn't have yet.
    let slots = ordered_bar_slots(usage);
    let mut session_pct = usage.session_pct;
    let mut weekly_pct = usage.weekly_pct;
    let mut codex_session_pct = usage.codex_session_pct;
    let mut codex_weekly_pct = usage.codex_weekly_pct;
    let mut antigravity_session_pct = usage.antigravity_session_pct;
    let mut antigravity_weekly_pct = usage.antigravity_weekly_pct;
    for (i, slot) in slots.iter().enumerate() {
        let Some(&frac) = frame.fill_pcts.get(i) else {
            continue;
        };
        let pct = frac as f64 * 100.0;
        match slot {
            BarSlot::ClaudeSession => session_pct = pct,
            BarSlot::ClaudeWeekly => weekly_pct = pct,
            BarSlot::CodexSession => codex_session_pct = pct,
            BarSlot::CodexWeekly => codex_weekly_pct = pct,
            BarSlot::AntigravitySession => antigravity_session_pct = pct,
            BarSlot::AntigravityWeekly => antigravity_weekly_pct = pct,
        }
    }

    // --- Left divider (two adjacent thin vertical strokes, a light/dark pair for a subtle
    // bevel look) ---
    let divider_h = 25.0_f32;
    let divider_top = (height - divider_h) / 2.0;
    let (div_left, div_right) = if is_dark {
        (hex("#505050"), hex("#282828"))
    } else {
        (hex("#A0A0A0"), hex("#E6E6E6"))
    };
    if let Some(r) = Rect::from_xywh(0.0, divider_top, LEFT_DIVIDER_W - 1.0, divider_h) {
        canvas.fill_rect(r, lerp_color(bg, div_left, effects.fade_alpha));
    }
    if let Some(r) = Rect::from_xywh(LEFT_DIVIDER_W - 1.0, divider_top, 1.0, divider_h) {
        canvas.fill_rect(r, lerp_color(bg, div_right, effects.fade_alpha));
    }

    let content_x = LEFT_DIVIDER_W + DIVIDER_RIGHT_MARGIN;
    let bar_thickness = settings.geometry.bar_thickness as f32;
    let row2_y = height - 5.0 - bar_thickness;
    let row1_y = row2_y - 10.0 - bar_thickness;

    let strings = settings
        .language
        .as_deref()
        .and_then(LanguageId::from_code)
        .unwrap_or(LanguageId::English)
        .strings();
    let size_px = font_px(&settings.typography);
    let palette = settings.appearance.palette;

    draw_row(
        canvas,
        text_renderer,
        content_x,
        row1_y,
        is_dark,
        text_color,
        strings.session_window,
        session_pct,
        &usage.session_text,
        codex_session_pct,
        &usage.codex_session_text,
        antigravity_session_pct,
        &usage.antigravity_session_text,
        usage,
        track,
        bg,
        &settings.geometry,
        palette,
        &effects,
        size_px,
    );
    draw_row(
        canvas,
        text_renderer,
        content_x,
        row2_y,
        is_dark,
        text_color,
        strings.weekly_window,
        weekly_pct,
        &usage.weekly_text,
        codex_weekly_pct,
        &usage.codex_weekly_text,
        antigravity_weekly_pct,
        &usage.antigravity_weekly_text,
        usage,
        track,
        bg,
        &settings.geometry,
        palette,
        &effects,
        size_px,
    );
}
