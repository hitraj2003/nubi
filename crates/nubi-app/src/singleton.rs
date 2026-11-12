//! Secured, versioned activation independent of the hook protocol.
use crate::host::{NativeHost, UiEvent};
use nubi_core::pipe::{self, PipeSecurity};
use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ClientOptions;
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Action {
    Settings,
    Launcher,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Activation {
    version: u32,
    action: Action,
}

fn name() -> String {
    format!("{}-activate-v1", nubi_core::protocol::pipe_name())
}

pub struct Owner(HANDLE);
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

/// Ownership is acquired on the GUI thread and held until all UI services stop.
pub fn enter(runtime: &tokio::runtime::Runtime, action: Action) -> Result<Option<Owner>, String> {
    let sid = pipe::current_user_sid().ok_or("cannot read current user SID")?;
    let suffix = if nubi_core::isolation::development() {
        "-dev"
    } else {
        ""
    };
    enter_at(
        runtime,
        action,
        &format!(r"Global\nubi-native-{sid}{suffix}"),
        &name(),
    )
}

fn enter_at(
    runtime: &tokio::runtime::Runtime,
    action: Action,
    mutex_name: &str,
    pipe_name: &str,
) -> Result<Option<Owner>, String> {
    let sid = pipe::current_user_sid().ok_or("cannot read current user SID")?;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    let sddl = wide(&format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)"));
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("cannot build mutex security descriptor".into());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let mutex_name = wide(mutex_name);
    let handle = unsafe { CreateMutexW(&attributes, 0, mutex_name.as_ptr()) };
    unsafe {
        windows_sys::Win32::Foundation::LocalFree(descriptor);
    }
    if handle.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let start = Instant::now();
    loop {
        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => return Ok(Some(Owner(handle))),
            WAIT_TIMEOUT => {}
            _ => {
                unsafe {
                    CloseHandle(handle);
                }
                return Err("cannot acquire GUI mutex".into());
            }
        }
        if runtime.block_on(forward(action, pipe_name)).is_ok() {
            unsafe {
                CloseHandle(handle);
            }
            return Ok(None);
        }
        if start.elapsed() >= Duration::from_secs(15) {
            unsafe {
                CloseHandle(handle);
            }
            return Err("running Nubi did not become ready within 15 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn forward(action: Action, pipe_name: &str) -> Result<(), String> {
    let exchange = async {
        let mut client = ClientOptions::new()
            .open(pipe_name)
            .map_err(|e| e.to_string())?;
        let mut pid = 0;
        if unsafe { GetNamedPipeServerProcessId(client.as_raw_handle() as HANDLE, &mut pid) } == 0
            || !pipe::process_allowed(pid, false)
        {
            return Err("activation server executable rejected".into());
        }
        // The launching process may grant its foreground permission to the owner.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(pid);
        }
        let mut frame =
            serde_json::to_vec(&Activation { version: 1, action }).map_err(|e| e.to_string())?;
        frame.push(b'\n');
        client.write_all(&frame).await.map_err(|e| e.to_string())?;
        let mut reply = [0; 3];
        client
            .read_exact(&mut reply)
            .await
            .map_err(|e| e.to_string())?;
        if &reply == b"OK\n" {
            Ok(())
        } else {
            Err("activation rejected".into())
        }
    };
    tokio::time::timeout(Duration::from_secs(1), exchange)
        .await
        .map_err(|_| "activation timed out".to_string())?
}

pub async fn serve(host: NativeHost) -> Result<(), String> {
    serve_at(host, name()).await
}

async fn serve_at(host: NativeHost, name: String) -> Result<(), String> {
    let security = PipeSecurity::current_user()?;
    let mut server = security.create(&name, true).map_err(|e| e.to_string())?;
    loop {
        server.connect().await.map_err(|e| e.to_string())?;
        // Keep an unconnected instance alive before dispatching an activation.
        let next = security.create(&name, false).map_err(|e| e.to_string())?;
        let host = host.clone();
        tokio::spawn(async move {
            let mut pid = 0;
            if unsafe { GetNamedPipeClientProcessId(server.as_raw_handle() as HANDLE, &mut pid) }
                == 0
                || !pipe::process_allowed(pid, false)
            {
                return;
            }
            let (read, mut write) = tokio::io::split(server);
            let mut bytes = Vec::new();
            let mut reader = BufReader::new(read).take(1024);
            let read =
                tokio::time::timeout(Duration::from_secs(2), reader.read_until(b'\n', &mut bytes))
                    .await;
            if !matches!(read, Ok(Ok(n)) if n > 0 && n < 1024 && bytes.last() == Some(&b'\n')) {
                return;
            }
            if let Ok(frame) = parse(&bytes) {
                if host.post(UiEvent::Activate(frame.action)).is_ok() {
                    let _ = write.write_all(b"OK\n").await;
                }
            }
        });
        server = next;
    }
}

fn parse(bytes: &[u8]) -> Result<Activation, String> {
    let frame: Activation = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if frame.version != 1 {
        return Err("unsupported activation version".into());
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn readiness_retry_forwards_action_to_validated_owner() {
        let unique = format!("nubi-activation-test-{}", std::process::id());
        let mutex = format!(r"Local\{unique}-readiness");
        let pipe = format!(r"\\.\pipe\{unique}-readiness");
        let (ready, started) = std::sync::mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let (host, events) = NativeHost::from_settings(Default::default());
        let owner_mutex = mutex.clone();
        let owner_pipe = pipe.clone();
        let owner = std::thread::spawn(move || {
            let runtime = runtime();
            let _owner = enter_at(&runtime, Action::Settings, &owner_mutex, &owner_pipe)
                .unwrap()
                .unwrap();
            ready.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(250));
            runtime.block_on(async { tokio::select! { result = serve_at(host, owner_pipe) => panic!("{result:?}"), _ = stopped => {} } });
        });
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(enter_at(&runtime(), Action::Launcher, &mutex, &pipe)
            .unwrap()
            .is_none());
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(2)),
            Ok(UiEvent::Activate(Action::Launcher))
        ));
        stop.send(()).unwrap();
        owner.join().unwrap();
    }

    #[test]
    fn abandoned_owner_during_readiness_is_recovered() {
        let mutex = format!(
            r"Local\nubi-activation-test-{}-abandoned",
            std::process::id()
        );
        let pipe = format!(
            r"\\.\pipe\nubi-activation-test-{}-abandoned",
            std::process::id()
        );
        let owner_mutex = mutex.clone();
        let owner_pipe = pipe.clone();
        let (ready, started) = std::sync::mpsc::channel();
        let owner = std::thread::spawn(move || {
            let guard = enter_at(&runtime(), Action::Settings, &owner_mutex, &owner_pipe)
                .unwrap()
                .unwrap();
            // Keep a kernel handle alive while the owning thread dies without releasing it.
            let raw = guard.0 as usize;
            std::mem::forget(guard);
            ready.send(raw).unwrap();
            std::thread::sleep(Duration::from_millis(250));
        });
        let raw = started.recv_timeout(Duration::from_secs(5)).unwrap();
        let recovered = enter_at(&runtime(), Action::Settings, &mutex, &pipe).unwrap();
        assert!(recovered.is_some());
        owner.join().unwrap();
        drop(recovered);
        unsafe {
            CloseHandle(raw as HANDLE);
        }
    }

    #[test]
    fn executable_peer_validation_rejects_unknown_process() {
        assert!(pipe::process_allowed(std::process::id(), false));
        assert!(!pipe::process_allowed(u32::MAX, false));
    }
    #[test]
    fn activation_rejects_versions_and_unrecognized_actions() {
        assert_eq!(
            parse(br#"{"version":1,"action":"Settings"}"#)
                .unwrap()
                .action,
            Action::Settings
        );
        assert!(parse(br#"{"version":2,"action":"Settings"}"#).is_err());
        assert!(parse(br#"{"version":1,"action":"Quit"}"#).is_err());
        assert!(parse(br#"{"version":1,"action":"Settings","command":"anything"}"#).is_err());
    }
}
