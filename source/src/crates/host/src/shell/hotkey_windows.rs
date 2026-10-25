//! Local Windows parity: Ctrl+Alt+H toggles the HUD glow, like Cmd+Option+H.
use std::{sync::mpsc, thread};
use tauri::Manager;
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT},
        WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT},
    },
};

struct Shortcut {
    id: u32,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Shortcut {
    fn drop(&mut self) {
        // The registration creates the thread's message queue before ready is sent.
        if unsafe { PostThreadMessageW(self.id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok() {
            if let Some(worker) = self.worker.take() { let _ = worker.join(); }
        }
    }
}

pub fn install(app: &tauri::AppHandle) {
    if app.try_state::<Shortcut>().is_some() { return; }
    let handle = app.clone();
    let (ready, waiting) = mpsc::sync_channel(1);
    let worker = thread::Builder::new().name("agenthud-hotkey".into()).spawn(move || {
        if let Err(error) = unsafe { RegisterHotKey(None, 1, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, b'H' as u32) } {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
        let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
            if message.message == WM_HOTKEY && message.wParam.0 == 1 {
                crate::coordinator::toggle_glow(&handle);
            }
        }
        let _ = unsafe { UnregisterHotKey(None, 1) };
    });
    match worker {
        Ok(worker) => match waiting.recv() {
            Ok(Ok(id)) => { app.manage(Shortcut { id, worker: Some(worker) }); }
            result => { eprintln!("Ctrl+Alt+H could not be registered: {result:?}"); let _ = worker.join(); }
        },
        Err(error) => eprintln!("Global shortcut worker could not start: {error}"),
    }
}
