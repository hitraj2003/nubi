#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod host;
mod launcher;
mod motion;
mod native;
mod notifications;
mod panel_logic;
mod shape;
mod singleton;
mod spring;
mod tray;
mod ui;
mod update_transport;
mod updater;

slint::include_modules!();

use nubi_core::{hook_client, native_host, session_actions};

fn main() {
    if let Err(error) = run() {
        eprintln!("[nubi] {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments: Vec<String> = std::env::args().collect();
    if arguments.iter().any(|argument| argument == "--dev") {
        nubi_core::isolation::enable_development();
        arguments.retain(|argument| argument != "--dev");
    }
    // Every command-line service mode bypasses GUI ownership and toolkit startup.
    match arguments.get(1).map(String::as_str) {
        Some("--hook") => std::process::exit(hook_client::run(
            arguments.get(2).map(String::as_str).unwrap_or("claude"),
        )),
        Some("--statusline") => std::process::exit(hook_client::statusline()),
        Some("--diag") => {
            for provider in ["claude", "codex"] {
                let (usage, _) =
                    nubi_core::usage::apply(provider, nubi_core::usage::fetch(provider), None);
                println!(
                    "{provider}: hooks={} usage={}",
                    nubi_core::hooks_config::is_enabled(provider),
                    serde_json::to_string(&usage).unwrap_or_default()
                );
            }
            return Ok(());
        }
        Some("--compact-session") => {
            std::process::exit(if session_actions::compact_helper(&arguments).is_ok() {
                0
            } else {
                1
            })
        }
        Some(origin) if native_host::is_native_call(origin) => {
            std::process::exit(native_host::run(origin))
        }
        _ => {}
    }
    let mut action = None;
    let mut exit_after = None;
    for argument in arguments.iter().skip(1) {
        match argument.as_str() {
            "--settings" => action = Some(singleton::Action::Settings),
            "--launcher" => action = Some(singleton::Action::Launcher),
            _ if argument.starts_with("--exit-after=") && nubi_core::isolation::development() => {
                exit_after = Some(std::time::Duration::from_secs(argument[13..].parse()?));
            }
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let Some(_owner) = singleton::enter(&runtime, action.unwrap_or(singleton::Action::Settings))?
    else {
        return Ok(());
    };
    let (host, events) = host::NativeHost::new();
    let (shutdown, stop) = tokio::sync::oneshot::channel::<()>();
    let worker_host = host.clone();
    let handle = runtime.handle().clone();
    let services = std::thread::spawn(move || {
        runtime.block_on(async move {
            use nubi_core::host::Host;
            let pipe_host = worker_host.clone();
            tokio::spawn(nubi_core::pipe::serve(pipe_host));
            let activation_host = worker_host.clone();
            tokio::spawn(async move {
                if let Err(error) = singleton::serve(activation_host.clone()).await {
                    activation_host.set_pipe_error(error);
                }
            });
            nubi_core::poller::start(worker_host.clone());
            updater::start(worker_host);
            let _ = stop.await;
        });
        runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    });
    let result = ui::run(host, events, handle, action, exit_after);
    let _ = shutdown.send(());
    let _ = services.join();
    result
}
