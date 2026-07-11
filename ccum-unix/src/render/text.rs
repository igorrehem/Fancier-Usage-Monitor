//! `cosmic-text` 0.19.0 integration: shapes and rasterizes text, then composites the
//! resulting glyph coverage into a `render::Canvas`'s `tiny_skia::Pixmap`.
//!
//! API surface confirmed against the actual `cosmic-text-0.19.0` source (fetched into the
//! local cargo registry cache and read directly), not assumed from memory -- a few details
//! are version-specific and worth calling out because they differ from older `cosmic-text`
//! releases documented elsewhere:
//! - `Attrs::new()` takes **no** arguments in 0.19.0 (defaults to `Family::SansSerif`); the
//!   family is set afterwards via the `.family(..)` builder method. Older `cosmic-text`
//!   versions took the family directly in `Attrs::new(family)`.
//! - `Buffer::set_text` takes `(text, &attrs, shaping, alignment)` -- no `&mut FontSystem`
//!   argument; only `Buffer::new`/`shape_until_scroll`/`draw` need `&mut FontSystem`
//!   explicitly (shaping is deferred until `shape_until_scroll`, which `Buffer::draw` calls
//!   internally).
//! - `Buffer::draw`'s callback is `FnMut(i32, i32, u32, u32, cosmic_text::Color)`: an
//!   absolute pixel rect (`x, y, w, h`) in the buffer's own coordinate space plus a
//!   (non-premultiplied) RGBA color for that rect. There is no separate "get glyph bitmap"
//!   step to call by hand -- `draw` walks every shaped glyph, rasterizes it via the
//!   `SwashCache` internally, and invokes the callback once per resulting coverage pixel
//!   (and once more for any decoration rects, e.g. underline/strikethrough, which this app
//!   doesn't currently use).

use cosmic_text::{Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, SwashCache};
use tiny_skia::Color;

use super::Canvas;

/// Owns the two `cosmic-text` resources that are expensive to (re)build and must persist
/// across frames: `FontSystem` performs font discovery/enumeration once at construction
/// (`FontSystem::new()`), and `SwashCache` caches rasterized glyph bitmaps between draws.
/// Deliberately NOT part of `Canvas` -- `Canvas` is a lightweight `Pixmap` wrapper rebuilt
/// fresh every `RedrawRequested` (see `main.rs`), whereas a `TextRenderer` is constructed
/// once and lives for the app's whole lifetime.
pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }

    /// Shapes `text` at `font_size` (in pixels) and rasterizes it into `canvas`, with the
    /// text's top-left origin at `(x, y)` in the canvas's own pixel coordinates.
    ///
    /// Single-line, unbounded-width usage only (no wrapping): `Buffer`'s `width_opt`/
    /// `height_opt` default to `None` (unset), which is exactly what's wanted here -- this
    /// is placeholder proof-of-life text for Task 6, not the real multi-line layout that
    /// later widget-rendering tasks may eventually need.
    pub fn draw_text(&mut self, canvas: &mut Canvas, x: f32, y: f32, text: &str, font_size: f32, color: Color) {
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);

        let attrs = Attrs::new().family(Family::SansSerif);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let c = color.to_color_u8();
        let cosmic_color = CosmicColor::rgba(c.red(), c.green(), c.blue(), c.alpha());

        let (x0, y0) = (x as i32, y as i32);
        let font_system = &mut self.font_system;
        let swash_cache = &mut self.swash_cache;

        buffer.draw(font_system, swash_cache, cosmic_color, |px, py, w, h, glyph_color| {
            let (r, g, b, a) = glyph_color.as_rgba_tuple();
            for dy in 0..h as i32 {
                for dx in 0..w as i32 {
                    canvas.blend_pixel(x0 + px + dx, y0 + py + dy, r, g, b, a);
                }
            }
        });
    }
}
