//! Windows adaptation: foreground an existing Antigravity desktop window.
//! Never launch a process, and never report a foreground request as success
//! until Windows confirms the selected window is actually foreground.
use std::{ffi::c_void, path::Path};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
};

type Window = *mut c_void;
#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(Window, isize) -> i32, data: isize) -> i32;
    fn IsWindowVisible(window: Window) -> i32;
    fn GetWindowThreadProcessId(window: Window, process: *mut u32) -> u32;
    fn GetForegroundWindow() -> Window;
    fn SetForegroundWindow(window: Window) -> i32;
    fn IsIconic(window: Window) -> i32;
    fn ShowWindow(window: Window, command: i32) -> i32;
}
pub(crate) const ACTIVATES_APPS: bool = true;
const ANTIGRAVITY: &str = "com.google.antigravity";

fn antigravity_executable(path: &str) -> bool {
    Path::new(path).file_name().and_then(|s| s.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("Antigravity.exe"))
}
unsafe extern "system" fn collect(window: Window, data: isize) -> i32 {
    if unsafe { IsWindowVisible(window) } == 0 { return 1; }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() { return 1; }
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    let found = unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) } != 0;
    unsafe { CloseHandle(process) };
    if found && antigravity_executable(&String::from_utf16_lossy(&path[..length as usize])) {
        // The caller owns this Vec for the entire synchronous EnumWindows call.
        unsafe { &mut *(data as *mut Vec<Window>) }.push(window);
    }
    1
}
fn windows() -> Vec<Window> {
    let mut result = Vec::<Window>::new();
    if unsafe { EnumWindows(collect, &mut result as *mut _ as isize) } == 0 {
        result.clear();
    }
    result
}
pub(crate) fn app_running(bundle_id: &str) -> bool {
    // Other clients preserve their existing attempt-navigation behavior.
    bundle_id != ANTIGRAVITY || !windows().is_empty()
}
pub(crate) fn activate_app(bundle_id: &str) -> bool {
    if bundle_id != ANTIGRAVITY { return false; }
    let candidates = windows();
    let foreground = unsafe { GetForegroundWindow() };
    if candidates.len() != 1 { return false; }
    if candidates.contains(&foreground) { return true; }
    // A CDP page does not identify its native HWND. Do not pick an arbitrary
    // project when several Antigravity windows exist.
    let [window] = candidates.as_slice() else { return false; };
    unsafe {
        if IsIconic(*window) != 0 { ShowWindow(*window, 9); }
        SetForegroundWindow(*window);
        GetForegroundWindow() == *window
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn executable_matching_is_exact_and_case_insensitive() {
        assert!(antigravity_executable(r"C:\Users\测试\App Data\Antigravity.exe"));
        assert!(antigravity_executable(r"D:\Apps\ANTIGRAVITY.EXE"));
        assert!(!antigravity_executable(r"C:\Apps\not-Antigravity.exe"));
        assert!(!antigravity_executable(r"C:\Antigravity.exe\other.exe"));
    }
}
