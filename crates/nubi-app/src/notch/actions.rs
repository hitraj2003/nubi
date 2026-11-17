use super::*;
pub fn session_action(host: &NativeHost, key: &str, action: &str) -> Result<(), String> {
    match action {
        "jump" => {
            let pids = with(host, |state| {
                state.sessions.get(key).map(|session| session.pids.clone())
            })
            .ok_or("unknown session")?;
            if !nubi_core::winutil::focus_pids(&pids) {
                return Err("Terminal window not found".into());
            }
        }
        "new" => {
            let cwd = with(host, |state| {
                state
                    .sessions
                    .get(key)
                    .and_then(|session| session.cwd.clone())
            })
            .ok_or("This session has no recorded folder.")?;
            nubi_core::session_actions::new_terminal(&cwd)?;
        }
        "close" => {
            let target = with(host, |state| {
                state
                    .sessions
                    .get(key)
                    .and_then(|session| session.action_target.clone())
            })
            .ok_or("Cannot identify this agent process. Use Jump to close it in the terminal.")?;
            target.close()?;
            with(host, |state| {
                state.sessions.remove(key);
                state
                    .pending
                    .retain(|_, pending| pending.session_key != key);
                state.missed.retain(|missed| missed.session_key != key);
            });
        }
        "cancel-compact" => with(host, |state| {
            if let Some(session) = state.sessions.get_mut(key) {
                session.compact_queued = false;
            }
        }),
        "compact" => {
            let target = with(host, |state| {
                let session = state.sessions.get_mut(key).ok_or("Unknown session")?;
                if session.compact_queued {
                    return Ok(None);
                }
                let target = session.action_target.clone().ok_or("Cannot identify this agent process. Use Jump and run /compact in the terminal.")?;
                session.compact_queued = true;
                session.action_error = None;
                Ok::<_, String>(Some(target))
            })?;
            if let Some(target) = target {
                let host = host.clone();
                let key = key.to_string();
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_millis(500));
                        let available = target.available();
                        let ready = with(&host, |state| {
                            let pending = state
                                .pending
                                .values()
                                .any(|pending| pending.session_key == key);
                            let session = state.sessions.get_mut(&key)?;
                            if !session.compact_queued
                                || session.action_target.as_ref() != Some(&target)
                            {
                                return None;
                            }
                            if !available {
                                session.compact_queued = false;
                                session.action_error = Some(
                                    "Queued compact cancelled: the session process has exited."
                                        .into(),
                                );
                                return None;
                            }
                            if !pending
                                && matches!(session.state.as_str(), "idle" | "done")
                                && now_ms().saturating_sub(session.last_event_ms) >= 500
                            {
                                session.compact_queued = false;
                                Some(true)
                            } else {
                                Some(false)
                            }
                        });
                        host.emit_state();
                        match ready {
                            None => return,
                            Some(false) => continue,
                            Some(true) => break,
                        }
                    }
                    let result = target.compact();
                    with(&host, |state| {
                        if let Some(session) = state.sessions.get_mut(&key) {
                            if session.action_target.as_ref() == Some(&target) {
                                session.action_error = result.err();
                            }
                        }
                    });
                    host.emit_state();
                });
            }
        }
        _ => return Err("Unknown session action".into()),
    }
    host.emit_state();
    Ok(())
}
