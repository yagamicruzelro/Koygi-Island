//! macOS shell helpers: opening URLs, files, apps and console commands.

use std::path::Path;

/// Open a URL, file path or custom URI scheme using macOS default handler.
pub fn open(target: &str) {
    crate::mac::services::open(target);
}

/// Open Discord app or deep-link channel URL on macOS.
pub fn open_discord_app(target: &str) {
    crate::mac::services::open(target);
}

/// Send shortcut to Discord seamlessly (Windows uses Win32 SendInput).
pub fn send_discord_shortcut_seamless(_ctrl: bool, _shift: bool, _vk: u16) {
    // macOS sandboxing restricts synthetic global keystrokes without Accessibility permissions.
}

/// Open macOS Login Items settings in System Settings.
pub fn open_startup_settings() {
    crate::mac::services::open("x-apple.systempreferences:com.apple.LoginItems-Settings.extension");
}

/// Open an action link for the recorder.
pub fn open_recorder_link(link: &str) {
    crate::mac::services::open(link);
}

/// Open the Claude desktop app via custom URI scheme.
pub fn open_claude_app() {
    crate::mac::services::open("claude://");
}

/// Run an interactive CLI command in macOS Terminal.app.
pub fn run_in_console(program: &Path, args: &str) {
    let script = format!(
        "tell application \"Terminal\" to do script \"{} {}\"",
        program.display(),
        args
    );
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .spawn();
}
