use std::{os::windows::process::CommandExt, process::{Child, Command, Stdio}};

pub(super) fn hide(command: &mut Command) {
    command.creation_flags(0x08000000);
}

pub(super) fn stop_tree(child: &mut Child) {
    let _ = Command::new("taskkill.exe")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .creation_flags(0x08000000)
        .stdout(Stdio::null()).stderr(Stdio::null()).status();
}
