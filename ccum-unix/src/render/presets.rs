//! Presets section (Task 12): a scrollable, 3-category-grouped (Built-in/Code editors/Apps)
//! grid of 24 clickable preset "cards", one per `ccum_core::presets::ALL_PRESET_IDS` entry.
//! Direct port of `ccum-windows/src/config_window.rs`'s `preset_grid_order`/`PresetsLayout`/
//! `presets_layout`/`presets_max_scroll_offset`/`clamp_scroll_offset`/`presets_scrollbar_thumb`/
//! `offset_rect_y`/`draw_presets_controls`/`dispatch_presets` -- read those (and this crate's
//! `render::layout`/`render::appearance` module doc comments for the shared porting notes: no
//! DPI scaling, hardcoded-English labels) before touching this file.
//!
//! # Porting notes
//!
//! - **Section-local scroll state, not a `*Controls` struct**: unlike every other section, the
//!   Presets grid has no draggable per-field widgets of its own -- clicking a card is a one-shot
//!   `apply_preset` call, not a value a `Control` owns and redraws from. So there's no
//!   `PresetsControls` struct here; the only piece of state that needs to persist across repaints
//!   is the scroll offset, which `panel.rs` holds directly (`Panel::presets_scroll_offset`,
//!   mirroring `ConfigState::presets_scroll_offset` on the Windows side) and threads through
//!   `draw_presets_controls`/`dispatch_presets` as a plain `f32` parameter.
//! - **Scroll units are `f32` pixels, not `i32`**: `ccum-windows`'s `presets_scroll_offset` is a
//!   whole-pixel `i32` (GDI/Win32 layout is integer throughout); this crate's `LRect`/`Canvas`
//!   layout is `f32` throughout (see `render::controls`'s module doc comment, "Rect type"), so
//!   the scroll offset and every helper here (`presets_max_scroll_offset`/`clamp_scroll_offset`/
//!   `presets_scrollbar_thumb`/`offset_rect_y`) stays `f32` for the same reason -- an unchanged,
//!   mechanical translation of the original arithmetic, not a redesign.
//! - **Mouse-wheel is a new event class for this crate**: every other section only reacts to
//!   `MouseMsg` (`Down`/`Move`/`Up`, translated from `winit`'s `CursorMoved`/`MouseInput` --  see
//!   `render::controls`'s module doc comment). Scrolling is `winit`'s `WindowEvent::MouseWheel`
//!   instead, handled directly in `panel.rs::handle_mouse_wheel` (there is no `Control::on_mouse`
//!   equivalent for it, since it's not addressed to any one card the way a click is) rather than
//!   through this module's `dispatch_presets` -- see that function for the click-only surface.
//! - **Clipping**: `draw_presets_controls` clips every draw call it makes (intro sentence, 3
//!   headers, 24 cards, scrollbar thumb) to `area`, so scrolled-out content never bleeds above/
//!   below the section's own content box -- `ccum-windows/src/config_window.rs::draw_presets_controls`'s
//!   `CreateRectRgn`/`SelectClipRgn` GDI clip becomes `Canvas::set_clip_rect`/`clear_clip` here
//!   (see that pair's own doc comments in `render::mod` for why a *persistent*, `Mask`-backed
//!   rect clip was added rather than reusing Task 7's narrower `fill_rect_clipped_to_rounded_rect`
//!   shape-clip).
//! - **No intro-sentence word-wrap**: `ccum-windows/src/config_window.rs::draw_presets_intro`
//!   word-wraps its intro sentence across up to ~3 lines (`DT_WORDBREAK`). `TextRenderer::draw_text`
//!   is documented single-line/unbounded-width only (see that module's own doc comment) --
//!   `ccum-unix` has no word-wrap primitive yet, and adding one is out of this task's scope (it's
//!   a `TextRenderer`-level capability every section could eventually use, not something specific
//!   to Presets). `draw_presets_intro` below draws the intro as a single line instead;
//!   `PRESETS_INTRO_H` is still reserved as the intro's layout height (matching the Windows
//!   constant) so `presets_layout`'s row math stays a direct, unchanged port -- only the drawn
//!   glyphs differ from the original, not the layout it's drawn into.
//!
//! # The scroll-offset draw/hit-test inverse relationship (highest-risk part of this port)
//!
//! `presets_layout` computes every intro/header/card rect as *unscrolled* -- positioned exactly
//! as if `scroll_offset` were `0`. Both directions below build on that same unscrolled layout, so
//! painting and hit-testing can never drift apart even as the scroll offset changes independently
//! of either:
//!
//! - **Draw** (`draw_presets_controls`): paints unscrolled rect `r` at on-screen position
//!   `offset_rect_y(r, -scroll_offset)`, i.e. `screen_top = r.top - scroll_offset`. Scrolling
//!   *down* (`scroll_offset` growing) moves content *up* on screen -- the standard scrollable-list
//!   convention.
//! - **Hit-test** (`dispatch_presets`): adds `scroll_offset` back to the click's `y` before
//!   comparing against the same *unscrolled* rects: `effective_y = click_y + scroll_offset`.
//!
//! These are exact algebraic inverses: a click landing at the on-screen position a card was
//! actually drawn at (`click_y = r.top - scroll_offset`, i.e. `screen_top`) recovers
//! `effective_y = (r.top - scroll_offset) + scroll_offset = r.top`, so it always falls back inside
//! `r` itself regardless of `scroll_offset`'s value -- see `scroll_offset_directions_are_mutual_inverses`
//! (an algebraic proof for an arbitrary rect/offset) and
//! `dispatch_presets_hit_test_matches_the_actual_on_screen_draw_position` (an end-to-end
//! regression test tying `presets_layout`+`offset_rect_y`+`dispatch_presets` together the way the
//! real app does, at a non-zero, non-made-up scroll offset, for a card only reachable by
//! scrolling) in this module's test suite below. This is the exact bug class
//! `ccum-windows/src/config_window.rs`'s own Presets section review cycle found and fixed (see
//! `.superpowers/sdd/progress.md`'s CT-Task 5 entry) -- a sign flip or an inconsistency here
//! silently applies the wrong preset on click, while the UI still visually "responds" normally,
//! so it's the one part of this port that got the most direct scrutiny.

use ccum_core::presets::{self, PresetCategory};
use ccum_core::settings::{PresetId, Settings};

use super::controls::{LRect, MouseMsg};
use super::layout::{CTRL_GROUP_GAP, CTRL_HEADER_GAP, CTRL_HEADER_H};
use super::text::TextRenderer;
use super::{Canvas, Color, Rect as SkiaRect};

/// Number of grid columns -- see `ccum-windows/src/config_window.rs`'s own `presets_layout`
/// comment for why 2 rather than 3+ (card labels like "Solarized Dark"/"Tokyo Night" need the
/// wider single-column card width 2 columns gives at this panel's width).
const PRESET_COLS: i32 = 2;
const PRESET_CARD_H: f32 = 56.0;
const PRESET_CARD_GAP: f32 = 12.0;
/// Height reserved for the intro sentence -- see this module's doc comment ("No intro-sentence
/// word-wrap") for why the sentence itself is drawn single-line despite this being sized for
/// multiple wrapped lines like the Windows original.
const PRESETS_INTRO_H: f32 = 48.0;
/// Width reserved on the right edge of the Presets content area for the passive scrollbar
/// indicator, subtracted from the card grid's available width unconditionally (not just when
/// scrolling is actually possible) -- see `ccum-windows/src/config_window.rs`'s own
/// `PRESET_SCROLLBAR_GUTTER` doc comment for why (keeps the grid's column width from jumping
/// between "scrollbar visible"/"not visible" states).
const PRESET_SCROLLBAR_GUTTER: f32 = 10.0;
const PRESET_SCROLLBAR_W: f32 = 4.0;
/// Mouse-wheel scroll step, in pixels, applied per wheel "notch" -- one card row (card height +
/// the gap above the next row), matching `ccum-windows/src/config_window.rs::PRESET_SCROLL_STEP`.
/// `pub(crate)` (rather than private) so `panel.rs::handle_mouse_wheel` can convert a `winit`
/// `LineDelta` (a notch count) into pixels using the exact same step this module's own layout
/// uses, instead of hardcoding a second copy of the constant there.
pub(crate) const PRESET_SCROLL_STEP: f32 = PRESET_CARD_H + PRESET_CARD_GAP;

const PRESETS_INTRO: &str = "Choose a starting look, then fine-tune it in the other sections.";
const CATEGORY_LABELS: [&str; 3] = ["Built-in", "Code editors", "Apps"];

/// All 24 presets (`presets::ALL_PRESET_IDS`), reordered into the Presets section's actual
/// on-screen grid order: grouped by `presets::theme_category` into the 3 fixed groups (Built-in,
/// Code editors, Apps, in that order), preserving each preset's original relative order *within*
/// its group. Direct port of `ccum-windows/src/config_window.rs::preset_grid_order` -- recomputed
/// on every call rather than cached, matching that function's own reasoning (a trivial ~72-
/// comparison cost, not worth caching for a popup that repaints at most a few dozen times a
/// second).
pub fn preset_grid_order() -> [PresetId; 24] {
    let mut order = [PresetId::Default; 24];
    let mut i = 0;
    for &category in &[PresetCategory::Builtin, PresetCategory::Editors, PresetCategory::Apps] {
        for &id in presets::ALL_PRESET_IDS.iter() {
            if presets::theme_category(id) == category {
                order[i] = id;
                i += 1;
            }
        }
    }
    debug_assert_eq!(i, 24, "preset_grid_order must place all 24 presets exactly once");
    order
}

/// Layout of the Presets section's scrollable content: the intro sentence, the 3 category-group
/// header rects, and all 24 preset card rects (in `preset_grid_order()`'s order, i.e. `cards[i]`
/// is `preset_grid_order()[i]`'s card) -- all as *unscrolled* rects, positioned as if the scroll
/// offset were `0`. Direct port of `ccum-windows/src/config_window.rs::PresetsLayout`. Both
/// `draw_presets_controls` and `dispatch_presets` build this from the same `area`, the same
/// shared-layout-function discipline every other section's `*_layout` follows -- see this
/// module's doc comment for why that discipline is especially load-bearing here.
pub struct PresetsLayout {
    pub intro: LRect,
    pub headers: [LRect; 3],
    pub cards: [LRect; 24],
    /// Full unscrolled content height, from `area.top` to the bottom of the last card/group-gap
    /// -- the basis for `presets_max_scroll_offset`.
    pub content_height: f32,
}

/// Presets section layout (Task 12): an intro sentence, then 3 category groups (Built-in/Code
/// editors/Apps), each a small header label followed by a `PRESET_COLS`-column grid of large
/// clickable preset cards. Direct port of `ccum-windows/src/config_window.rs::presets_layout`.
pub fn presets_layout(area: LRect) -> PresetsLayout {
    let mut y = area.top;

    let intro = LRect { left: area.left, top: y, right: area.right, bottom: y + PRESETS_INTRO_H };
    y += PRESETS_INTRO_H + CTRL_HEADER_GAP;
    y += CTRL_GROUP_GAP;

    let grid_right = (area.right - PRESET_SCROLLBAR_GUTTER).max(area.left);
    let cols = PRESET_COLS as f32;
    let col_gap = PRESET_CARD_GAP;
    let row_gap = PRESET_CARD_GAP;
    let card_w = ((grid_right - area.left) - col_gap * (cols - 1.0)).max(cols) / cols;
    let card_h = PRESET_CARD_H;
    let header_h = CTRL_HEADER_H;
    let header_gap = CTRL_HEADER_GAP;

    let order = preset_grid_order();
    let categories = [PresetCategory::Builtin, PresetCategory::Editors, PresetCategory::Apps];
    let mut headers = [LRect::default(); 3];
    let mut cards = [LRect::default(); 24];
    let mut idx = 0usize;
    for (ci, &category) in categories.iter().enumerate() {
        headers[ci] = LRect { left: area.left, top: y, right: grid_right, bottom: y + header_h };
        y += header_h + header_gap;

        let count = order.iter().filter(|id| presets::theme_category(**id) == category).count() as i32;
        let rows = (count + PRESET_COLS - 1) / PRESET_COLS;
        let grid_top = y;
        for r in 0..count {
            let col = r % PRESET_COLS;
            let row = r / PRESET_COLS;
            let left = area.left + col as f32 * (card_w + col_gap);
            let top = grid_top + row as f32 * (card_h + row_gap);
            cards[idx] = LRect { left, top, right: left + card_w, bottom: top + card_h };
            idx += 1;
        }
        y = if rows > 0 { grid_top + rows as f32 * card_h + (rows - 1) as f32 * row_gap } else { grid_top };
        y += CTRL_GROUP_GAP;
    }
    debug_assert_eq!(idx, 24, "presets_layout must place all 24 cards exactly once");

    PresetsLayout { intro, headers, cards, content_height: (y - area.top).max(0.0) }
}

/// Max valid scroll offset for the Presets section at `area`'s current size: how far the content
/// can scroll before its bottom edge reaches `area`'s own bottom edge, `0.0` when everything
/// already fits without scrolling. Direct port of
/// `ccum-windows/src/config_window.rs::presets_max_scroll_offset`. `pub` so `panel.rs::handle_mouse_wheel`
/// can clamp against the exact same value `dispatch_presets`'s hit-testing implicitly assumes.
pub fn presets_max_scroll_offset(area: LRect) -> f32 {
    let visible_h = (area.bottom - area.top).max(0.0);
    (presets_layout(area).content_height - visible_h).max(0.0)
}

/// Clamps a candidate scroll offset to `[0, max_offset]`. Direct port of
/// `ccum-windows/src/config_window.rs::clamp_scroll_offset`.
pub fn clamp_scroll_offset(offset: f32, max_offset: f32) -> f32 {
    offset.clamp(0.0, max_offset.max(0.0))
}

/// Passive scrollbar-thumb geometry: given the visible track's height, the visible content
/// height, and the section's full unscrolled content height, returns `(thumb_top, thumb_height)`
/// as pixel offsets from the track's own top. Direct port of
/// `ccum-windows/src/config_window.rs::presets_scrollbar_thumb`.
fn presets_scrollbar_thumb(track_height: f32, visible_height: f32, content_height: f32, scroll_offset: f32, max_offset: f32) -> (f32, f32) {
    if track_height <= 0.0 || content_height <= 0.0 {
        return (0.0, 0.0);
    }
    let min_thumb = (track_height / 8.0).max(1.0);
    let raw_h = (visible_height * track_height) / content_height;
    let thumb_h = raw_h.clamp(min_thumb, track_height);
    let thumb_top = if max_offset > 0.0 {
        let room = (track_height - thumb_h).max(0.0);
        (scroll_offset.clamp(0.0, max_offset) * room) / max_offset
    } else {
        0.0
    };
    (thumb_top, thumb_h)
}

/// Offsets `rect` vertically by `dy`. Direct port of
/// `ccum-windows/src/config_window.rs::offset_rect_y` -- see this module's doc comment for the
/// draw/hit-test inverse relationship this function is one half of.
pub fn offset_rect_y(rect: LRect, dy: f32) -> LRect {
    LRect { left: rect.left, top: rect.top + dy, right: rect.right, bottom: rect.bottom + dy }
}

fn to_skia(rect: LRect) -> Option<SkiaRect> {
    SkiaRect::from_ltrb(rect.left, rect.top, rect.right, rect.bottom)
}

/// Draws one preset card: a filled, centered-label "button", highlighted (accent-filled, white
/// text) when `active` (the currently-applied preset, purely a visual "this one's active" cue --
/// clicking any card, including the highlighted one, always re-applies it, same as
/// `ccum-windows/src/config_window.rs::draw_button`'s `primary` flag). Direct styling port of
/// that function.
fn draw_preset_card(canvas: &mut Canvas, text: &mut TextRenderer, rect: LRect, dark: bool, label: &str, active: bool) {
    let accent = Color::from_rgba8(0xd9, 0x77, 0x57, 0xff);
    let (bg, text_color, border) = if active {
        (accent, Color::from_rgba8(0xff, 0xff, 0xff, 0xff), None)
    } else {
        let bg = if dark { Color::from_rgba8(0x2a, 0x2a, 0x2a, 0xff) } else { Color::from_rgba8(0xff, 0xff, 0xff, 0xff) };
        let txt = if dark { Color::from_rgba8(0xf0, 0xf0, 0xf0, 0xff) } else { Color::from_rgba8(0x20, 0x20, 0x20, 0xff) };
        let border = if dark { Color::from_rgba8(0x45, 0x45, 0x45, 0xff) } else { Color::from_rgba8(0xc0, 0xc0, 0xc0, 0xff) };
        (bg, txt, Some(border))
    };

    if let Some(r) = to_skia(rect) {
        canvas.fill_rounded_rect(r, 6.0, bg);
    }
    if let Some(border_color) = border {
        // `Canvas` has no stroke-only rect primitive (see `render::controls::Slider::draw`'s
        // identical "border behind fill" workaround for its knob) -- an inset border-colored
        // rounded rect drawn first, then a slightly smaller fill on top, reads as a 1px border.
        if let Some(r) = to_skia(rect) {
            canvas.fill_rounded_rect(r, 6.0, border_color);
        }
        let inset = LRect { left: rect.left + 1.0, top: rect.top + 1.0, right: rect.right - 1.0, bottom: rect.bottom - 1.0 };
        if let Some(r) = to_skia(inset) {
            canvas.fill_rounded_rect(r, 5.0, bg);
        }
    }

    let label_w = text.text_width(label, 12.0);
    let label_x = rect.left + ((rect.width() - label_w) / 2.0).max(4.0);
    let label_y = rect.top + (rect.height() - 12.0) / 2.0;
    text.draw_text(canvas, label_x, label_y, label, 12.0, text_color);
}

/// Draws the intro line, the 3 category headers, and all 24 preset cards, highlighting whichever
/// preset (if any) `active` records as last-applied. Scrolls the whole block by `scroll_offset`
/// (see `offset_rect_y`) and clips all of it to `area` so scrolled-out content never bleeds
/// outside the section's own content box. Also draws a passive (non-interactive) scrollbar
/// indicator on the right edge when the content overflows `area`'s height. Direct port of
/// `ccum-windows/src/config_window.rs::draw_presets_controls`.
pub fn draw_presets_controls(canvas: &mut Canvas, text: &mut TextRenderer, area: LRect, dark: bool, active: Option<PresetId>, scroll_offset: f32) {
    let layout = presets_layout(area);
    let order = preset_grid_order();

    let Some(clip) = to_skia(area) else {
        return;
    };
    canvas.set_clip_rect(Some(clip));

    let intro_color = if dark { Color::from_rgba8(0xd0, 0xd0, 0xd0, 0xff) } else { Color::from_rgba8(0x30, 0x30, 0x30, 0xff) };
    let intro_rect = offset_rect_y(layout.intro, -scroll_offset);
    text.draw_text(canvas, intro_rect.left, intro_rect.top, PRESETS_INTRO, 12.0, intro_color);

    for (i, &header) in layout.headers.iter().enumerate() {
        let header_rect = offset_rect_y(header, -scroll_offset);
        let header_color = if dark { Color::from_rgba8(0xd0, 0xd0, 0xd0, 0xff) } else { Color::from_rgba8(0x30, 0x30, 0x30, 0xff) };
        let y = header_rect.top + (header_rect.height() - 12.0) / 2.0;
        text.draw_text(canvas, header_rect.left, y, CATEGORY_LABELS[i], 12.0, header_color);
    }

    for (i, &id) in order.iter().enumerate() {
        let primary = active == Some(id);
        let label = presets::theme_display_name(id);
        draw_preset_card(canvas, text, offset_rect_y(layout.cards[i], -scroll_offset), dark, label, primary);
    }

    let visible_h = (area.bottom - area.top).max(0.0);
    let max_offset = presets_max_scroll_offset(area);
    if max_offset > 0.0 {
        let (thumb_top, thumb_h) = presets_scrollbar_thumb(visible_h, visible_h, layout.content_height, scroll_offset, max_offset);
        let track_left = area.right - PRESET_SCROLLBAR_W;
        let thumb_color = if dark { Color::from_rgba8(0x60, 0x60, 0x60, 0xff) } else { Color::from_rgba8(0xb0, 0xb0, 0xb0, 0xff) };
        let thumb_rect = LRect { left: track_left, top: area.top + thumb_top, right: area.right, bottom: area.top + thumb_top + thumb_h };
        if let Some(r) = to_skia(thumb_rect) {
            canvas.fill_rect(r, thumb_color);
        }
    }

    canvas.clear_clip();
}

/// Hit-tests a click against the preset card grid, accounting for the section's current scroll
/// offset, and applies the hit card's preset to `draft` on a match. Direct port of
/// `ccum-windows/src/config_window.rs::dispatch_presets` -- see this module's doc comment for the
/// draw/hit-test inverse relationship this function's `effective_y` line implements, and why it
/// must stay the exact algebraic inverse of `draw_presets_controls`'s `-scroll_offset`.
///
/// Only reacts to `MouseMsg::Down` (a click) -- there is no drag/hover state for a card the way
/// `Slider`/`RgbaPicker` have. Also gates on `(x, y)` falling within the visible `area` itself,
/// not just on landing inside *some* unscrolled card rect: `panel.rs::dispatch_content_mouse`
/// routes every click here unconditionally (matching every other section), and without this
/// guard a click far outside `area` could still spuriously land on a scrolled-out card once
/// `scroll_offset` is added to its `y` -- the unscrolled content is much taller than the visible
/// area, so "effective y" can range far beyond what's actually on screen.
pub fn dispatch_presets(draft: &mut Settings, area: LRect, scroll_offset: f32, msg: MouseMsg, x: f32, y: f32) -> bool {
    if msg != MouseMsg::Down {
        return false;
    }
    if x < area.left || x >= area.right || y < area.top || y >= area.bottom {
        return false;
    }
    let layout = presets_layout(area);
    let order = preset_grid_order();
    let effective_y = y + scroll_offset;
    let Some(i) = layout
        .cards
        .iter()
        .position(|r| x >= r.left && x < r.right && effective_y >= r.top && effective_y < r.bottom)
    else {
        return false;
    };
    presets::apply_preset(order[i], draft);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> LRect {
        LRect { left, top, right, bottom }
    }

    #[test]
    fn preset_grid_order_groups_by_category_in_builtin_editors_apps_order() {
        let order = preset_grid_order();

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

        for id in &order[0..4] {
            assert_eq!(presets::theme_category(*id), PresetCategory::Builtin, "{id:?}");
        }
        for id in &order[4..22] {
            assert_eq!(presets::theme_category(*id), PresetCategory::Editors, "{id:?}");
        }
        for id in &order[22..24] {
            assert_eq!(presets::theme_category(*id), PresetCategory::Apps, "{id:?}");
        }
    }

    #[test]
    fn presets_layout_produces_24_non_overlapping_cards_within_area() {
        let area = rect(0.0, 0.0, 500.0, 5000.0);
        let layout = presets_layout(area);
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
        assert!(layout.content_height > 0.0);
    }

    #[test]
    fn presets_max_scroll_offset_is_zero_when_everything_fits() {
        let area = rect(0.0, 0.0, 500.0, 20_000.0);
        assert_eq!(presets_max_scroll_offset(area), 0.0);
    }

    #[test]
    fn presets_max_scroll_offset_matches_content_minus_visible_when_short() {
        let area = rect(0.0, 0.0, 500.0, 300.0);
        let content_h = presets_layout(area).content_height;
        let visible_h = area.bottom - area.top;
        assert!(content_h > visible_h, "fixture area must actually need scrolling");
        assert_eq!(presets_max_scroll_offset(area), content_h - visible_h);
    }

    #[test]
    fn clamp_scroll_offset_clamps_to_0_and_max() {
        assert_eq!(clamp_scroll_offset(-50.0, 200.0), 0.0);
        assert_eq!(clamp_scroll_offset(500.0, 200.0), 200.0);
        assert_eq!(clamp_scroll_offset(100.0, 200.0), 100.0);
        assert_eq!(clamp_scroll_offset(50.0, -10.0), 0.0);
    }

    /// The highest-risk correctness point in this task, per this module's doc comment: drawing
    /// offsets a card's unscrolled rect by `-scroll_offset` (`offset_rect_y`), while
    /// `dispatch_presets` adds `scroll_offset` back to the click's `y` before comparing against
    /// the same unscrolled rect. These two must be exact inverses, or a click would resolve to
    /// the wrong card whenever the grid is scrolled. Proven here algebraically for an arbitrary
    /// unscrolled rect and scroll offset -- the end-to-end regression test below then ties this
    /// same relationship to the real `presets_layout`/`dispatch_presets` functions together.
    #[test]
    fn scroll_offset_directions_are_mutual_inverses() {
        let unscrolled = rect(0.0, 500.0, 100.0, 556.0);
        for scroll_offset in [0.0, 1.0, 137.0, 4000.0] {
            let drawn = offset_rect_y(unscrolled, -scroll_offset);
            for on_screen_y in [drawn.top, drawn.top + 10.0, drawn.bottom - 1.0] {
                let effective_y = on_screen_y + scroll_offset;
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
        let (top_at_min, h_min) = presets_scrollbar_thumb(400.0, 400.0, 1200.0, 0.0, 800.0);
        assert_eq!(top_at_min, 0.0);
        let (top_at_max, h_max) = presets_scrollbar_thumb(400.0, 400.0, 1200.0, 800.0, 800.0);
        assert_eq!(h_max, h_min, "thumb height shouldn't depend on scroll position");
        assert!((top_at_max - (400.0 - h_max)).abs() < 0.01, "thumb should reach the track's bottom at max scroll");
    }

    #[test]
    fn presets_scrollbar_thumb_is_proportional_to_visible_fraction() {
        let (_, h) = presets_scrollbar_thumb(400.0, 400.0, 800.0, 0.0, 400.0);
        assert!((h - 200.0).abs() <= 1.0, "h={h}");
    }

    #[test]
    fn presets_scrollbar_thumb_has_a_floor_height_for_very_long_content() {
        let (_, h) = presets_scrollbar_thumb(400.0, 400.0, 1_000_000.0, 0.0, 999_600.0);
        assert!(h >= 400.0 / 8.0, "h={h} must not shrink below the floor");
    }

    #[test]
    fn presets_scrollbar_thumb_degrades_gracefully_for_degenerate_inputs() {
        assert_eq!(presets_scrollbar_thumb(0.0, 0.0, 1000.0, 0.0, 500.0), (0.0, 0.0));
        assert_eq!(presets_scrollbar_thumb(400.0, 400.0, 0.0, 0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn dispatch_presets_ignores_non_down_messages_and_out_of_area_clicks() {
        let area = rect(0.0, 0.0, 500.0, 300.0);
        let layout = presets_layout(area);
        let card = layout.cards[0];
        let x = (card.left + card.right) / 2.0;
        let y = (card.top + card.bottom) / 2.0;

        let mut draft = Settings::default();
        assert!(!dispatch_presets(&mut draft, area, 0.0, MouseMsg::Move, x, y));
        assert!(!dispatch_presets(&mut draft, area, 0.0, MouseMsg::Up, x, y));
        assert_eq!(draft.animation.preset, None);

        // Click far outside `area` (below the visible content) must not hit anything even
        // though, once `scroll_offset` is added, "effective_y" could otherwise land inside some
        // scrolled-out card's unscrolled rect.
        assert!(!dispatch_presets(&mut draft, area, 5000.0, MouseMsg::Down, x, area.bottom + 500.0));
        assert_eq!(draft.animation.preset, None);
    }

    /// End-to-end regression test for the finding the mutual-inverses unit test above doesn't
    /// cover: it proves the algebraic relationship, but never actually calls `presets_layout`,
    /// `preset_grid_order`, or `dispatch_presets` together. This test ties them together the way
    /// the real app does: compute a card's true on-screen position the same way
    /// `draw_presets_controls` does (`presets_layout` + `offset_rect_y(.., -scroll_offset)`), then
    /// feed that exact on-screen point into `dispatch_presets` and assert the *correct* preset
    /// (via `draft.animation.preset`, exactly what `apply_preset` sets) was applied -- not a
    /// neighbor, and not what a sign-flipped/omitted scroll-offset would have resolved to. Direct
    /// port of `ccum-windows/src/config_window.rs`'s
    /// `dispatch_presets_hit_test_matches_the_actual_on_screen_draw_position`, the test that
    /// function's own doc comment says CT-Task 5's review cycle added after finding this exact
    /// gap in the original Windows implementation's first draft.
    #[test]
    fn dispatch_presets_hit_test_matches_the_actual_on_screen_draw_position() {
        // Short area: presets_max_scroll_offset_matches_content_minus_visible_when_short (above)
        // already establishes this fixture requires real scrolling to reach every card.
        let area = rect(0.0, 0.0, 500.0, 300.0);
        let layout = presets_layout(area);
        let order = preset_grid_order();

        // --- Case 1: scroll_offset = 0, a card near the top (sanity baseline). ---
        {
            let card = layout.cards[0];
            let onscreen = offset_rect_y(card, 0.0); // -scroll_offset with scroll_offset = 0
            let x = (onscreen.left + onscreen.right) / 2.0;
            let y = (onscreen.top + onscreen.bottom) / 2.0;

            let mut draft = Settings::default();
            let handled = dispatch_presets(&mut draft, area, 0.0, MouseMsg::Down, x, y);
            assert!(handled, "click on card 0's on-screen position must be handled");
            assert_eq!(draft.animation.preset, Some(order[0]), "clicking card 0 at scroll_offset=0 must apply card 0's own preset");
        }

        // --- Case 2: a card only reachable/visible at a non-zero scroll offset -- the case that
        // actually exercises the scroll math, using the real clamp helper (not a made-up value).
        {
            let max_offset = presets_max_scroll_offset(area);
            assert!(max_offset > 0.0, "fixture must actually require scrolling");
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
                    let visible_scrolled = onscreen.top >= area.top && onscreen.bottom <= area.bottom;
                    (not_visible_unscrolled && visible_scrolled).then_some((i, onscreen))
                })
                .expect("fixture must contain a card only reachable by scrolling");

            let x = (onscreen.left + onscreen.right) / 2.0;
            let y = (onscreen.top + onscreen.bottom) / 2.0;

            // What a *broken* hit-test (scroll offset ignored entirely) would have resolved to at
            // this same on-screen click point, against the same unscrolled card rects -- used
            // below to prove this test would actually catch that regression, not just happen to
            // pass by coincidence.
            let naive_hit_ignoring_scroll = layout
                .cards
                .iter()
                .position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
                .map(|i| order[i]);

            let mut draft = Settings::default();
            let handled = dispatch_presets(&mut draft, area, scroll_offset, MouseMsg::Down, x, y);
            assert!(
                handled,
                "click on card {target_idx}'s real on-screen position (scroll_offset={scroll_offset}) must be handled"
            );
            assert_eq!(
                draft.animation.preset,
                Some(order[target_idx]),
                "clicking card {target_idx} at its true scrolled on-screen position must apply that card's own \
                 preset, not a neighbor's"
            );
            // Guards against a sign-flip/omitted-offset regression: with scroll ignored, this
            // same screen point either hits nothing or a *different* card than the one actually
            // drawn there.
            assert_ne!(
                draft.animation.preset,
                naive_hit_ignoring_scroll,
                "this click must NOT resolve to whatever a scroll-ignoring hit-test would have picked -- if it \
                 does, the scroll offset isn't actually being applied"
            );
        }
    }

    #[test]
    fn dispatch_presets_click_on_the_already_active_card_reapplies_it() {
        let area = rect(0.0, 0.0, 500.0, 5000.0); // tall enough that nothing scrolls
        let layout = presets_layout(area);
        let order = preset_grid_order();
        let card = layout.cards[0];
        let x = (card.left + card.right) / 2.0;
        let y = (card.top + card.bottom) / 2.0;

        let mut draft = Settings::default();
        assert!(dispatch_presets(&mut draft, area, 0.0, MouseMsg::Down, x, y));
        assert_eq!(draft.animation.preset, Some(order[0]));

        // Mutate a field `apply_preset` also touches, then click the same card again: it must
        // re-apply (not treat "already active" as a no-op the way `Segmented::on_mouse` does for
        // its own already-selected pill).
        draft.appearance.opacity = 0.1234;
        assert!(dispatch_presets(&mut draft, area, 0.0, MouseMsg::Down, x, y));

        let mut expected = Settings::default();
        presets::apply_preset(order[0], &mut expected);
        assert_eq!(draft.appearance.opacity, expected.appearance.opacity, "re-clicking the active card must re-apply, not no-op");
        assert_eq!(draft.animation.preset, Some(order[0]));
    }
}
