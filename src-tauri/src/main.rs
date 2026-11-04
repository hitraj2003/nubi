// Nubi: Claude Code + Codex overlay for Windows.
// One exe, two modes: GUI (default) and `--hook <claude|codex>` (short-lived hook client).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod updater;
mod hook_client;
mod hooks_config;
mod island;
mod notch;
mod pipe;
mod poller;
mod protocol;
mod ring;
mod taskbar;
mod settings;
mod state;
mod usage;
mod winutil;

use std::sync::Mutex;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Hook mode runs before any Tauri/WebView start-up.
    if args.get(1).map(String::as_str) == Some("--hook") {
        let provider = args.get(2).map(String::as_str).unwrap_or("claude");
        std::process::exit(hook_client::run(provider));
    }
    // `--statusline`: Claude Code status line (usage numbers without an API call).
    if args.get(1).map(String::as_str) == Some("--statusline") {
        std::process::exit(hook_client::statusline());
    }
    // `--diag`: print usage and hook status (never prints tokens).
    if args.get(1).map(String::as_str) == Some("--diag") {
        for p in ["claude", "codex"] {
            let (u, _) = usage::apply(p, usage::fetch(p), None);
            println!("{p}: hooks={} usage={}", hooks_config::is_enabled(p), serde_json::to_string(&u).unwrap_or_default());
        }
        return;
    }

    tauri::Builder::default()
        // Only one copy runs: the pipe can have one owner. A second launch opens the running
        // copy's panel and exits. Must be the first plugin. Hook mode returned above, so hook
        // calls never reach this.
        .plugin(tauri_plugin_single_instance::init(|a, _args, _cwd| app::show_panel(a.clone())))
        .plugin(app::shortcut_plugin())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app::Shared(Mutex::new(state::Inner::default())))
        .invoke_handler(tauri::generate_handler![
            app::get_state,
            app::answer_permission,
            app::answer_question,
            app::jump_session,
            app::save_settings,
            app::set_dnd,
            app::set_hooks,
            app::test_alert,
            app::refresh_usage,
            app::hide_overlay,
            app::show_panel,
            app::monitors,
            updater::check_update,
            updater::app_version,
            notch::report_hot,
            notch::report_overlay_hot,
            notch::report_island_hot,
            island::island_offset,
            app::show_requests,
            app::log_js,
            app::report_issue,
            notch::notch_drag,
            notch::notch_drag_end,
            ring::ring_move,
            ring::ring_drop,
            ring::ring_tip,
        ])
        .setup(|a| {
            app::setup(a.handle())?;
            updater::start(a.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Nubi");
}
