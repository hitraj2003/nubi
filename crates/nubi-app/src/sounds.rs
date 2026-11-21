//! Original Web Audio chimes rendered offline; independent playback permits overlap.
use std::mem::size_of;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    Media::Audio::{
        waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader,
        waveOutWrite, CALLBACK_FUNCTION, HWAVEOUT, WAVEFORMATEX, WAVEHDR, WAVE_FORMAT_PCM,
        WAVE_MAPPER,
    },
    System::Threading::{CreateEventW, SetEvent, WaitForSingleObject},
};

fn asset(kind: &str) -> &'static [u8] {
    match kind {
        "done" => include_bytes!("../assets/chimes/done.wav"),
        "question" => include_bytes!("../assets/chimes/question.wav"),
        "permission" => include_bytes!("../assets/chimes/permission.wav"),
        "idle" => include_bytes!("../assets/chimes/idle.wav"),
        _ => include_bytes!("../assets/chimes/info.wav"),
    }
}

pub fn chime(kind: &str) {
    start(kind, std::time::Duration::ZERO);
}

// The original overlay delays request chimes alongside its card lookup.
pub fn overlay_chime(kind: &str) {
    let delay = if matches!(kind, "permission" | "question") {
        std::time::Duration::from_millis(400)
    } else {
        std::time::Duration::ZERO
    };
    start(kind, delay);
}

fn start(kind: &str, delay: std::time::Duration) {
    let bytes = asset(kind);
    if let Err(error) = std::thread::Builder::new()
        .name("nubi-chime".into())
        .spawn(move || {
            std::thread::sleep(delay);
            if let Err(error) = play(bytes) {
                eprintln!("[nubi] chime: {error}");
            }
        })
    {
        eprintln!("[nubi] chime thread: {error}");
    }
}

// The callback never touches the audio buffer. Its event stays alive until device close.
unsafe extern "system" fn completed(_: HWAVEOUT, message: u32, event: usize, _: usize, _: usize) {
    const WOM_DONE: u32 = 0x3bd;
    if message == WOM_DONE {
        // SAFETY: waveOutOpen receives this live event, retained through waveOutClose.
        unsafe {
            SetEvent(event as HANDLE);
        }
    }
}

fn play(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() != 96044
        || &bytes[..4] != b"RIFF"
        || &bytes[8..16] != b"WAVEfmt "
        || &bytes[36..40] != b"data"
    {
        return Err("invalid bundled chime".into());
    }
    let mut pcm = bytes[44..].to_vec();
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 1,
        nSamplesPerSec: 48000,
        nAvgBytesPerSec: 96000,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    // SAFETY: null security/name, auto-reset event; handle ownership stays on this thread.
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    if event.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut device = std::ptr::null_mut();
    // SAFETY: format and output pointer valid; callback signature is WAVECALLBACK.
    let opened = unsafe {
        waveOutOpen(
            &mut device,
            WAVE_MAPPER,
            &format,
            completed as *const () as usize,
            event as usize,
            CALLBACK_FUNCTION,
        )
    };
    if opened != 0 {
        // SAFETY: failed open owns no device or callback; event belongs to this thread.
        unsafe {
            CloseHandle(event);
        }
        return Err(format!("waveOutOpen: {opened}"));
    }
    let mut header = Box::new(WAVEHDR {
        lpData: pcm.as_mut_ptr(),
        dwBufferLength: pcm.len() as u32,
        dwBytesRecorded: 0,
        dwUser: 0,
        dwFlags: 0,
        dwLoops: 0,
        lpNext: std::ptr::null_mut(),
        reserved: 0,
    });
    let header_size = size_of::<WAVEHDR>() as u32;
    // SAFETY: boxed header and PCM allocation do not move or resize until unprepared.
    let prepared = unsafe { waveOutPrepareHeader(device, &mut *header, header_size) };
    let mut result = if prepared != 0 {
        Err(format!("waveOutPrepareHeader: {prepared}"))
    } else {
        // SAFETY: prepared header and its backing PCM remain owned and alive.
        let written = unsafe { waveOutWrite(device, &mut *header, header_size) };
        if written != 0 {
            Err(format!("waveOutWrite: {written}"))
        }
        // SAFETY: event remains valid, wait runs on the audio worker, never the UI thread.
        else if unsafe { WaitForSingleObject(event, 3000) } != WAIT_OBJECT_0 {
            Err("chime playback timed out".into())
        } else {
            Ok(())
        }
    };
    if prepared == 0 {
        // SAFETY: stop/return any pending buffer before trying to release its memory.
        unsafe {
            waveOutReset(device);
        }
        // SAFETY: same live header/device used for preparation; reset attempted above.
        let released = unsafe { waveOutUnprepareHeader(device, &mut *header, header_size) };
        if released != 0 {
            // A broken driver may still own the allocations. Retain them and both handles
            // rather than freeing memory or closing a callback event still in use.
            std::mem::forget((header, pcm));
            return Err(format!(
                "waveOutUnprepareHeader: {released}; driver resources retained"
            ));
        }
    }
    // SAFETY: no prepared buffers remain; close device before releasing callback event.
    let closed = unsafe { waveOutClose(device) };
    if closed == 0 {
        // SAFETY: successfully closed device cannot call the callback again.
        unsafe {
            CloseHandle(event);
        }
    } else {
        result = Err(format!("waveOutClose: {closed}; callback event retained"));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_chime_assets_are_valid_distinct_and_have_finished_tails() {
        let kinds = ["done", "question", "permission", "idle", "info"];
        for (index, kind) in kinds.iter().enumerate() {
            let wav = asset(kind);
            assert_eq!(wav.len(), 96044);
            assert_eq!(
                &wav[20..36],
                &[1, 0, 1, 0, 128, 187, 0, 0, 0, 119, 1, 0, 2, 0, 16, 0]
            );
            assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 96000);
            assert!(wav[44..].iter().any(|byte| *byte != 0));
            assert!(
                wav[wav.len() - 9600..].iter().all(|byte| *byte == 0),
                "{kind} tail"
            );
            for other in &kinds[..index] {
                assert_ne!(wav, asset(other));
            }
        }
        assert_eq!(asset("unknown"), asset("info"));
    }
    #[test]
    #[ignore = "Requires an audio device; plays all five chimes for owner verification"]
    fn play_original_chimes_on_audio_device() {
        for kind in ["done", "question", "permission", "idle", "info"] {
            println!("Playing {kind}");
            play(asset(kind)).unwrap();
        }
    }
}
