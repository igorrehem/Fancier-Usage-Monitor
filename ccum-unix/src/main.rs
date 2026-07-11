// ccum-unix: macOS + Linux entry point for Claude Code Usage Monitor.
//
// Task 6 scope: wire up the actual CPU-rasterized rendering pipeline everything else in this
// crate will build on -- a `render::Canvas` (tiny-skia `Pixmap` wrapper) built fresh every
// `WindowEvent::RedrawRequested`, painted entirely off-screen via `render::paint`, then
// presented to the window's real surface in a single shot through `softbuffer`. This is the
// same "build off-screen, present once" double-buffering discipline
// `ccum-windows/src/window.rs`'s flicker fix (commit `2b6d4f3` on `main`) established for the
// GDI implementation -- see `render/mod.rs`'s `Canvas` doc comment for the full reasoning.
//
// Prior task (4) scope was a blank window only; `tray.rs`/settings-persistence wiring are
// still later Phase 2 tasks (see the design spec's crate layout) and are NOT part of this
// file yet.
//
// winit 0.30's API: an `ApplicationHandler` impl driven by `EventLoop::run_app`, replacing
// the older `EventLoop::run(closure)` API (deprecated in 0.30, removed later). See
// https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html.
//
// softbuffer 0.4's API: a `Context<D>` (bound to a display handle) plus a `Surface<D, W>`
// (bound to a window handle) created from it; `Surface::resize` before every present (its
// size must always match the window's current size), `Surface::buffer_mut` to get a
// `&mut [u32]` to write pixels into, `Buffer::present` to hand them to the platform's
// windowing system. The window is wrapped in `Rc<Window>` because both `Context` and
// `Surface` need their own handle to it (the standard `softbuffer` + `winit` pairing
// pattern -- confirmed by reading `softbuffer-0.4.8`'s own source: `Context<D>`/`Surface<D,
// W>` are generic over anything implementing `raw_window_handle`'s
// `HasDisplayHandle`/`HasWindowHandle`, which `Rc<Window>` satisfies via `winit`'s blanket
// impls, same as a bare `Window` would, but shareable).

mod render;

use std::num::NonZeroU32;
use std::rc::Rc;

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use render::text::TextRenderer;

struct App {
    window: Option<Rc<Window>>,
    // Never read after `resumed` stores it -- kept alive only because `surface` was created
    // from it and, per softbuffer's documented contract, the `Context` a `Surface` was built
    // from must outlive that `Surface`. `#[allow(dead_code)]` because rustc's `dead_code`
    // lint only sees "written, never read" and can't see the drop-order requirement.
    #[allow(dead_code)]
    context: Option<Context<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    /// Persistent across frames -- see `render::text::TextRenderer`'s doc comment for why
    /// this must NOT be rebuilt inside `Canvas`/every redraw.
    text: TextRenderer,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            context: None,
            surface: None,
            text: TextRenderer::new(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once (e.g. on some platforms after a suspend/resume
        // cycle), so only create the window/context/surface the first time.
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes().with_title("Claude Code Usage Monitor");
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => {
                eprintln!("ccum-unix: failed to create window: {err}");
                event_loop.exit();
                return;
            }
        };

        let context = match Context::new(window.clone()) {
            Ok(context) => context,
            Err(err) => {
                eprintln!("ccum-unix: failed to create softbuffer context: {err}");
                event_loop.exit();
                return;
            }
        };

        let surface = match Surface::new(&context, window.clone()) {
            Ok(surface) => surface,
            Err(err) => {
                eprintln!("ccum-unix: failed to create softbuffer surface: {err}");
                event_loop.exit();
                return;
            }
        };

        window.request_redraw();
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                // The surface itself is resized lazily in `redraw` (right before painting,
                // using the window's current size at that moment) -- just ask for a repaint.
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }
}

impl App {
    /// Builds one full frame into an off-screen `Canvas`, then presents it to the window
    /// surface in one shot. No drawing call in `render::paint` (or anything it calls) ever
    /// touches `surface`/the softbuffer-mapped pixels directly -- that separation is the
    /// entire point of the double-buffering fix this task is porting from `ccum-windows`.
    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };

        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            // Zero-sized window (e.g. minimized on some platforms) -- nothing to paint.
            return;
        };

        if let Err(err) = surface.resize(width, height) {
            eprintln!("ccum-unix: failed to resize softbuffer surface: {err}");
            return;
        }

        let Some(mut canvas) = render::Canvas::new(width.get(), height.get()) else {
            return;
        };
        render::paint(&mut canvas, &mut self.text);

        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(err) => {
                eprintln!("ccum-unix: failed to acquire softbuffer buffer: {err}");
                return;
            }
        };

        // softbuffer's buffer format is opaque `0x00RRGGBB` per pixel; tiny-skia's Pixmap is
        // premultiplied RGBA8. `render::paint` always starts each frame with
        // `Canvas::clear`, so every pixel ends up fully opaque (alpha == 255) by the time we
        // get here -- a premultiplied color at alpha 255 is numerically identical to its
        // straight-alpha RGB, so dropping the alpha byte here is exact for this app's
        // always-opaque window, not an approximation.
        for (dst, src) in buffer.iter_mut().zip(canvas.pixmap().pixels().iter()) {
            *dst = (u32::from(src.red()) << 16) | (u32::from(src.green()) << 8) | u32::from(src.blue());
        }

        if let Err(err) = buffer.present() {
            eprintln!("ccum-unix: failed to present softbuffer buffer: {err}");
        }
    }
}

fn main() {
    // Sanity touch of ccum-core: confirms the workspace path dependency resolves and
    // type-checks from ccum-unix, matching the pattern ccum-windows already uses for its own
    // ccum-core dependency. Real settings load/persist wiring is a later task
    // (settings_paths.rs's XDG/Library-path resolution hasn't been written yet).
    let _ = ccum_core::settings::Settings::default();

    let event_loop = EventLoop::new().expect("ccum-unix: failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app).expect("ccum-unix: event loop exited with an error");
}
