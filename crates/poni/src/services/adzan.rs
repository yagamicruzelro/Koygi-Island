//! Plays the Prayer times add-on's adzan (`adzan.m4a`) with WinRT's
//! `MediaPlayer`. Its automatic media controls are switched off, so the
//! adzan never shows up as a playing song: not in Windows' volume flyout,
//! and not on Oyenza Studio's own Music page.
//!
//! One adzan at a time; a new one replaces it. `AppEvent::AdzanEnded` comes
//! when it finishes or fails, from a thread-pool thread.

use crate::events::{AppEvent, EventSink};
use std::path::Path;
use windows::core::HSTRING;
use windows::Foundation::{TypedEventHandler, Uri};
use windows::Media::Core::MediaSource;
use windows::Media::Playback::MediaPlayer;

#[derive(Default)]
pub struct AdzanPlayer {
    player: Option<MediaPlayer>,
}

impl AdzanPlayer {
    /// Start the recording at `path`. An error means nothing plays (the
    /// caller plays the tone instead).
    pub fn play(&mut self, path: &Path, sink: &EventSink) -> Result<(), String> {
        self.stop();
        if !path.is_file() || path.metadata().map_or(0, |m| m.len()) < 1024 {
            return Err(format!("adzan file invalid or missing: {}", path.display()));
        }
        let uri = Uri::CreateUri(&HSTRING::from(file_uri(path))).map_err(|e| format!("bad path ({e})"))?;
        let player = MediaPlayer::new().map_err(|e| format!("no media player ({e})"))?;
        // No media session: the adzan isn't a song.
        if let Ok(cm) = player.CommandManager() {
            let _ = cm.SetIsEnabled(false);
        }
        let source = MediaSource::CreateFromUri(&uri).map_err(|e| format!("can't open the recording ({e})"))?;
        player.SetSource(&source).map_err(|e| format!("can't open the recording ({e})"))?;
        let ended = sink.clone();
        let _ = player.MediaEnded(&TypedEventHandler::new(move |_, _| {
            ended.send(AppEvent::AdzanEnded);
            Ok(())
        }));
        let failed = sink.clone();
        let _ = player.MediaFailed(&TypedEventHandler::new(move |_, _| {
            crate::pwarn!("adzan: playback failed; playing tone instead");
            crate::services::sound::play(crate::commands::Sound::Chime);
            failed.send(AppEvent::AdzanEnded);
            Ok(())
        }));
        player.Play().map_err(|e| format!("can't play ({e})"))?;
        self.player = Some(player);
        Ok(())
    }

    /// Stop it now (Stop in the notice, the add-on switched off), or forget a
    /// player that finished.
    pub fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            let _ = p.Pause();
            let _ = p.Close();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.player.is_some()
    }
}

/// `C:\Users\A B\x.m4a` -> `file:///C:/Users/A%20B/x.m4a` (Oyen's voice too).
pub(crate) fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy();
    let parts: Vec<String> = text
        .split(['\\', '/'])
        .filter(|s| !s.is_empty())
        .enumerate()
        .map(|(i, s)| if i == 0 && s.ends_with(':') { s.to_string() } else { poni_core::urlenc::encode(s) })
        .collect();
    format!("file:///{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_keep_the_drive_and_encode_the_rest() {
        assert_eq!(
            file_uri(Path::new(r"C:\Users\Luca Cada\AppData\Local\Poni\addons\sholat\addon-sholat-1\adzan.m4a")),
            "file:///C:/Users/Luca%20Cada/AppData/Local/Poni/addons/sholat/addon-sholat-1/adzan.m4a"
        );
        assert_eq!(file_uri(Path::new(r"D:\Müzik\a.m4a")), "file:///D:/M%C3%BCzik/a.m4a");
    }

    #[test]
    fn missing_adzan_file_returns_error() {
        let path = Path::new("non_existent_adzan_path.m4a");
        let (tx, _rx) = std::sync::mpsc::channel();
        let sink = EventSink::new(tx, crate::events::SendHwnd(0));
        let mut player = AdzanPlayer::default();
        let res = player.play(path, &sink);
        assert!(res.is_err(), "missing file must return error immediately");
    }

    #[test]
    fn valid_adzan_file_plays_and_can_be_stopped() {
        let path = crate::app::App::adzan_audio_path().or_else(|| {
            let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../addons/sholat/adzan.m4a");
            p.is_file().then_some(p)
        });
        let Some(path) = path else {
            return;
        };
        let (tx, _rx) = std::sync::mpsc::channel();
        let sink = EventSink::new(tx, crate::events::SendHwnd(0));
        let mut player = AdzanPlayer::default();
        let res = player.play(&path, &sink);
        assert!(res.is_ok(), "playback should start: {res:?}");
        assert!(player.is_playing());
        player.stop();
        assert!(!player.is_playing());
    }

    #[test]
    fn zero_byte_or_corrupted_adzan_file_returns_error() {
        let dir = std::env::temp_dir().join("poni_test_adzan");
        let _ = std::fs::create_dir_all(&dir);
        let empty_path = dir.join("empty_adzan.m4a");
        let _ = std::fs::write(&empty_path, b"");
        let (tx, _rx) = std::sync::mpsc::channel();
        let sink = EventSink::new(tx, crate::events::SendHwnd(0));
        let mut player = AdzanPlayer::default();
        let res = player.play(&empty_path, &sink);
        let _ = std::fs::remove_file(&empty_path);
        assert!(res.is_err(), "zero-byte file must return error immediately");
    }
}

