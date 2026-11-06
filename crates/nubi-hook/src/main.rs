//! Standalone hook/status-line client. Every unsupported mode fails open.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("--hook") => {
            nubi_core::hook_client::run(args.get(2).map(String::as_str).unwrap_or("claude"))
        }
        Some("--statusline") => nubi_core::hook_client::statusline(),
        _ => 0,
    };
    // A deadline can leave a pipe worker blocked. Exit the process without joining it.
    std::process::exit(code);
}
