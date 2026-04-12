// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let scheduler_daemon = std::env::args()
        .skip(1)
        .any(|arg| arg == "--scheduler-daemon");

    if scheduler_daemon {
        if let Err(error) = app_lib::run_scheduler_daemon() {
            eprintln!("scheduler daemon failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    app_lib::run();
}
