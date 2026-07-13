//! Shared row-layout helpers for the Font/Size/Animations/Update sections (Task 11): a direct,
//! mechanical port of `ccum-windows/src/config_window.rs`'s `RowCursor`/`dispatch_hit`/
//! `draw_field_label`, adapted to this crate's `LRect`/`Canvas`/`TextRenderer` types (see
//! `render::controls`'s module doc comment for the same coordinate-type/mouse-event-model
//! translation these three followed). Factored out into its own module -- rather than
//! duplicated four more times across `render::font`/`render::size`/`render::animations`/
//! `render::update` -- since all four sections share the exact same "label column + control
//! column, stacked in rows, with occasional group headers/gaps" layout shape.
//!
//! `render::appearance` (Task 10) is NOT ported to use this module: it predates this file and
//! already has its own private, independently-tested `dispatch_hit`/`draw_field_label` copies
//! (its own row layout, `appearance_grid`/`appearance_layout`, is a 2-column grid unlike the
//! straight single-column stack every other section uses, so it never needed a `RowCursor` of
//! its own). Leaving it as-is avoids an unnecessary diff to already-reviewed code.
//!
//! # Porting notes
//!
//! - **No DPI scaling**: mirrors `render::appearance`'s own documented choice (`ccum-unix`
//!   doesn't plumb DPI/scale-factor handling yet) -- every constant below is used at its raw
//!   (96-DPI-baseline) pixel value, unlike `config_window.rs`'s own `RowCursor`, which scales
//!   every measurement by the window's live DPI via `scale(px, dpi)`.

use super::controls::{LRect, MouseMsg};
use super::text::TextRenderer;
use super::{Canvas, Color};

// --- Layout constants -- exact values ported from `ccum-windows/src/config_window.rs`'s own
// constants of the same name (96-DPI-baseline pixels, unscaled -- see this module's doc
// comment). ---
pub const CTRL_ROW_H: f32 = 22.0;
pub const CTRL_LABEL_W: f32 = 96.0;
pub const CTRL_CONTROL_GAP: f32 = 10.0;
pub const CTRL_ROW_GAP: f32 = 8.0;
pub const CTRL_HEADER_H: f32 = 18.0;
pub const CTRL_HEADER_GAP: f32 = 4.0;
pub const CTRL_GROUP_GAP: f32 = 12.0;

/// Lays out a section's controls top-to-bottom as a simple stack of rows (each a left label
/// column + a right control column) and/or group headers, advancing a running `y` cursor as
/// each is consumed. Direct port of `ccum-windows/src/config_window.rs::RowCursor`.
pub struct RowCursor {
    left: f32,
    right: f32,
    y: f32,
}

impl RowCursor {
    pub fn new(area: LRect) -> Self {
        Self { left: area.left, right: area.right, y: area.top }
    }

    /// A section header row (e.g. "Fill"); returns its text rect and advances past it.
    pub fn header(&mut self) -> LRect {
        let h = CTRL_HEADER_H;
        let r = LRect { left: self.left, top: self.y, right: self.right, bottom: self.y + h };
        self.y += h + CTRL_HEADER_GAP;
        r
    }

    /// A label+control row; returns `(label_rect, control_rect)` and advances past it.
    pub fn row(&mut self) -> (LRect, LRect) {
        let h = CTRL_ROW_H;
        let label = LRect { left: self.left, top: self.y, right: self.left + CTRL_LABEL_W, bottom: self.y + h };
        let control = LRect {
            left: self.left + CTRL_LABEL_W + CTRL_CONTROL_GAP,
            top: self.y,
            right: self.right,
            bottom: self.y + h,
        };
        self.y += h + CTRL_ROW_GAP;
        (label, control)
    }

    pub fn group_gap(&mut self) {
        self.y += CTRL_GROUP_GAP;
    }
}

/// Gates a dispatch call so a control only *starts* an interaction (`MouseMsg::Down`) when the
/// click actually landed within its own `rect`. Direct port of
/// `ccum-windows/src/config_window.rs::dispatch_hit` -- see that function's doc comment for why
/// this matters (a bare `Slider::on_mouse` deliberately ignores `y`, trusting the embedder to
/// have already confirmed the click's row). `MouseMsg::Move`/`MouseMsg::Up` always pass through
/// unfiltered so an in-progress drag keeps tracking even once the cursor strays outside its row.
pub fn dispatch_hit(msg: MouseMsg, x: f32, y: f32, rect: LRect) -> bool {
    if msg != MouseMsg::Down {
        return true;
    }
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

/// Draws a left-aligned field/group-header label (e.g. "Height", "Fill"), matching the muted/
/// foreground text color convention `ccum-windows/src/config_window.rs::draw_field_label` uses.
/// Also stands in for that file's separate `draw_group_header` (which is just the same style
/// applied to a header rect there too -- see its own doc comment).
pub fn draw_field_label(canvas: &mut Canvas, text: &mut TextRenderer, rect: LRect, dark: bool, label: &str) {
    let color = if dark { Color::from_rgba8(0xd0, 0xd0, 0xd0, 0xff) } else { Color::from_rgba8(0x30, 0x30, 0x30, 0xff) };
    let y = rect.top + (rect.height() - 12.0) / 2.0;
    text.draw_text(canvas, rect.left, y, label, 12.0, color);
}
