//! Popup settings panel (Task 9: shell + section-nav sidebar only -- per-section content is
//! Tasks 10-12): a second, independent `winit` window, toggled open/closed by a left-click on
//! the tray icon (see `tray::handle_click`/`main.rs::App::handle_tray_event`), with a left
//! sidebar listing the same 6 sections `ccum-windows/src/config_window.rs`'s settings window
//! uses (Appearance/Font/Size/Animations/Update/Presets). Clicking a section switches which
//! one is highlighted; the content area to its right is still a placeholder.
//!
//! # Porting notes (`ccum-windows/src/config_window.rs` is the reference)
//!
//! - **Section labels**: hardcoded English literals here (`SECTIONS`), not yet routed through
//!   `ccum_core::localization::Strings`. This mirrors the ORIGINAL Windows settings window's own
//!   phased history: its Task 11 (the equivalent shell task) also hardcoded English labels,
//!   with real i18n only added later in its Task 18. Same phasing here -- a real i18n pass isn't
//!   trivial (it needs `Strings::section_*` fields already used by `config_window.rs::sections`,
//!   plus a language-resolution call) and isn't worth blocking this shell task on.
//! - **Sidebar layout/hit-test**: `section_rect`/`section_at` are a direct, mechanical port of
//!   `config_window.rs`'s `section_rects`/`section_at` (same "top padding + index * item height"
//!   formula), minus that file's DPI scaling (`scale(px, dpi)`) -- `ccum-unix` doesn't plumb
//!   DPI/scale-factor handling yet, matching `render::bars`'s own documented choice to skip it
//!   for now (see that module's doc comment).
//! - **Paint structure**: `paint` below ports `config_window.rs::paint`'s overall shape --
//!   whole-window background, sidebar background, sidebar/content divider, active-section tint
//!   + accent bar, section labels -- but leaves out that file's preview strip and per-section
//!   controls/button bar entirely (Tasks 10-14's job on this platform), replacing the content
//!   area with a single placeholder line.
//!
//! # Window creation, positioning, decorations
//!
//! - **Multi-window**: this is a genuinely separate `winit` window from `main.rs`'s Tasks 6-7
//!   demo window, created via a second `ActiveEventLoop::create_window` call (confirmed this is
//!   supported in winit 0.30 -- there is nothing window-count-limited about that API) and
//!   tracked by its own `WindowId`, routed in `App::window_event` by comparing against
//!   `Panel::window_id()` before falling through to the demo window's own handling.
//! - **Lazy creation, hide/show toggling**: the window is NOT created until the first time the
//!   panel is opened (`show`, called from `toggle`), mirroring `config_window.rs::open_config_window`'s
//!   own lazy-create-once-then-reuse idiom. Unlike the Windows version (which fully
//!   `DestroyWindow`s on Cancel/Save and recreates fresh next time), this toggles visibility via
//!   `Window::set_visible` and keeps the window/softbuffer surface alive across closes -- cheaper
//!   (no repeated window/context/surface allocation for what will likely be a very
//!   frequently-toggled popup) and avoids re-doing the position/decoration dance on every open.
//! - **Decorations**: `with_decorations(false)` (confirmed available on `WindowAttributes` in
//!   winit 0.30 -- `with_decorations`/`set_decorations`). A borderless popup was chosen over a
//!   normal titled window because it reads as a transient "flyout" (matching how OS-native tray/
//!   menu-bar popups look) rather than a regular application window competing for taskbar/dock
//!   space -- and nothing in this shell's verification (open/close, section-click hit-testing)
//!   turned out to depend on window chrome, so there was no reason to fall back to a titled
//!   window. If a later task finds borderless-window quirks (e.g. platform-specific drag/resize
//!   loss) that matter for real usability, `with_decorations(true)` is a one-line revert here.
//! - **Positioning near the tray icon**: `tray_icon::TrayIcon::rect()` (confirmed present on
//!   Windows/macOS in `tray-icon` 0.24.1's own doc comment -- "Linux: Unsupported") returns the
//!   icon's live on-screen `Rect` (physical position + size), read fresh every time the panel is
//!   shown (`App::handle_tray_event` passes it into `Panel::show` as `Some(TrayAnchor)`).
//!   `compute_position` centers the panel horizontally under/over that rect and opens it upward
//!   if there's room above the icon (matching the Windows/Linux tray convention of icons living
//!   near the screen's bottom edge) or downward otherwise (matching macOS's menu bar living at
//!   the top edge) -- so the exact same formula produces the right-looking flyout direction on
//!   either OS without any `#[cfg(target_os = ...)]` branching. When `rect()` returns `None`
//!   (Linux, or a transient query failure), `compute_position` falls back to a corner of the
//!   primary monitor chosen by the same top/bottom OS convention (top-right on macOS,
//!   bottom-right elsewhere) -- "near the primary display's corner where the OS convention
//!   places menu-bar/tray panels", per this task's brief.
//!
//! # Click-outside-to-close
//!
//! `WindowEvent::Focused(false)` hides the panel, so clicking anywhere outside it closes it like
//! a normal OS flyout. A short grace period (`FOCUS_GRACE`) after `show()` ignores the first
//! unfocus, guarding against a spurious immediate `Focused(false)` some platforms can send right
//! after a freshly-shown window hasn't actually received input focus yet.
//!
//! ## Tray-click/focus-loss race (post-Task-9 fix)
//!
//! Clicking the tray icon again *while the panel is open* can race this same unfocus handling:
//! the click itself steals focus (the OS delivers `WindowEvent::Focused(false)` to the panel
//! near-synchronously with the physical click) before `tray_icon`'s own `Click{Up}` event -- the
//! one `tray::handle_click` gates the toggle on -- arrives, since that event is relayed through a
//! slower path (`TrayIconEvent::set_event_handler` -> `EventLoopProxy::send_event`, see
//! `main.rs`'s top-of-module comment). So the observed order is typically:
//! 1. `Focused(false)` arrives -> `handle_focus_change` -> `hide()` (panel now invisible).
//! 2. The tray's `Click{Up}` event arrives afterward -> `handle_tray_event` -> `Panel::toggle()`.
//!
//! Without a guard, step 2 would see `visible == false` and call `show()` again, reopening the
//! panel a click meant to close. `focus_lost_at`, set by `handle_focus_change` right before a
//! real (non-grace-swallowed) `hide()`, closes exactly that ordering: `toggle()`'s reopen branch
//! (`suppress_reopen`) checks it, and if the panel was just hidden by focus-loss within
//! `TRAY_CLICK_GRACE`, the click that's about to reopen it almost certainly CAUSED that
//! focus-loss, so the toggle treats its "close" goal as already met and does not call `show()`
//! (this is the ordering above, and the one actually observed).
//!
//! The reverse ordering -- the tray's `Click{Up}` event processed first, then a `Focused(false)`
//! plausibly caused by that same click's focus-steal arriving shortly after -- does NOT need a
//! dedicated guard, because `handle_focus_change` is already safe against it via mechanisms that
//! predate this fix:
//! - If that `Click{Up}` was itself a close (`toggle()` took its `hide()` branch), `hide()` sets
//!   `self.visible = false` synchronously, before any later `Focused(false)` can arrive.
//!   `handle_focus_change`'s very first line, `if focused || !self.visible { return; }`, then
//!   discards that later event before it can do anything.
//! - If that `Click{Up}` was instead a reopen (`toggle()` took its `show()` branch), `show()`
//!   sets `self.shown_at = Some(Instant::now())` synchronously. Since `TRAY_CLICK_GRACE` and the
//!   pre-existing post-show `FOCUS_GRACE` window (see "Click-outside-to-close" above) are the
//!   same duration, the `shown_at`-based grace check already in `handle_focus_change` swallows
//!   any `Focused(false)` arriving that soon after, before it would reach any tray-click-specific
//!   check.
//!
//! An earlier version of this fix also added a second timestamp (`last_tray_click_at`, checked
//! in `handle_focus_change`) to guard this reverse ordering explicitly. It was removed: tracing
//! both `toggle()` branches above shows that check could never actually fire -- one of the two
//! existing early-returns always fired first -- so it was dead code that changed no observable
//! behavior. See `Panel::toggle`/`Panel::suppress_reopen`/`Panel::handle_focus_change`.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ccum_core::settings::{PresetId, Settings};
use softbuffer::{Context, Surface};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::render::animations::{self, AnimationControls};
use crate::render::appearance::{self, AppearanceControls};
use crate::render::controls::{LRect, MouseMsg};
use crate::render::font::{self, FontControls};
use crate::render::presets;
use crate::render::size::{self, SizeControls};
use crate::render::text::TextRenderer;
use crate::render::update::{self, UpdateControls};
use crate::render::{Canvas, Color, Rect};

/// `SECTIONS`' index for Presets -- the last of the 6 sections. Named rather than a bare `5`
/// literal at every call site (`handle_mouse_wheel`'s active-section gate, `paint`/
/// `dispatch_content_mouse`'s match arms) so the "which index is Presets" fact lives in one
/// place, mirroring `ccum-windows/src/config_window.rs::draw_section_controls`'s own `_ =>` arm
/// convention (Presets is Windows' fall-through/last arm too, for the same reason: it's the
/// final section in `SECTIONS`' declared order).
const PRESETS_SECTION: usize = 5;

/// Section-nav labels -- see this module's doc comment ("Porting notes") for why these are
/// hardcoded English rather than routed through `ccum_core::localization::Strings` yet. Order
/// matches `Panel::active_section`'s indices throughout this file, same as
/// `config_window.rs::sections`'s own 0=Appearance..5=Presets convention.
pub const SECTIONS: [&str; 6] = ["Appearance", "Font", "Size", "Animations", "Update", "Presets"];

// Layout constants, in unscaled (no-DPI-awareness-yet) pixels -- see this module's doc comment.
//
// `PANEL_W`/`PANEL_H` were widened by Task 10 (from Task 9's original 420x360) to actually fit
// the Appearance section's real content: the 2-column `RgbaPicker` grid's quick-swatch popover
// needs at least ~144px of width within a single grid cell (5 columns x `POPOVER_SWATCH_CELL`
// (24) + gaps + padding -- see `render::controls`), and the tallest reflow case (the top-left
// picker's popover open, pushing every row below it down by up to `popover_height()`) needs
// enough headroom that the opacity row doesn't run off the bottom of a non-scrolling,
// non-resizable window. `ccum-windows/src/config_window.rs`'s own settings window is 700x700 for
// the same reason (`WINDOW_W`/`WINDOW_H`) -- this is a smaller popup, not the full window, but
// needed the same kind of widening once real content (not a placeholder) had to fit inside it.
//
// `PANEL_H` was heightened again by Task 11 (from 460 to 520): the Animations section's own
// content (`render::animations::animation_layout` -- 4 header+row groups plus a trailing
// reduce-motion row, stacked with `layout::RowCursor`) needs ~458px of vertical room below
// `content_area()`'s top, i.e. a content-area bottom of at least `16 (CTRL_PAD) + 458 = 474`px
// -- at the old `PANEL_H` (460), `content_area().bottom` was only 444px, which left the
// `reduce_motion` toggle (and the tail of the Fade/Slide group) drawn *and hit-tested* past the
// bottom of the actual window: real content there is simply unreachable by a real mouse click,
// since the OS never delivers pointer events outside a window's own client area. This was caught
// by Task 11's own native-Windows-run verification (see that task's report), not by any unit
// test -- `dispatch_hit`/`RowCursor`'s pure layout math has no way to know it's being asked to
// lay out more content than the window is tall enough to show. 520 gives content-area bottom
// 504px, ~30px of slack past the 474px actually needed.
const PANEL_W: i32 = 520;
/// Content-area height, unchanged from Task 11's tuning (see the comment above) -- the button
/// bar (Task 13) is added as EXTRA height below this, via `PANEL_H`, specifically so
/// `content_area()`'s footprint (and every section's already-tuned layout, e.g. Animations' own
/// `reduce_motion` row) stays byte-for-byte the same as before this task rather than shrinking
/// to make room for the new buttons.
const BODY_H: i32 = 520;
/// Save/Cancel/Reset button bar height, matching `ccum-windows/src/config_window.rs`'s own
/// `BUTTON_BAR_H`.
const BUTTON_BAR_H: i32 = 56;
const PANEL_H: i32 = BODY_H + BUTTON_BAR_H;
const SIDEBAR_W: i32 = 150;
const SECTION_ITEM_H: i32 = 36;
const SECTION_TOP_PAD: i32 = 12;
const SECTION_TEXT_INSET: i32 = 16;
const ACCENT_BAR_W: i32 = 3;

// --- Task 13: Save/Cancel/Reset button bar -- sizes match
// `ccum-windows/src/config_window.rs`'s own `BTN_W`/`BTN_H`/`BTN_GAP`/`BTN_BAR_PAD` exactly.
const BTN_W: i32 = 84;
const BTN_H: i32 = 30;
const BTN_GAP: i32 = 10;
const BTN_BAR_PAD: i32 = 16;

/// Padding, in unscaled pixels, applied on all sides of the content area (right of the
/// sidebar) before handing it to a section's layout function -- mirrors
/// `ccum-windows/src/config_window.rs`'s `CTRL_PAD` (used by `split_content` there), minus that
/// function's live-preview-strip subtraction (Task 9's shell has no preview strip -- see
/// `render::appearance`'s module doc comment, "No preview strip / button bar").
const CTRL_PAD: f32 = 16.0;

/// Gap, in physical pixels, kept between the panel's edge and the tray icon rect (or monitor
/// edge, for the no-anchor fallback) it's positioned against.
const POSITION_GAP: f64 = 6.0;

/// How long after `show()` an incoming `Focused(false)` is ignored -- see this module's doc
/// comment ("Click-outside-to-close").
const FOCUS_GRACE: Duration = Duration::from_millis(200);

/// Grace window `suppress_reopen` uses to decide whether a reopen-in-progress was almost
/// certainly caused by a `hide()` that just ran in response to that same tray click's OS-level
/// focus-steal -- see this module's doc comment ("Tray-click/focus-loss race"). Deliberately the
/// same duration as `FOCUS_GRACE` (both are "OS event delivery/relay jitter" grace windows) --
/// that equality is also *why* the race's reverse ordering needs no separate guard of its own
/// (see the doc comment), so don't let the two drift apart without re-reading that reasoning.
const TRAY_CLICK_GRACE: Duration = FOCUS_GRACE;

/// Which button-bar button (if any) a click landed on. Direct port of
/// `ccum-windows/src/config_window.rs::ButtonAction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ButtonAction {
    Save,
    Cancel,
    Reset,
}

/// The tray icon's on-screen rect at the moment the panel is being shown, in physical pixels --
/// a local copy of the handful of fields this module actually needs out of
/// `tray_icon::Rect`/`PhysicalPosition`/`PhysicalSize`, so `panel.rs` doesn't have to depend on
/// `tray_icon`'s types directly (kept as a small, independent module like `render`).
#[derive(Clone, Copy, Debug)]
pub struct TrayAnchor {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Owns the popup settings panel's window/softbuffer surface plus its (currently minimal)
/// section-nav state. One instance lives on `main.rs::App` for the whole process lifetime;
/// `window`/`context`/`surface` start `None` and are populated once, lazily, the first time the
/// panel is shown (see this module's doc comment).
pub struct Panel {
    window: Option<Rc<Window>>,
    // Same drop-order justification as `main.rs::App::context`: never read again after
    // `create` stores it, kept alive only because `surface` was built from it.
    #[allow(dead_code)]
    context: Option<Context<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    visible: bool,
    active_section: usize,
    /// Last known cursor position within the panel's own client area, in physical pixels --
    /// updated by `WindowEvent::CursorMoved`, read by `WindowEvent::MouseInput`'s click
    /// handling (winit delivers no position on `MouseInput` itself, only on `CursorMoved`,
    /// same as every other winit app that wants "where was the mouse when this button was
    /// pressed").
    cursor: (f64, f64),
    /// Set by `show()`, cleared once `FOCUS_GRACE` has elapsed -- see this module's doc comment
    /// ("Click-outside-to-close").
    shown_at: Option<Instant>,
    /// Set by `handle_focus_change` immediately before it calls `hide()` in response to a real
    /// (non-grace-swallowed) `Focused(false)`; cleared once consumed by `suppress_reopen`. See
    /// this module's doc comment ("Tray-click/focus-loss race").
    focus_lost_at: Option<Instant>,

    // --- Task 10: Appearance section content ---
    /// Working copy of settings, cloned once (in `create`) from whatever `Settings` `show()`
    /// was first called with -- mirrors `ccum-windows/src/config_window.rs::ConfigState::draft`.
    /// Kept alive across hide/show toggles (see this module's doc comment, "Lazy creation,
    /// hide/show toggling") rather than rebuilt every open, matching how `window`/`surface`
    /// already persist. Only `appearance` (below) reads/writes into it so far; Tasks 11-13
    /// extend both to the remaining sections and Save/Cancel/Reset.
    draft: Option<Settings>,
    /// The settings actually persisted to disk / applied to the real tray icon and demo
    /// rendering, as of the moment `draft` was seeded (see `create`) or last synced by a
    /// successful Save (see `commit_draft`). Distinct from `draft` (the working copy the user is
    /// actively editing): `Cancel` reverts `draft` back to this, and `Reset` rebuilds `draft` as
    /// `Settings::default_preserving_position` of THIS (not of `draft`) -- mirroring
    /// `ccum-windows/src/config_window.rs`'s own Reset handler, which resets from
    /// `window::current_settings()` (the real widget's live settings), never from its own
    /// in-progress `draft`, so a Reset genuinely undoes any unsaved edits too, not just the
    /// appearance/animation fields `default_preserving_position` itself resets. `None` until the
    /// panel's window has been created once (see `create`), same lifecycle as `draft`.
    committed: Option<Settings>,
    /// Live `RgbaPicker`/`Slider` widgets for the Appearance section, seeded from `draft` the
    /// same moment `draft` itself is seeded. `None` until the panel's window has been created
    /// once (see `create`).
    appearance: Option<AppearanceControls>,

    // --- Task 11: Font/Size/Animations/Update section content ---
    // Same seed-once-alongside-`draft`, `None`-until-first-`create` lifecycle as `appearance`
    // above -- see that field's doc comment.
    font: Option<FontControls>,
    size: Option<SizeControls>,
    animations: Option<AnimationControls>,
    update: Option<UpdateControls>,

    // --- Task 12: Presets section content ---
    /// Vertical scroll position of the Presets section's card grid, in pixels of content
    /// scrolled past the top of the section's content area -- `0.0` means scrolled all the way
    /// to the top (the initial/default state). Mirrors
    /// `ccum-windows/src/config_window.rs::ConfigState::presets_scroll_offset` -- see that
    /// field's doc comment for why this lives directly on `Panel` rather than inside some
    /// `PresetsControls` struct (there is no per-field widget state to bundle; this is the only
    /// piece of transient UI state the section has). Clamped to
    /// `[0, presets::presets_max_scroll_offset(..)]` on every mutation (`handle_mouse_wheel`),
    /// never negative. Reset only implicitly (never explicitly), same as the Windows original --
    /// re-opening the panel does NOT currently rebuild `Panel` from scratch (unlike
    /// `ccum-windows`'s per-open `ConfigState`; see `create`'s doc comment, "Lazy creation,
    /// hide/show toggling"), so a scroll position set in one session persists into the next
    /// until the process exits. This is a deliberate, minor behavioral difference from Windows,
    /// not a bug: `ccum-unix`'s hide/show model already keeps every other section's control
    /// state (slider drag positions, open dropdowns notwithstanding `close_dropdown`) across
    /// toggles too, so a persisted scroll position is consistent with that same lifecycle
    /// choice, not an outlier.
    presets_scroll_offset: f32,
}

impl Panel {
    pub fn new() -> Self {
        Self {
            window: None,
            context: None,
            surface: None,
            visible: false,
            active_section: 0,
            cursor: (0.0, 0.0),
            shown_at: None,
            focus_lost_at: None,
            draft: None,
            committed: None,
            appearance: None,
            font: None,
            size: None,
            animations: None,
            update: None,
            presets_scroll_offset: 0.0,
        }
    }

    /// This panel's `WindowId`, if its window has been created yet -- `None` before the first
    /// `show()`. `App::window_event` compares every incoming `WindowId` against this to decide
    /// whether an event belongs to the panel or the Tasks 6-7 demo window.
    pub fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|w| w.id())
    }

    // Not called outside this module's own tests yet -- a natural, minimal accessor (mirroring
    // `render::Canvas::width`'s same `#[allow(dead_code)]` precedent) for later tasks/debugging
    // that need to query panel visibility from outside `panel.rs`.
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    // Same rationale as `is_visible` above -- a natural accessor Tasks 10-12 (per-section
    // content) will need to know which section to draw, not yet called from outside this
    // module's own tests.
    #[allow(dead_code)]
    pub fn active_section(&self) -> usize {
        self.active_section
    }

    /// Opens the panel if it's currently closed, or closes it if it's open -- the single entry
    /// point `main.rs::App::handle_tray_event` calls for every tray-icon left-click. `settings`
    /// is only actually consumed the FIRST time the panel is ever shown (see `create`'s doc
    /// comment on `draft`) -- passed on every call anyway so the caller doesn't need to track
    /// which call is "the first one". `text` is likewise only read on that same first show, to
    /// enumerate installed font families for the Font section's `Dropdown` (see
    /// `render::text::TextRenderer::font_families`) -- `main.rs::App` owns the one persistent
    /// `TextRenderer` this crate has, so it must be threaded in from there rather than the panel
    /// building its own (which would duplicate `FontSystem::new()`'s expensive font-discovery
    /// work -- see that type's own doc comment on why it's built once and shared).
    pub fn toggle(&mut self, event_loop: &ActiveEventLoop, anchor: Option<TrayAnchor>, settings: &Settings, text: &TextRenderer) {
        if self.visible {
            self.hide();
        } else if !self.suppress_reopen() {
            self.show(event_loop, anchor, settings, text);
        }
    }

    /// Returns `true` (and consumes `focus_lost_at`) if a reopen -- about to happen because
    /// `self.visible == false` -- should be suppressed because the panel was hidden by a
    /// `Focused(false)` only moments ago. That focus-loss almost certainly was CAUSED by this
    /// very tray click (the OS's near-synchronous focus-steal notification arriving before
    /// `tray_icon`'s own, more slowly relayed, `Click{Up}` event -- see this module's doc
    /// comment, "Tray-click/focus-loss race"), so the click's "close the panel" goal already
    /// happened and reopening now would undo it instead.
    ///
    /// Split out from `toggle` (rather than inlined) so this decision -- pure timestamp
    /// comparison, no window/`ActiveEventLoop` access -- is directly unit-testable; `toggle`
    /// itself needs a real `ActiveEventLoop` that a `#[test]` here can't construct.
    fn suppress_reopen(&mut self) -> bool {
        if within_grace(self.focus_lost_at, TRAY_CLICK_GRACE) {
            self.focus_lost_at = None;
            true
        } else {
            false
        }
    }

    fn show(&mut self, event_loop: &ActiveEventLoop, anchor: Option<TrayAnchor>, settings: &Settings, text: &TextRenderer) {
        if self.window.is_none() && !self.create(event_loop, settings, text) {
            return;
        }
        let Some(window) = &self.window else { return };

        let monitor_size = event_loop.primary_monitor().map(|m| m.size());
        let pos = compute_position(anchor, PANEL_W, PANEL_H, monitor_size);
        window.set_outer_position(pos);
        window.set_visible(true);
        window.focus_window();
        window.request_redraw();

        self.visible = true;
        self.shown_at = Some(Instant::now());
    }

    fn hide(&mut self) {
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
        self.visible = false;
        self.shown_at = None;
    }

    /// Builds the window/softbuffer context+surface the first time the panel is shown, and
    /// seeds `draft`/`appearance` from `settings` at the same time (once -- see `draft`'s doc
    /// comment on `Panel` for why this only happens on the very first `show()`, not every
    /// toggle). Returns `false` (leaving `self.window`/etc. untouched at `None`) if any step
    /// fails, mirroring `main.rs::App::resumed`'s own error handling for the demo window --
    /// except a failure here only disables the popup panel, not the whole process
    /// (`event_loop.exit()` is NOT called).
    fn create(&mut self, event_loop: &ActiveEventLoop, settings: &Settings, text: &TextRenderer) -> bool {
        let attributes = Window::default_attributes()
            .with_title("Claude Code Usage Monitor \u{2014} Settings")
            .with_inner_size(PhysicalSize::new(PANEL_W as u32, PANEL_H as u32))
            .with_resizable(false)
            .with_decorations(false)
            // Created hidden -- `show()` positions it first, then makes it visible, so it never
            // flashes at winit's default (0, 0)-ish placement before `set_outer_position` runs.
            .with_visible(false);

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => {
                eprintln!("ccum-unix: failed to create popup panel window: {err}");
                return false;
            }
        };

        let context = match Context::new(window.clone()) {
            Ok(context) => context,
            Err(err) => {
                eprintln!("ccum-unix: failed to create popup panel softbuffer context: {err}");
                return false;
            }
        };

        let surface = match Surface::new(&context, window.clone()) {
            Ok(surface) => surface,
            Err(err) => {
                eprintln!("ccum-unix: failed to create popup panel softbuffer surface: {err}");
                return false;
            }
        };

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);

        if self.draft.is_none() {
            let draft = settings.clone();
            // `is_dark: true` -- no OS dark/light-mode detection wired up yet, matching
            // `render::bars`/this module's own `paint` (see that function's doc comment).
            self.appearance = Some(AppearanceControls::from_settings(&draft.appearance, true));
            self.font = Some(FontControls::from_settings(&draft.typography, text.font_families()));
            self.size = Some(SizeControls::from_settings(&draft.geometry));
            self.animations = Some(AnimationControls::from_settings(&draft.animation));
            self.update = Some(UpdateControls::from_settings(draft.poll_interval_ms));
            self.committed = Some(draft.clone());
            self.draft = Some(draft);
        }

        true
    }

    /// Routes one `WindowEvent` already confirmed (by `App::window_event`, via `window_id`) to
    /// belong to this panel's own window. Kept as a single dispatch method (rather than having
    /// `main.rs` match on individual variants itself) so all of the panel's window-event
    /// handling logic -- and the invariants around it (e.g. `cursor` must be updated before a
    /// same-frame `MouseInput` click can hit-test against it) -- stays local to this module.
    ///
    /// Returns `Some(settings)` exactly when this event was a click on the Save button that
    /// actually persisted a draft (see `handle_button_action`) -- `main.rs::App` uses this to
    /// apply the just-saved settings to its own live state (tray icon rendering, poll interval),
    /// which only `App`, not `Panel`, has access to. Every other event returns `None`.
    pub fn handle_window_event(&mut self, event: &WindowEvent, text: &mut TextRenderer) -> Option<Settings> {
        match event {
            // A borderless window has no OS close button to generate this from directly, but
            // some platforms/window managers can still send it (e.g. Alt+F4) -- treat it as
            // "close the panel", not "exit the whole app" (only the demo window's
            // `CloseRequested` does that, in `main.rs`).
            WindowEvent::CloseRequested => {
                self.hide();
                None
            }
            WindowEvent::Resized(_) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                None
            }
            WindowEvent::RedrawRequested => {
                self.redraw(text);
                None
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                // Broadcast unconditionally -- see `config_window.rs`'s own `WM_MOUSEMOVE`
                // handler's identical reasoning (`render::controls`'s module doc comment,
                // "Mouse message model"): only a control mid-drag reacts to this at all, so
                // it's cheap even at full mouse-move rates.
                self.dispatch_content_mouse(MouseMsg::Move);
                None
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if *button == MouseButton::Left {
                    match state {
                        ElementState::Pressed => return self.handle_click(),
                        ElementState::Released => self.dispatch_content_mouse(MouseMsg::Up),
                    }
                }
                None
            }
            WindowEvent::Focused(focused) => {
                self.handle_focus_change(*focused);
                None
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(*delta);
                None
            }
            _ => None,
        }
    }

    /// Handles `WindowEvent::MouseWheel`: adjusts the Presets section's scroll offset, gated to
    /// the Presets section being active (mirroring
    /// `ccum-windows/src/config_window.rs`'s own `WM_MOUSEWHEEL` handler, which is a no-op
    /// unless `active_section == 5` -- no other section has scrollable content of its own yet).
    /// `winit` delivers no cursor position with this event on every platform (unlike Win32's
    /// `WM_MOUSEWHEEL`, whose `lParam` at least carries *screen* coordinates), so -- like the
    /// Windows original, which explicitly documents skipping a cursor-position gate here for the
    /// same reason -- this reacts to every wheel event delivered to the panel window while
    /// Presets is active, without checking where the cursor actually is; safe because the whole
    /// visible content area *is* the scrollable region whenever Presets is active, so "the panel
    /// has focus/received this event" and "the cursor is over the scrollable content" coincide
    /// in practice for this one section.
    ///
    /// `MouseScrollDelta::LineDelta`'s `y` is a notch count (`ccum-windows`' `WHEEL_DELTA`-based
    /// `notches`); `PixelDelta`'s `y` is already real pixels (high-precision trackpad/touch
    /// scroll) and is used directly rather than re-quantized into notch-sized steps. Both cases
    /// follow `winit::event::MouseScrollDelta`'s own documented sign convention -- "positive
    /// values indicate that the content that is being scrolled should move right and down" --
    /// which is the same direction `ccum-windows/src/config_window.rs`'s own `WM_MOUSEWHEEL`
    /// handler assumes for its `notches` (there, a positive `notches` -- wheel rotated away from
    /// the user, the traditional "scroll toward the top" gesture -- *decreases* the offset, i.e.
    /// moves content down/reveals what's above): `new_offset = old_offset - delta_px`.
    fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if self.active_section != PRESETS_SECTION {
            return;
        }
        let delta_px = match delta {
            MouseScrollDelta::LineDelta(_, y) => y * presets::PRESET_SCROLL_STEP,
            MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
        };
        if delta_px == 0.0 {
            return;
        }
        let area = content_area();
        let max_offset = presets::presets_max_scroll_offset(area);
        let new_offset = presets::clamp_scroll_offset(self.presets_scroll_offset - delta_px, max_offset);
        if new_offset != self.presets_scroll_offset {
            self.presets_scroll_offset = new_offset;
            self.request_redraw();
        }
    }

    /// Handles `MouseInput{Pressed, Left}`: sidebar clicks switch the active section (closing
    /// any open Appearance popover -- see `AppearanceControls::close_all_popovers`'s doc
    /// comment); every other click is routed to the active section's content dispatch. Direct
    /// port of `ccum-windows/src/config_window.rs`'s `WM_LBUTTONDOWN` handler's own
    /// sidebar-vs-content split (a sidebar hit always returns early, never falling through to
    /// `dispatch_controls`/`dispatch_content_mouse`).
    /// Handles `MouseInput{Pressed, Left}`: button-bar clicks (Task 13) act immediately, before
    /// any section-nav/content dispatch -- matching
    /// `ccum-windows/src/config_window.rs::WM_LBUTTONDOWN`'s own ordering comment ("Button-bar
    /// clicks act immediately on down ... and are handled before capture/dispatch since
    /// Save/Cancel destroy the window"; this panel doesn't destroy its window on Save/Cancel,
    /// but hides it, so the same "handle first, don't fall through" ordering still applies).
    /// Sidebar clicks switch the active section (closing any open Appearance popover -- see
    /// `AppearanceControls::close_all_popovers`'s doc comment); every other click is routed to
    /// the active section's content dispatch.
    fn handle_click(&mut self) -> Option<Settings> {
        let size = match &self.window {
            Some(window) => window.inner_size(),
            None => return None,
        };
        let (cx, cy) = (self.cursor.0 as f32, self.cursor.1 as f32);
        let (reset_rect, cancel_rect, save_rect) = button_rects(size.width as i32, size.height as i32);
        if let Some(action) = button_at(cx, cy, reset_rect, cancel_rect, save_rect) {
            return self.handle_button_action(action);
        }
        if let Some(idx) = section_at(self.cursor.0, self.cursor.1, size.width, size.height) {
            if idx != self.active_section {
                self.active_section = idx;
                // Force-close every popover/dropdown that extends outside its own control rect
                // (see `AppearanceControls::close_all_popovers`'s doc comment): navigating away
                // while one is open must never leave it rendering pre-expanded on return.
                // `Segmented`/`Toggle`/bare `Slider` have no such open/closed state, so Size/
                // Animations/Update need no equivalent close call here.
                if let Some(appearance) = &mut self.appearance {
                    appearance.close_all_popovers();
                }
                if let Some(font) = &mut self.font {
                    font.close_dropdown();
                }
                self.request_redraw();
            }
            return None;
        }
        self.dispatch_content_mouse(MouseMsg::Down);
        None
    }

    /// Runs a button-bar button's effect -- direct port of
    /// `ccum-windows/src/config_window.rs::handle_button_action`'s three behaviors, adapted to
    /// this panel's hide/show (not destroy/recreate) window lifecycle:
    /// - `Save`: persists `draft` to disk (`ccum_core::settings::save`, the SAME function/path
    ///   `ccum-windows` uses -- see `ccum_core::settings::settings_path`'s per-OS doc comments)
    ///   via `commit_draft`, then hides the panel. Returns the just-saved settings so
    ///   `main.rs::App` can apply them to its own live state.
    /// - `Cancel`: discards `draft`'s in-progress edits by reverting it (and every section's
    ///   cached control state) back to `committed` -- what's actually on disk/live, as of the
    ///   last successful Save or this panel's first-ever open -- then hides the panel. Nothing
    ///   is persisted; always returns `None`.
    /// - `Reset`: rebuilds `draft` as `Settings::default_preserving_position` of `committed`
    ///   (NOT of `draft` -- see `committed`'s own doc comment on why this must read from the
    ///   real live settings, undoing in-progress edits too, matching the Windows original). Does
    ///   NOT close the panel (so the user can keep editing or still Cancel afterward) and never
    ///   persists; always returns `None`.
    fn handle_button_action(&mut self, action: ButtonAction) -> Option<Settings> {
        match action {
            ButtonAction::Save => {
                let committed = self.commit_draft();
                if let Some(draft) = &committed {
                    ccum_core::settings::save(draft);
                }
                committed
            }
            ButtonAction::Cancel => {
                self.revert_draft_to_committed();
                self.hide();
                None
            }
            ButtonAction::Reset => {
                if let Some(committed) = self.committed.clone() {
                    self.apply_new_draft(Settings::default_preserving_position(committed));
                }
                None
            }
        }
    }

    /// The disk-I/O-free portion of Save's effect: commits `draft` as the new `committed`
    /// settings and hides the panel, returning a clone of what was just committed (or `None` if
    /// `draft` was never seeded, i.e. the panel's window was never created). Split out from
    /// `handle_button_action`'s `Save` arm specifically so this logic is directly unit-testable
    /// without writing to the real `ccum_core::settings::settings_path()` on whatever machine
    /// runs `cargo test` -- see this module's test suite for why that matters (mirroring
    /// `ccum-core::settings`'s own test module, which likewise never exercises `save`/`load`'s
    /// actual disk I/O, only the pure data-shape logic around it).
    fn commit_draft(&mut self) -> Option<Settings> {
        let draft = self.draft.clone()?;
        self.committed = Some(draft.clone());
        self.hide();
        Some(draft)
    }

    /// Reverts `draft` to `committed` -- Cancel's effect, minus hiding the panel (kept separate
    /// so a future caller could revert without also closing, though none does today). A no-op if
    /// `committed` was never seeded (panel window never created).
    fn revert_draft_to_committed(&mut self) {
        if let Some(committed) = self.committed.clone() {
            self.apply_new_draft(committed);
        }
    }

    /// Replaces `draft` with `new_settings` and rebuilds every section's cached control state
    /// from it -- the same "rebuild controls from settings after an external draft mutation"
    /// discipline `dispatch_content_mouse`'s Presets arm already uses (see that call site's own
    /// comment), just applied to the whole `Settings` at once instead of two sections. The
    /// Font section's dropdown reuses whatever font-family list is already cached on
    /// `self.font` (rather than needing a `TextRenderer` here to re-enumerate installed fonts --
    /// this method has no access to one, and the list doesn't change at runtime anyway), falling
    /// back to a single-item list containing `new_settings.typography.family` if `self.font` was
    /// never seeded yet, mirroring `FontControls::from_settings`'s own empty-list fallback.
    fn apply_new_draft(&mut self, new_settings: Settings) {
        // `is_dark: true` matches `create`'s own hardcoded seed (no OS dark/light-mode detection
        // wired up yet -- see that method's doc comment).
        self.appearance = Some(AppearanceControls::from_settings(&new_settings.appearance, true));
        let families = self
            .font
            .as_ref()
            .map(|f| f.family.items.clone())
            .unwrap_or_else(|| vec![new_settings.typography.family.clone()]);
        self.font = Some(FontControls::from_settings(&new_settings.typography, families));
        self.size = Some(SizeControls::from_settings(&new_settings.geometry));
        self.animations = Some(AnimationControls::from_settings(&new_settings.animation));
        self.update = Some(UpdateControls::from_settings(new_settings.poll_interval_ms));
        self.draft = Some(new_settings);
        self.request_redraw();
    }

    /// Routes a mouse message to the active section's content controls and requests a repaint if
    /// anything changed. A no-op before the panel's window (and thus `draft`/the section
    /// controls) has ever been created.
    fn dispatch_content_mouse(&mut self, msg: MouseMsg) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        let area = content_area();
        let (x, y) = (self.cursor.0 as f32, self.cursor.1 as f32);
        let changed = match self.active_section {
            0 => match &mut self.appearance {
                Some(appearance) => appearance::dispatch_appearance(appearance, draft, area, msg, x, y),
                None => false,
            },
            1 => match &mut self.font {
                Some(font) => font::dispatch_font(font, draft, area, msg, x, y),
                None => false,
            },
            2 => match &mut self.size {
                Some(size) => size::dispatch_size(size, draft, area, msg, x, y),
                None => false,
            },
            3 => match &mut self.animations {
                Some(animations) => animations::dispatch_animations(animations, draft, area, msg, x, y),
                None => false,
            },
            4 => match &mut self.update {
                Some(update) => update::dispatch_update(update, draft, area, msg, x, y),
                None => false,
            },
            PRESETS_SECTION => {
                let applied = presets::dispatch_presets(draft, area, self.presets_scroll_offset, msg, x, y);
                if applied {
                    // `ccum_core::presets::apply_preset` only ever touches `draft.appearance`/
                    // `draft.animation` (never `typography`/`geometry`/`poll_interval_ms` -- see
                    // that function's own doc comment), so only those two sections' cached
                    // control state needs rebuilding from the mutated draft -- the same
                    // "rebuild controls from settings after an external draft mutation"
                    // discipline `ccum-windows/src/config_window.rs::dispatch_presets` uses via
                    // `SectionControls::from_settings`, scoped down to just the two sections
                    // actually touched instead of every section wholesale (this crate has no
                    // single `SectionControls` struct to rebuild in one shot -- see `Panel`'s
                    // own per-section `Option<...Controls>` fields). `is_dark: true` matches
                    // `create`'s own hardcoded seed (see that method's doc comment -- no OS
                    // dark/light-mode detection wired up yet).
                    self.appearance = Some(AppearanceControls::from_settings(&draft.appearance, true));
                    self.animations = Some(AnimationControls::from_settings(&draft.animation));
                }
                applied
            }
            _ => false,
        };
        if changed {
            self.request_redraw();
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn handle_focus_change(&mut self, focused: bool) {
        if focused || !self.visible {
            return;
        }
        // Swallow the first unfocus within `FOCUS_GRACE` of showing -- see this module's doc
        // comment ("Click-outside-to-close"). This same check also happens to be what makes the
        // tray-click/focus-loss race's reverse ordering safe without a dedicated guard: see this
        // module's doc comment ("Tray-click/focus-loss race") for why.
        if within_grace(self.shown_at, FOCUS_GRACE) {
            return;
        }
        self.focus_lost_at = Some(Instant::now());
        self.hide();
    }

    /// Ticks nothing (this shell has no animation of its own yet) and paints one full frame:
    /// sidebar + section labels + a content placeholder, then presents it -- same
    /// build-off-screen-then-present-once double-buffering discipline `main.rs::App::redraw`
    /// uses for the demo window, for the same tearing/flicker-avoidance reason (see
    /// `render::Canvas`'s doc comment).
    fn redraw(&mut self, text: &mut TextRenderer) {
        if !self.visible {
            return;
        }
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };

        let size = window.inner_size();
        let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };

        if let Err(err) = surface.resize(width, height) {
            eprintln!("ccum-unix: failed to resize popup panel softbuffer surface: {err}");
            return;
        }

        let Some(mut canvas) = Canvas::new(width.get(), height.get()) else {
            return;
        };
        let active_preset = self.draft.as_ref().and_then(|d| d.animation.preset);
        paint(
            &mut canvas,
            text,
            width.get(),
            height.get(),
            self.active_section,
            self.appearance.as_ref(),
            self.font.as_ref(),
            self.size.as_ref(),
            self.animations.as_ref(),
            self.update.as_ref(),
            active_preset,
            self.presets_scroll_offset,
        );

        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(err) => {
                eprintln!("ccum-unix: failed to acquire popup panel softbuffer buffer: {err}");
                return;
            }
        };

        // Same opaque-0x00RRGGBB / premultiplied-RGBA8-at-alpha-255 equivalence `main.rs`'s own
        // present step relies on -- see that function's doc comment.
        for (dst, src) in buffer.iter_mut().zip(canvas.pixmap().pixels().iter()) {
            *dst = (u32::from(src.red()) << 16) | (u32::from(src.green()) << 8) | u32::from(src.blue());
        }

        if let Err(err) = buffer.present() {
            eprintln!("ccum-unix: failed to present popup panel softbuffer buffer: {err}");
        }
    }
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `ts` (a timestamp of some prior event) is still within `grace` of now -- shared by
/// `Panel::suppress_reopen`'s `focus_lost_at` check (the tray-click/focus-loss race guard, see
/// this module's doc comment) and `Panel::handle_focus_change`'s pre-existing `shown_at` check
/// ("Click-outside-to-close"). `None` (no prior event recorded) is never "within grace".
fn within_grace(ts: Option<Instant>, grace: Duration) -> bool {
    ts.is_some_and(|t| t.elapsed() < grace)
}

/// Client rect of section-nav item `i`, top to bottom, spanning the sidebar's width -- a direct
/// (DPI-scaling-free -- see this module's doc comment) port of
/// `config_window.rs::section_rects`'s "top padding + index * item height" formula.
fn section_rect(i: usize) -> Rect {
    let top = (SECTION_TOP_PAD + i as i32 * SECTION_ITEM_H) as f32;
    Rect::from_xywh(0.0, top, SIDEBAR_W as f32, SECTION_ITEM_H as f32)
        .expect("section_rect: SIDEBAR_W/SECTION_ITEM_H are fixed positive constants")
}

/// The content panel's controls area: everything right of the sidebar divider, inset by
/// `CTRL_PAD` on all sides -- see the `CTRL_PAD` doc comment for how this differs from
/// `ccum-windows/src/config_window.rs::split_content`'s equivalent (no preview-strip
/// subtraction). Fixed (not size-dependent) since the panel is `with_resizable(false)`, the
/// same reasoning `section_rect` below already relies on for its own fixed layout.
fn content_area() -> LRect {
    let content_left = SIDEBAR_W as f32 + 1.0;
    LRect {
        left: content_left + CTRL_PAD,
        top: CTRL_PAD,
        right: (PANEL_W as f32 - CTRL_PAD).max(content_left + CTRL_PAD),
        bottom: (BODY_H as f32 - CTRL_PAD).max(CTRL_PAD),
    }
}

/// Rects for the three button-bar buttons, given the window's client size: `Reset` pinned to
/// the left edge, `Cancel`/`Save` pinned to the right edge (`Save` rightmost, matching the
/// common OK-is-rightmost convention), all vertically centered in the bar below `BODY_H`. Direct
/// port of `ccum-windows/src/config_window.rs::button_rects`, minus DPI scaling (matching every
/// other layout function in this module -- see this module's doc comment, "Porting notes").
/// Shared by `paint` (drawing) and `handle_click` (hit-testing) so the two can never drift apart.
fn button_rects(client_w: i32, client_h: i32) -> (LRect, LRect, LRect) {
    let body_bottom = (client_h - BUTTON_BAR_H) as f32;
    let bar_h = BUTTON_BAR_H as f32;
    let btn_w = BTN_W as f32;
    let btn_h = BTN_H as f32;
    let gap = BTN_GAP as f32;
    let pad = BTN_BAR_PAD as f32;

    let top = body_bottom + (bar_h - btn_h) / 2.0;
    let bottom = top + btn_h;

    let save_right = client_w as f32 - pad;
    let save_left = save_right - btn_w;
    let save = LRect { left: save_left, top, right: save_right, bottom };

    let cancel_right = save_left - gap;
    let cancel_left = cancel_right - btn_w;
    let cancel = LRect { left: cancel_left, top, right: cancel_right, bottom };

    let reset_left = pad;
    let reset_right = reset_left + btn_w;
    let reset = LRect { left: reset_left, top, right: reset_right, bottom };

    (reset, cancel, save)
}

/// Which button-bar button (if any) contains client point `(x, y)`. Direct port of
/// `ccum-windows/src/config_window.rs::button_at`.
fn button_at(x: f32, y: f32, reset: LRect, cancel: LRect, save: LRect) -> Option<ButtonAction> {
    let hit = |r: LRect| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
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

/// Draws one button-bar button: `Save` as a filled accent "primary" button, `Cancel`/`Reset` as
/// bordered "secondary" buttons -- the same accent/neutral tones `paint`'s sidebar
/// active-section highlight already uses, matching
/// `ccum-windows/src/config_window.rs::draw_button`'s own palette choice. The border is drawn as
/// a filled outer rect in the border color, then a filled 1px-inset rect in `bg` on top --
/// `render::Canvas` has no stroke-a-rect-outline primitive (only `fill_rect`/`fill_rounded_rect`/
/// `fill_circle`, see that module's doc comment), so this is the simplest way to get a
/// 1px-bordered rect out of fill-only primitives, same trick used nowhere else in this crate
/// only because nothing else needed a bordered rect until now.
fn draw_button(canvas: &mut Canvas, text: &mut TextRenderer, rect: LRect, label: &str, primary: bool) {
    let accent = Color::from_rgba8(0xd9, 0x77, 0x57, 0xff);
    let (bg, text_color, border) = if primary {
        (accent, Color::from_rgba8(0xff, 0xff, 0xff, 0xff), None)
    } else {
        (
            Color::from_rgba8(0x2a, 0x2a, 0x2a, 0xff),
            Color::from_rgba8(0xf0, 0xf0, 0xf0, 0xff),
            Some(Color::from_rgba8(0x45, 0x45, 0x45, 0xff)),
        )
    };

    match border {
        Some(border_color) => {
            if let Some(r) = Rect::from_ltrb(rect.left, rect.top, rect.right, rect.bottom) {
                canvas.fill_rect(r, border_color);
            }
            let (left, top, right, bottom) = (rect.left + 1.0, rect.top + 1.0, rect.right - 1.0, rect.bottom - 1.0);
            if let Some(r) = Rect::from_ltrb(left, top, right, bottom) {
                canvas.fill_rect(r, bg);
            }
        }
        None => {
            if let Some(r) = Rect::from_ltrb(rect.left, rect.top, rect.right, rect.bottom) {
                canvas.fill_rect(r, bg);
            }
        }
    }

    let font_size = 13.0;
    let text_w = text.text_width(label, font_size);
    let tx = rect.left + (rect.width() - text_w) / 2.0;
    // Same close-enough fixed vertical-centering offset the sidebar labels use in `paint` below
    // (see that call site's own comment on why this isn't real text-metrics-based centering).
    let ty = rect.top + (rect.height() - font_size) / 2.0;
    text.draw_text(canvas, tx, ty, label, font_size, text_color);
}

/// Draws the three buttons into the button bar, given the window's client size. Called from
/// `paint`.
fn draw_button_bar(canvas: &mut Canvas, text: &mut TextRenderer, client_w: i32, client_h: i32) {
    let (reset, cancel, save) = button_rects(client_w, client_h);
    draw_button(canvas, text, reset, "Reset", false);
    draw_button(canvas, text, cancel, "Cancel", false);
    draw_button(canvas, text, save, "Save", true);
}

/// Index of the section-nav item containing client point `(x, y)`, if any -- mirrors
/// `config_window.rs::section_at`. `width`/`height` (the panel's live client size) are accepted
/// for symmetry with that function's signature but not currently needed by the hit-test itself
/// (every `section_rect` is already bounded to the sidebar's fixed width/height, independent of
/// the window's overall size).
fn section_at(x: f64, y: f64, width: u32, height: u32) -> Option<usize> {
    let _ = (width, height);
    let (x, y) = (x as f32, y as f32);
    (0..SECTIONS.len()).find(|&i| {
        let r = section_rect(i);
        x >= r.left() && x < r.right() && y >= r.top() && y < r.bottom()
    })
}

/// Computes where to place the panel's top-left corner (in physical screen pixels), given the
/// tray icon's anchor rect (if known) and the primary monitor's size (if known) -- see this
/// module's doc comment ("Positioning near the tray icon") for the full reasoning.
fn compute_position(
    anchor: Option<TrayAnchor>,
    panel_w: i32,
    panel_h: i32,
    monitor_size: Option<PhysicalSize<u32>>,
) -> PhysicalPosition<i32> {
    let (monitor_w, monitor_h) = monitor_size
        .map(|s| (f64::from(s.width), f64::from(s.height)))
        // 1920x1080 is only reached if `primary_monitor()` itself returns `None` (no monitor
        // info available at all, e.g. some headless/CI window-manager setups) -- a plausible
        // modern-display guess purely so the clamp below still produces an on-screen-ish
        // position instead of operating on a degenerate 0x0 "monitor".
        .unwrap_or((1920.0, 1080.0));

    let (x, y) = match anchor {
        Some(a) => {
            let cx = a.x + a.width / 2.0 - f64::from(panel_w) / 2.0;
            let above_y = a.y - f64::from(panel_h) - POSITION_GAP;
            let below_y = a.y + a.height + POSITION_GAP;
            // Opens upward if there's room above the icon (the Windows/Linux tray convention:
            // icons live near the bottom edge, so the flyout should grow toward the middle of
            // the screen, not off the bottom) -- otherwise downward (the macOS convention: the
            // menu bar lives at the very top, so `above_y` goes negative and this falls through
            // to opening below it instead). One formula, no `#[cfg(target_os = ...)]` needed.
            let y = if above_y >= 0.0 { above_y } else { below_y };
            (cx, y)
        }
        None => {
            // No anchor available (Linux, where `TrayIcon::rect()` is documented unsupported --
            // see `tray-icon` 0.24.1's own doc comment -- or a transient query failure): fall
            // back to the screen corner the OS convention places tray/menu-bar panels in --
            // top-right on macOS (menu bar), bottom-right elsewhere (Windows/Linux taskbar
            // system trays are conventionally bottom-right).
            #[cfg(target_os = "macos")]
            {
                (monitor_w - f64::from(panel_w) - POSITION_GAP, POSITION_GAP)
            }
            #[cfg(not(target_os = "macos"))]
            {
                (
                    monitor_w - f64::from(panel_w) - POSITION_GAP,
                    monitor_h - f64::from(panel_h) - POSITION_GAP,
                )
            }
        }
    };

    let x = x.clamp(0.0, (monitor_w - f64::from(panel_w)).max(0.0));
    let y = y.clamp(0.0, (monitor_h - f64::from(panel_h)).max(0.0));
    PhysicalPosition::new(x.round() as i32, y.round() as i32)
}

/// Paints one full frame into `canvas`: whole-window background, sidebar background, the
/// sidebar/content divider, each section's label (with the active one tinted + accent-barred),
/// and each section's own controls -- a direct port of `config_window.rs::paint`'s overall shape
/// (see this module's doc comment), minus that file's preview strip/button bar, which don't
/// exist yet on this platform.
///
/// Colors are the same fixed dark-mode palette `config_window.rs::paint` uses for its own dark
/// branch -- `ccum-unix` has no OS dark/light-mode detection wired up yet, matching
/// `render::bars::draw_bars`'s own documented `is_dark` placeholder (see that module's doc
/// comment).
#[allow(clippy::too_many_arguments)]
fn paint(
    canvas: &mut Canvas,
    text: &mut TextRenderer,
    width: u32,
    height: u32,
    active_section: usize,
    appearance: Option<&AppearanceControls>,
    font: Option<&FontControls>,
    size: Option<&SizeControls>,
    animation_controls: Option<&AnimationControls>,
    update_controls: Option<&UpdateControls>,
    active_preset: Option<PresetId>,
    presets_scroll_offset: f32,
) {
    let bg = Color::from_rgba8(0x1e, 0x1e, 0x1e, 0xff);
    let sidebar_bg = Color::from_rgba8(0x25, 0x25, 0x26, 0xff);
    let divider = Color::from_rgba8(0x3a, 0x3a, 0x3a, 0xff);
    let text_color = Color::from_rgba8(0xf0, 0xf0, 0xf0, 0xff);
    let muted_text = Color::from_rgba8(0xb0, 0xb0, 0xb0, 0xff);
    let active_tint = Color::from_rgba8(0x33, 0x2a, 0x26, 0xff);
    let accent = Color::from_rgba8(0xd9, 0x77, 0x57, 0xff);

    canvas.clear(bg);

    let (w, h) = (width as f32, height as f32);
    let sidebar_w = (SIDEBAR_W as f32).min(w);

    if let Some(sidebar_rect) = Rect::from_xywh(0.0, 0.0, sidebar_w, h) {
        canvas.fill_rect(sidebar_rect, sidebar_bg);
    }
    if let Some(divider_rect) = Rect::from_xywh(sidebar_w, 0.0, 1.0f32.min((w - sidebar_w).max(0.0)), h) {
        canvas.fill_rect(divider_rect, divider);
    }

    for (i, label) in SECTIONS.iter().enumerate() {
        let item = section_rect(i);
        if item.top() >= h {
            break;
        }
        let active = i == active_section;
        if active {
            canvas.fill_rect(item, active_tint);
            if let Some(accent_rect) = Rect::from_xywh(0.0, item.top(), ACCENT_BAR_W as f32, item.height()) {
                canvas.fill_rect(accent_rect, accent);
            }
        }
        let color = if active { text_color } else { muted_text };
        // Roughly vertically centers a 14px-line-height label within `SECTION_ITEM_H`; not a
        // real text-metrics-based centering (cosmic-text's `Buffer` doesn't expose ascent/
        // descent through `TextRenderer::draw_text`'s current minimal API -- see that module's
        // doc comment on why it's single-line/unbounded-width only for now), just a close-enough
        // fixed offset matching the 14px font size used below.
        let text_y = item.top() + (item.height() - 14.0) / 2.0;
        text.draw_text(canvas, item.left() + SECTION_TEXT_INSET as f32, text_y, label, 14.0, color);
    }

    let content_left = sidebar_w + 1.0;
    if content_left < w {
        match active_section {
            0 => {
                if let Some(appearance) = appearance {
                    appearance::draw_appearance_controls(canvas, text, content_area(), true, appearance);
                }
            }
            1 => {
                if let Some(font) = font {
                    font::draw_font_controls(canvas, text, content_area(), true, font);
                }
            }
            2 => {
                if let Some(size) = size {
                    size::draw_size_controls(canvas, text, content_area(), true, size);
                }
            }
            3 => {
                if let Some(animation_controls) = animation_controls {
                    animations::draw_animation_controls(canvas, text, content_area(), true, animation_controls);
                }
            }
            4 => {
                if let Some(update_controls) = update_controls {
                    update::draw_update_controls(canvas, text, content_area(), true, update_controls);
                }
            }
            PRESETS_SECTION => {
                presets::draw_presets_controls(canvas, text, content_area(), true, active_preset, presets_scroll_offset);
            }
            _ => {}
        }
    }

    // --- Task 13: Save/Cancel/Reset button bar --- a horizontal divider directly above the bar
    // (same `divider` color the sidebar/content divider above already uses), then the three
    // buttons themselves.
    let body_bottom = (BODY_H as f32).min(h);
    if let Some(bar_divider) = Rect::from_xywh(0.0, body_bottom, w, 1.0f32.min((h - body_bottom).max(0.0))) {
        canvas.fill_rect(bar_divider, divider);
    }
    draw_button_bar(canvas, text, width as i32, height as i32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_at_matches_section_rect_for_every_section() {
        for i in 0..SECTIONS.len() {
            let r = section_rect(i);
            let cx = f64::from(r.left() + 1.0);
            let cy = f64::from(r.top() + 1.0);
            assert_eq!(
                section_at(cx, cy, PANEL_W as u32, PANEL_H as u32),
                Some(i),
                "point just inside section_rect({i}) should hit-test to section {i}"
            );
        }
    }

    #[test]
    fn section_at_returns_none_outside_sidebar() {
        assert_eq!(section_at(-5.0, 10.0, PANEL_W as u32, PANEL_H as u32), None);
        assert_eq!(section_at(f64::from(SIDEBAR_W + 50), 10.0, PANEL_W as u32, PANEL_H as u32), None);
        // Below the last section item.
        let last_bottom = section_rect(SECTIONS.len() - 1).bottom();
        assert_eq!(
            section_at(10.0, f64::from(last_bottom) + 100.0, PANEL_W as u32, PANEL_H as u32),
            None
        );
    }

    #[test]
    fn new_panel_starts_hidden_with_appearance_active() {
        let panel = Panel::new();
        assert!(!panel.is_visible());
        assert_eq!(panel.active_section, 0);
        assert!(panel.window_id().is_none());
    }

    // --- Tray-click/focus-loss race regression coverage -- see this module's doc comment
    // ("Tray-click/focus-loss race"). `Panel::toggle`/`Panel::show` need a real
    // `ActiveEventLoop`, which no `#[test]` here can construct (winit only hands one out inside
    // a running event loop's own callbacks) -- so these tests drive the panel's private state
    // directly (as `new_panel_starts_hidden_with_appearance_active` above already does) and
    // exercise `handle_focus_change`/`suppress_reopen`, the two functions that actually
    // implement the guard, exactly as `toggle`/`WindowEvent::Focused` would call them.

    #[test]
    fn within_grace_is_false_for_none_and_for_a_stale_timestamp() {
        assert!(!within_grace(None, TRAY_CLICK_GRACE));
        let stale = Instant::now() - (TRAY_CLICK_GRACE + Duration::from_millis(50));
        assert!(!within_grace(Some(stale), TRAY_CLICK_GRACE));
        assert!(within_grace(Some(Instant::now()), TRAY_CLICK_GRACE));
    }

    #[test]
    fn handle_focus_change_closes_normally_with_no_recent_tray_click() {
        // A genuine click-outside-the-panel close (unrelated to any tray click) must behave
        // exactly as before the fix: no guard fires, and the panel still closes.
        let mut panel = Panel::new();
        panel.visible = true;
        panel.shown_at = None; // past the post-show grace window

        panel.handle_focus_change(false);

        assert!(!panel.is_visible(), "an unrelated click-outside must still close the panel");
        assert!(panel.focus_lost_at.is_some(), "the close should be recorded as focus-loss-caused");
    }

    #[test]
    fn tray_click_reopen_is_suppressed_after_a_focus_loss_triggered_hide() {
        // Reproduces the diagnosed race directly: the panel is open, and a `Focused(false)`
        // caused by the OS's near-synchronous focus-steal on a tray click is processed BEFORE
        // that same click's `Click{Up}` event (relayed through `tray_icon`'s slower path -- see
        // this module's doc comment). Without the fix, `toggle()` would then see
        // `visible == false` and call `show()` again, reopening the panel the click meant to
        // close.
        let mut panel = Panel::new();
        panel.visible = true;
        panel.shown_at = None; // past the post-show grace window

        // Step 1: the focus-loss event arrives first and closes the panel.
        panel.handle_focus_change(false);
        assert!(!panel.is_visible(), "focus loss should close the panel");
        assert!(panel.focus_lost_at.is_some(), "the hide should be recorded as focus-loss-caused");

        // Step 2: the tray's Click{Up} event arrives afterward. `toggle()` itself needs a real
        // `ActiveEventLoop` this test can't construct, but its reopen decision is exactly
        // `suppress_reopen` -- exercising it directly proves `toggle()` would skip `show()`
        // here instead of reopening.
        assert!(
            panel.suppress_reopen(),
            "a reopen within TRAY_CLICK_GRACE of a focus-loss-triggered hide must be suppressed"
        );
        assert!(
            !panel.is_visible(),
            "panel must remain closed -- exactly one transition, no spurious reopen"
        );
        assert!(panel.focus_lost_at.is_none(), "the guard should be consumed once used");
    }

    #[test]
    fn tray_click_reopen_proceeds_once_the_focus_loss_grace_window_has_passed() {
        // A focus-loss-triggered hide that happened well outside `TRAY_CLICK_GRACE` (e.g. the
        // panel was closed by an unrelated click-outside minutes ago, then the user separately
        // clicks the tray icon to reopen it) must NOT be suppressed -- otherwise every reopen
        // after any click-outside close would silently break.
        let mut panel = Panel::new();
        panel.visible = false;
        panel.focus_lost_at = Some(Instant::now() - (TRAY_CLICK_GRACE + Duration::from_millis(50)));

        assert!(
            !panel.suppress_reopen(),
            "a stale focus-loss timestamp must not suppress an unrelated later reopen"
        );
    }

    #[test]
    fn compute_position_opens_upward_when_icon_is_near_bottom_of_screen() {
        // A tray icon near the bottom-right of a 1920x1080 screen (the common Windows taskbar
        // system-tray placement) should open the panel ABOVE the icon, not below (which would
        // push it off-screen).
        let anchor = TrayAnchor { x: 1880.0, y: 1060.0, width: 20.0, height: 20.0 };
        let pos = compute_position(Some(anchor), PANEL_W, PANEL_H, Some(PhysicalSize::new(1920, 1080)));
        assert!(
            pos.y < 1060,
            "panel should open above a bottom-of-screen tray icon, got y={}",
            pos.y
        );
        // Stays fully on-screen.
        assert!(pos.x >= 0 && pos.x + PANEL_W <= 1920);
        assert!(pos.y >= 0 && pos.y + PANEL_H <= 1080);
    }

    #[test]
    fn compute_position_opens_downward_when_icon_is_near_top_of_screen() {
        // A menu-bar icon near the top of the screen (the macOS convention) should open the
        // panel BELOW the icon.
        let anchor = TrayAnchor { x: 1880.0, y: 4.0, width: 20.0, height: 20.0 };
        let pos = compute_position(Some(anchor), PANEL_W, PANEL_H, Some(PhysicalSize::new(1920, 1080)));
        assert!(pos.y >= 4, "panel should open below a top-of-screen icon, got y={}", pos.y);
    }

    #[test]
    fn compute_position_falls_back_to_a_screen_corner_with_no_anchor() {
        let pos = compute_position(None, PANEL_W, PANEL_H, Some(PhysicalSize::new(1920, 1080)));
        assert!(pos.x >= 0 && pos.x + PANEL_W <= 1920);
        assert!(pos.y >= 0 && pos.y + PANEL_H <= 1080);
    }

    #[test]
    fn compute_position_never_panics_with_no_monitor_info() {
        let pos = compute_position(None, PANEL_W, PANEL_H, None);
        // Just confirms no panic/degenerate NaN-driven output; the exact fallback value is an
        // implementation detail already covered by the other `compute_position` tests.
        assert!(pos.x >= 0);
        assert!(pos.y >= 0);
    }

    // --- Task 13: Save/Cancel/Reset button bar ---

    #[test]
    fn button_at_hits_reset_cancel_save_and_nothing_outside_them() {
        let (reset, cancel, save) = button_rects(PANEL_W, PANEL_H);
        let center = |r: LRect| ((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0);

        let (rx, ry) = center(reset);
        assert_eq!(button_at(rx, ry, reset, cancel, save), Some(ButtonAction::Reset));
        let (cx, cy) = center(cancel);
        assert_eq!(button_at(cx, cy, reset, cancel, save), Some(ButtonAction::Cancel));
        let (sx, sy) = center(save);
        assert_eq!(button_at(sx, sy, reset, cancel, save), Some(ButtonAction::Save));

        // The sidebar area (top-left corner) hits no button.
        assert_eq!(button_at(0.0, 0.0, reset, cancel, save), None);
    }

    #[test]
    fn button_rects_reset_is_left_of_cancel_is_left_of_save_all_below_body_h() {
        let (reset, cancel, save) = button_rects(PANEL_W, PANEL_H);
        assert!(reset.right <= cancel.left, "Reset must sit left of Cancel, no overlap");
        assert!(cancel.right <= save.left, "Cancel must sit left of Save, no overlap");
        assert!(reset.top >= BODY_H as f32, "buttons must be drawn below the unchanged content-area height");
        assert!(save.right <= PANEL_W as f32, "Save must not overflow the panel's right edge");
    }

    #[test]
    fn commit_draft_updates_committed_and_hides_without_touching_disk() {
        // No `ccum_core::settings::save` call anywhere in this test -- see `commit_draft`'s own
        // doc comment for why this method exists (a real Save DOES call `save`, exercised only
        // by native-Windows-run verification, never by `cargo test`, so routine test runs on any
        // dev machine never write to that machine's real settings.json).
        let mut panel = Panel::new();
        let mut s = Settings::default();
        s.geometry.height = 99;
        panel.draft = Some(s);
        panel.visible = true;

        let result = panel.commit_draft();

        assert_eq!(result.as_ref().map(|c| c.geometry.height), Some(99));
        assert_eq!(panel.committed.as_ref().map(|c| c.geometry.height), Some(99));
        assert!(!panel.is_visible(), "commit_draft (Save's effect) must hide the panel");
    }

    #[test]
    fn commit_draft_returns_none_when_draft_was_never_seeded() {
        let mut panel = Panel::new();
        assert!(panel.commit_draft().is_none());
    }

    #[test]
    fn cancel_reverts_draft_to_committed_and_hides() {
        let mut panel = Panel::new();
        let committed = Settings::default();
        let mut edited = committed.clone();
        edited.geometry.height = 123;
        panel.committed = Some(committed.clone());
        panel.draft = Some(edited);
        panel.visible = true;

        let result = panel.handle_button_action(ButtonAction::Cancel);

        assert!(result.is_none(), "Cancel must never return Some -- nothing is persisted");
        assert!(!panel.is_visible(), "Cancel must close the panel");
        assert_eq!(
            panel.draft.as_ref().map(|d| d.geometry.height),
            Some(committed.geometry.height),
            "Cancel must discard draft's in-progress edit, reverting to committed"
        );
    }

    #[test]
    fn reset_rebuilds_draft_from_default_preserving_position_of_committed_not_of_draft() {
        // Distinguishes committed vs. draft as Reset's source: committed's tray_offset (42) must
        // survive Reset; draft's own unsaved edit to that same field (999) must NOT.
        let mut panel = Panel::new();
        let mut committed = Settings::default();
        committed.tray_offset = 42;
        let mut edited = committed.clone();
        edited.tray_offset = 999; // an in-progress, unsaved edit to a state-derived field
        edited.geometry.height = 12345; // an in-progress, unsaved appearance-ish edit
        panel.committed = Some(committed.clone());
        panel.draft = Some(edited);
        panel.visible = true;

        let result = panel.handle_button_action(ButtonAction::Reset);

        assert!(result.is_none(), "Reset must never return Some -- nothing is persisted");
        assert!(panel.is_visible(), "Reset must NOT close the panel");
        let draft = panel.draft.as_ref().expect("Reset must leave draft seeded");
        assert_eq!(
            draft.tray_offset, 42,
            "Reset must preserve committed's state-derived fields, not draft's in-progress edit"
        );
        assert_eq!(
            draft.geometry.height,
            Settings::default().geometry.height,
            "Reset discards appearance/geometry edits (from either committed or draft) back to Settings::default()"
        );
    }

    #[test]
    fn reset_is_a_noop_when_committed_was_never_seeded() {
        let mut panel = Panel::new();
        panel.visible = true;
        let result = panel.handle_button_action(ButtonAction::Reset);
        assert!(result.is_none());
        assert!(panel.draft.is_none(), "no committed settings to reset from -- draft stays unseeded");
    }
}
