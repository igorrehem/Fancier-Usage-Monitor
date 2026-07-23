#![windows_subsystem = "windows"]

mod config_window;
mod controls;
mod localization;
mod native_interop;
mod theme;
mod tray_icon;
mod updater;
mod window;

use ccum_core::diagnose;

fn main() {
    diagnose::install_panic_hook();

    let args: Vec<String> = std::env::args().collect();
    diagnose::journal(format!("process started args={args:?}"));
    let diagnose_enabled = args.iter().any(|arg| arg == "--diagnose");
    if diagnose_enabled {
        match diagnose::init() {
            Ok(path) => diagnose::log(format!("startup args={args:?} log_path={}", path.display())),
            Err(error) => {
                // Logging may not be available yet, but keep startup behavior unchanged.
                let _ = error;
            }
        }
    }

    if let Some(exit_code) = updater::handle_cli_mode(&args) {
        if diagnose_enabled {
            diagnose::log(format!("cli mode exited with code {exit_code}"));
        }
        std::process::exit(exit_code);
    }

    if diagnose_enabled {
        diagnose::log("entering window::run");
    }
    window::run();
}
