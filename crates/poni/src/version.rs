//! Version and branding information for Koygi Island by Ryo.

pub const VERSION: &str = "1.0.0";
pub const APP_NAME: &str = "Koygi Island";
pub const DEVELOPER: &str = "Ryo";

/// "About Koygi 1.0.0 by Ryo"
pub fn about_label() -> String {
    format!("About {APP_NAME} {VERSION} by {DEVELOPER}")
}

/// The page the About line opens.
pub fn notes_url() -> String {
    "https://github.com/koygi/koygi/releases".to_string()
}
