//! WinRT apartment lifetime for calls made by the pipe's worker thread.
windows_link::link!("combase.dll" "system" fn RoInitialize(kind: u32) -> i32);
windows_link::link!("combase.dll" "system" fn RoUninitialize());

struct Apartment(bool);
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                RoUninitialize();
            }
        }
    }
}

pub fn show(title: &str, body: &str) -> Result<(), String> {
    // RO_INIT_MULTITHREADED. If a caller already selected STA, retain that apartment.
    let result = unsafe { RoInitialize(1) };
    let _apartment = Apartment(result >= 0);
    if result < 0 && result != 0x80010106u32 as i32 {
        return Err(format!("WinRT initialization failed: {result:#x}"));
    }
    let id = if nubi_core::isolation::development() {
        "dev.nubi.app.dev"
    } else {
        "dev.nubi.app"
    };
    tauri_winrt_notification::Toast::new(id)
        .title(title)
        .text1(body)
        .show()
        .map_err(|error| error.to_string())
}
