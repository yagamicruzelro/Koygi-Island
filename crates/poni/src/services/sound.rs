//! System sounds for timers and reminders.

use crate::commands::Sound;
use windows::core::w;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};

#[link(name = "user32")]
extern "system" {
    fn MessageBeep(utype: u32) -> i32;
}

pub fn play(s: Sound) {
    let alias = match s {
        Sound::Chime => w!("Notification.Reminder"),
        Sound::Tick => w!("SystemAsterisk"),
    };
    unsafe {
        if !PlaySoundW(alias, None, SND_ALIAS | SND_ASYNC | SND_NODEFAULT).as_bool()
            && !PlaySoundW(w!("SystemNotification"), None, SND_ALIAS | SND_ASYNC | SND_NODEFAULT).as_bool()
            && !PlaySoundW(w!("SystemAsterisk"), None, SND_ALIAS | SND_ASYNC | SND_NODEFAULT).as_bool()
            && !PlaySoundW(w!("SystemDefault"), None, SND_ALIAS | SND_ASYNC).as_bool()
        {
            let _ = MessageBeep(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_play() {
        play(Sound::Chime);
    }
}

