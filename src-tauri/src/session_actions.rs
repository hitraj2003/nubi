//! Actions bound to a native agent process, never its terminal host.
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub pid: u32,
    pub born: u64,
    pub provider: String,
}

struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn identity(pid: u32) -> Result<(Process, u64), String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return Err("The session process is unavailable.".into());
        }
        let h = Process(h);
        let mut created: FILETIME = std::mem::zeroed();
        let mut other: [FILETIME; 3] = std::mem::zeroed();
        let mut code = 0;
        if GetProcessTimes(
            h.0,
            &mut created,
            &mut other[0],
            &mut other[1],
            &mut other[2],
        ) == 0
            || GetExitCodeProcess(h.0, &mut code) == 0
            || code != 259
        {
            return Err("The session process has exited.".into());
        }
        let born = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        Ok((h, born))
    }
}

fn matches_agent(provider: &str, image: &str) -> bool {
    let name = image.rsplit(['\\', '/']).next().unwrap_or("");
    match provider {
        "claude" => name.eq_ignore_ascii_case("claude.exe"),
        "codex" => name.eq_ignore_ascii_case("codex.exe"),
        _ => false,
    }
}

pub fn find_target(provider: &str, pids: &[u32]) -> Option<Target> {
    pids.iter().find_map(|&pid| {
        if !matches_agent(provider, &crate::winutil::process_image(pid)?) {
            return None;
        }
        let (_, born) = identity(pid).ok()?;
        Some(Target {
            pid,
            born,
            provider: provider.into(),
        })
    })
}

impl Target {
    pub fn available(&self) -> bool {
        self.verify().is_ok()
    }

    fn verify(&self) -> Result<Process, String> {
        let (h, born) = identity(self.pid)?;
        let image = crate::winutil::process_image(self.pid)
            .ok_or("Cannot identify the session process.")?;
        if born != self.born || !matches_agent(&self.provider, &image) {
            return Err(
                "The original session has ended. Start a new session to use this action.".into(),
            );
        }
        Ok(h)
    }

    pub fn close(&self) -> Result<(), String> {
        // Keep the verified process handle open while taskkill targets its tree.
        let _process = self.verify()?;
        let output = Command::new(system_exe("taskkill.exe")?)
            .args(["/PID", &self.pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("Could not close this session. {detail}"));
        }
        Ok(())
    }

    pub fn compact(&self) -> Result<(), String> {
        let _process = self.verify()?;
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args([
                "--compact-session",
                &self.pid.to_string(),
                &self.born.to_string(),
                &self.provider,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        for _ in 0..100 {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return if status.success() {
                    Ok(())
                } else {
                    Err("Could not send /compact to this console. Use Jump and run /compact in the terminal.".into())
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err("The console did not respond to /compact.".into())
    }
}

fn system_exe(name: &str) -> Result<std::path::PathBuf, String> {
    Ok(std::path::PathBuf::from(
        std::env::var_os("SystemRoot").ok_or("Windows directory is unavailable.")?,
    )
    .join("System32")
    .join(name))
}

pub fn new_terminal(cwd: &str) -> Result<(), String> {
    let path = std::path::Path::new(cwd);
    if !path.is_absolute() || !path.is_dir() {
        return Err("The session folder is no longer available.".into());
    }
    // Argument arrays preserve spaces and shell metacharacters in the directory.
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let wt = std::path::PathBuf::from(local).join("Microsoft/WindowsApps/wt.exe");
        if wt.is_file()
            && Command::new(wt)
                .args(["-w", "new", "-d", cwd])
                .current_dir(path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .is_ok()
        {
            return Ok(());
        }
    }
    // A visible window is intentional: the user explicitly requested a new terminal.
    Command::new(system_exe("WindowsPowerShell/v1.0/powershell.exe")?)
        .args(["-NoLogo", "-NoExit"])
        .current_dir(path)
        .creation_flags(0x00000010)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// A short-lived process attaches to exactly one console. No foreground keystrokes.
pub fn compact_helper(args: &[String]) -> Result<(), String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{
        AttachConsole, FreeConsole, GetConsoleProcessList, WriteConsoleInputW,
    };
    let target = Target {
        pid: args
            .get(2)
            .and_then(|s| s.parse().ok())
            .ok_or("Missing process ID")?,
        born: args
            .get(3)
            .and_then(|s| s.parse().ok())
            .ok_or("Missing process identity")?,
        provider: args.get(4).cloned().ok_or("Missing provider")?,
    };
    let _process = target.verify()?;
    unsafe {
        FreeConsole();
        if AttachConsole(target.pid) == 0 {
            return Err("Cannot attach to the session console".into());
        }
    }
    let mut pids = vec![0u32; 256];
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) } as usize;
    if count == 0 || count > pids.len() {
        return Err("Cannot identify the console clients".into());
    }
    if pids[..count].iter().any(|&pid| {
        pid != target.pid
            && crate::winutil::process_image(pid)
                .map(|image| matches_agent("claude", &image) || matches_agent("codex", &image))
                .unwrap_or(false)
    }) {
        return Err("Several agents share this console; use /compact in the terminal".into());
    }
    let input = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")
        .map_err(|e| e.to_string())?;
    let records = compact_records();
    let mut written = 0;
    let ok = unsafe {
        WriteConsoleInputW(
            input.as_raw_handle() as HANDLE,
            records.as_ptr(),
            records.len() as u32,
            &mut written,
        )
    };
    if ok == 0 || written != records.len() as u32 {
        return Err("Console input failed".into());
    }
    Ok(())
}

fn compact_records() -> Vec<windows_sys::Win32::System::Console::INPUT_RECORD> {
    use windows_sys::Win32::System::Console::{
        INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0,
    };
    "/compact\r"
        .encode_utf16()
        .flat_map(|c| {
            [1, 0].map(|down| INPUT_RECORD {
                EventType: KEY_EVENT as u16,
                Event: INPUT_RECORD_0 {
                    KeyEvent: KEY_EVENT_RECORD {
                        bKeyDown: down,
                        wRepeatCount: 1,
                        wVirtualKeyCode: if c == 13 { 13 } else { 0 },
                        wVirtualScanCode: 0,
                        uChar: KEY_EVENT_RECORD_0 { UnicodeChar: c },
                        dwControlKeyState: 0,
                    },
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_native_provider_processes_match() {
        for provider in ["claude", "codex"] {
            assert!(matches_agent(
                provider,
                &format!("C:\\Program Files\\{provider}.exe")
            ));
            for image in [
                "node.exe",
                "powershell.exe",
                "WindowsTerminal.exe",
                "Code.exe",
                "nubi.exe",
                "codex-helper.exe",
            ] {
                assert!(!matches_agent(provider, image));
            }
        }
        assert!(!matches_agent("claude", "codex.exe"));
        assert!(!matches_agent("codex", "claude.exe"));
        assert!(!matches_agent("other", "other.exe"));
    }
    #[test]
    fn rejects_unrelated_live_process() {
        let pid = std::process::id();
        let (_, born) = identity(pid).unwrap();
        assert!(Target {
            pid,
            born,
            provider: "codex".into()
        }
        .verify()
        .is_err());
        assert!(find_target("claude", &[pid]).is_none());
    }
    #[test]
    fn compact_is_only_fixed_command_with_enter() {
        let records = compact_records();
        let text: String = records
            .iter()
            .step_by(2)
            .map(|r| unsafe { char::from_u32(r.Event.KeyEvent.uChar.UnicodeChar as u32).unwrap() })
            .collect();
        assert_eq!(text, "/compact\r");
        assert_eq!(records.len(), 18);
    }

    #[test]
    #[ignore = "subprocess fixture, launched only by closes_only_the_bound_process"]
    fn agent_fixture() {
        if std::env::var_os("NUBI_TEST_AGENT_FIXTURE").is_some() {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    }

    #[test]
    fn closes_only_the_bound_process() {
        let dir = std::env::temp_dir().join(format!("nubi-agent-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("codex.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let mut child = Command::new(&exe)
            .args([
                "--ignored",
                "--exact",
                "session_actions::tests::agent_fixture",
            ])
            .env("NUBI_TEST_AGENT_FIXTURE", "1")
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let target = find_target("codex", &[child.id()]).expect("Fixture must be identifiable");
        assert!(find_target("claude", &[child.id()]).is_none());
        let stale = Target {
            born: target.born + 1,
            ..target.clone()
        };
        assert!(stale.close().is_err());
        assert!(
            child.try_wait().unwrap().is_none(),
            "Stale identity must not kill a live process"
        );
        let result = target.close();
        if result.is_err() {
            let _ = child.kill();
        }
        child.wait().unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert!(!target.available());
        // Remove only the two explicitly created test artifacts.
        std::fs::remove_file(&exe).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
