use std::ffi::OsStr;
use std::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;

#[cfg(windows)]
pub fn command(program: impl AsRef<OsStr>) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
pub fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

#[cfg(windows)]
pub fn attach_parent_console() -> bool {
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 }
}

#[cfg(not(windows))]
pub fn attach_parent_console() -> bool {
    false
}

#[cfg(windows)]
pub fn console_handle() -> Option<std::fs::File> {
    std::fs::OpenOptions::new().write(true).open("CONOUT$").ok()
}

#[cfg(not(windows))]
pub fn console_handle() -> Option<std::fs::File> {
    None
}

pub fn open_folder(path: &std::path::Path) {
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    let opener = "xdg-open";
    #[cfg(any(target_os = "android", target_arch = "wasm32"))]
    {
        let _ = path;
        return;
    }
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    {
        let _ = command(opener).arg(path).spawn();
    }
}
