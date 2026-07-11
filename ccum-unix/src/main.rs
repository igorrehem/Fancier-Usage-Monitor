// ccum-unix: macOS + Linux entry point for Claude Code Usage Monitor.
//
// Task 4 scope ONLY: prove that the workspace + ccum-core + winit/tiny-skia/tray-icon/
// cosmic-text all resolve and type-check together, via the smallest possible skeleton --
// construct an `EventLoop`, create one blank window, run until the window is closed. There
// is no usage-monitor rendering, no tray icon, and no settings-persistence wiring here yet;
// those land in later Phase 2 tasks (`render.rs`, `tray.rs`, `settings_paths.rs` per the
// design spec's crate layout). `tiny-skia` and `cosmic-text` are pulled in as Cargo
// dependencies already (see Cargo.toml for the reasoning) but deliberately UNUSED by this
// file's code -- wiring them up is later work, not this task's.
//
// winit 0.30's API: an `ApplicationHandler` impl driven by `EventLoop::run_app`, replacing
// the older `EventLoop::run(closure)` API (deprecated in 0.30, removed later). See
// https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html.

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct App {
    window: Option<Window>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once (e.g. on some platforms after a suspend/resume
        // cycle), so only create the window the first time.
        if self.window.is_none() {
            let attributes =
                Window::default_attributes().with_title("Claude Code Usage Monitor");
            match event_loop.create_window(attributes) {
                Ok(window) => self.window = Some(window),
                Err(err) => {
                    eprintln!("ccum-unix: failed to create window: {err}");
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }
}

fn main() {
    // Sanity touch of ccum-core: confirms the workspace path dependency resolves and
    // type-checks from ccum-unix, matching the pattern ccum-windows already uses for its
    // own ccum-core dependency. Nothing further -- real settings load/persist wiring is a
    // later task (settings_paths.rs's XDG/Library-path resolution hasn't been written yet).
    let _ = ccum_core::settings::Settings::default();

    let event_loop = EventLoop::new().expect("ccum-unix: failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app).expect("ccum-unix: event loop exited with an error");
}
