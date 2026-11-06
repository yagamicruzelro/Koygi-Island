//! Discord Voice Activity & Microphone Input Monitor.
//!
//! 1. Parses Discord's renderer log (%APPDATA%\Discord\logs\renderer_js.log)
//!    incrementally in real-time (<60ms) to track active WebRTC voice channel
//!    connection, guild ID, channel ID, and mute status across ALL servers.
//! 2. Uses Windows Core Audio API (IAudioMeterInformation) to sample real-time
//!    microphone peak audio level (0.0..1.0) so the equalizer animated wave
//!    reacts directly to actual human voice input, not a static sine wave.
//!
//! Emits AppEvent::Discord when in-voice state or speaking status changes.

use crate::events::{AppEvent, EventSink};
use crate::state::DiscordVoiceState;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;
use windows::core::Result;
use windows::Win32::Foundation::{CloseHandle, BOOL, E_FAIL, HANDLE, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};

#[link(name = "kernel32")]
extern "system" {
    fn ReadProcessMemory(
        hprocess: HANDLE,
        lpbaseaddress: *const std::ffi::c_void,
        lpbuffer: *mut std::ffi::c_void,
        nsize: usize,
        lpnumberofbytesread: *mut usize,
    ) -> BOOL;
}
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{eCapture, eConsole, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
};

static CURRENT_MIC_PEAK: AtomicU32 = AtomicU32::new(0);

// Global persistent cache for server and channel names across sessions
static SERVER_NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static CHANNEL_NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn names_file_path() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let dir = PathBuf::from(local).join("Poni");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("discord_names.json"))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct DiscordNamesConfig {
    #[serde(default)]
    servers: HashMap<String, String>,
    #[serde(default)]
    channels: HashMap<String, String>,
}

fn init_name_caches() {
    let mut s = SERVER_NAMES.lock().unwrap();
    let mut c = CHANNEL_NAMES.lock().unwrap();
    if s.is_some() && c.is_some() {
        return;
    }

    let mut sm = HashMap::new();
    let mut cm = HashMap::new();

    // Default built-in server mappings
    sm.insert("881625323463147640".into(), "Hisyam Clan".into());
    sm.insert("835574753032667176".into(), "nu aing".into());
    sm.insert("937335410252738570".into(), "AWOOOOOOOOOOOOOOOOO".into());
    sm.insert("1373991818894508062".into(), "Discord Voice".into());
    sm.insert("1516131238967247040".into(), "Discord Voice".into());

    // Default built-in channel mappings
    cm.insert("1283511866588659742".into(), "Umat Hisyam".into());
    cm.insert("835574753468612620".into(), "Study/Recording Room".into());
    cm.insert("1477003693252411482".into(), "penghuni soerga".into());

    // Load custom names from disk if present
    if let Some(path) = names_file_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<DiscordNamesConfig>(&content) {
                for (k, v) in cfg.servers {
                    sm.insert(k, v);
                }
                for (k, v) in cfg.channels {
                    cm.insert(k, v);
                }
            }
        }
    }

    *s = Some(sm);
    *c = Some(cm);
    drop(s);
    drop(c);
    save_names_to_disk();
}

fn save_names_to_disk() {
    let s = SERVER_NAMES.lock().unwrap();
    let c = CHANNEL_NAMES.lock().unwrap();
    if let (Some(sm), Some(cm)) = (s.as_ref(), c.as_ref()) {
        if let Some(path) = names_file_path() {
            let cfg = DiscordNamesConfig {
                servers: sm.clone(),
                channels: cm.clone(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}

pub fn get_server_name(guild_id: &str) -> Option<String> {
    init_name_caches();
    SERVER_NAMES.lock().unwrap().as_ref()?.get(guild_id).cloned()
}

pub fn get_channel_name(channel_id: &str) -> Option<String> {
    init_name_caches();
    CHANNEL_NAMES.lock().unwrap().as_ref()?.get(channel_id).cloned()
}

pub fn set_channel_name(channel_id: String, name: String) {
    init_name_caches();
    let mut changed = false;
    if let Some(c) = CHANNEL_NAMES.lock().unwrap().as_mut() {
        if c.get(&channel_id) != Some(&name) {
            c.insert(channel_id, name);
            changed = true;
        }
    }
    if changed {
        save_names_to_disk();
    }
}

pub fn set_server_name(guild_id: String, name: String) {
    init_name_caches();
    let mut changed = false;
    if let Some(s) = SERVER_NAMES.lock().unwrap().as_mut() {
        if s.get(&guild_id) != Some(&name) {
            s.insert(guild_id, name);
            changed = true;
        }
    }
    if changed {
        save_names_to_disk();
    }
}

/// Dynamically discovers server and channel names from Discord window titles
pub fn resolve_discord_names_from_windows(guild_id: &str, channel_id: &str) {
    if guild_id.is_empty() && channel_id.is_empty() {
        return;
    }

    struct WindowFinder {
        found_titles: Vec<String>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let finder = &mut *(lparam.0 as *mut WindowFinder);
        if IsWindowVisible(hwnd).as_bool() {
            let len = GetWindowTextLengthW(hwnd);
            if len > 0 && len < 512 {
                let mut buf = vec![0u16; len as usize + 1];
                let copied = GetWindowTextW(hwnd, &mut buf);
                if copied > 0 {
                    let title = String::from_utf16_lossy(&buf[..copied as usize]);
                    if title.contains("Discord") || title.contains("discord") {
                        finder.found_titles.push(title);
                    }
                }
            }
        }
        BOOL(1)
    }

    let mut finder = WindowFinder {
        found_titles: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut finder as *mut _ as isize));
    }

    for title in finder.found_titles {
        let mut trimmed = title.trim();
        if let Some(rest) = trimmed.strip_prefix('•') {
            trimmed = rest.trim();
        }
        if trimmed.starts_with('(') {
            if let Some(idx) = trimmed.find(')') {
                trimmed = trimmed[idx + 1..].trim();
            }
        }
        if let Some(pos) = trimmed.rfind(" - Discord") {
            let before = trimmed[..pos].trim();
            if before.contains('|') {
                let parts: Vec<&str> = before.split('|').collect();
                if parts.len() >= 2 {
                    let srv = parts[1].trim();
                    if !srv.is_empty() && !guild_id.is_empty() {
                        set_server_name(guild_id.to_string(), srv.to_string());
                    }
                }
            } else if !before.is_empty() && !guild_id.is_empty() {
                set_server_name(guild_id.to_string(), before.to_string());
            }
        }
    }
}

/// Returns current instantaneous microphone audio peak level (0.0..1.0).
pub fn current_mic_peak() -> f32 {
    f32::from_bits(CURRENT_MIC_PEAK.load(Ordering::Relaxed))
}

pub struct DiscordService {
    tx: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl DiscordService {
    pub fn start(sink: EventSink) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("poni-discord".into())
            .spawn(move || {
                let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                worker(sink, rx);
                unsafe { CoUninitialize() };
            })
            .map_err(|e| windows::core::Error::new(E_FAIL, format!("discord thread: {e}")))?;
        Ok(Self { tx, thread: Some(thread) })
    }
}

impl Drop for DiscordService {
    fn drop(&mut self) {
        let _ = self.tx.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn discord_log_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let path = PathBuf::from(appdata).join("discord").join("logs").join("renderer_js.log");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn sample_wasapi_mic_peak() -> f32 {
    unsafe {
        let enumerator: IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(e) => e,
            Err(_) => return 0.0,
        };
        let device = match enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) {
            Ok(d) => d,
            Err(_) => return 0.0,
        };
        let meter: IAudioMeterInformation = match device.Activate(CLSCTX_ALL, None) {
            Ok(m) => m,
            Err(_) => return 0.0,
        };
        meter.GetPeakValue().unwrap_or(0.0)
    }
}

fn get_discord_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut pe = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut pe).is_ok() {
                loop {
                    let null_idx = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(pe.szExeFile.len());
                    let name = String::from_utf16_lossy(&pe.szExeFile[..null_idx]);
                    if name.eq_ignore_ascii_case("discord.exe") {
                        pids.push(pe.th32ProcessID);
                    }
                    if Process32NextW(snap, &mut pe).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
    }
    pids
}

/// Dynamically scans Discord renderer process memory to extract the real active voice channel name.
/// Searches for:
/// 1. `{channel_name} (voice channel)` in accessibility/DOM trees (with high score if followed by "call duration" or active users).
/// 2. `{channel_name} | {server_name}` or `{channel_name} / {server_name}`.
pub fn scan_discord_voice_channel_from_memory(server_name: &str) -> Option<String> {
    let pids = get_discord_pids();
    if pids.is_empty() {
        return None;
    }

    let mut scores: HashMap<String, usize> = HashMap::new();
    let vc_needle = b" (voice channel)";

    // Prepare server needles if known
    let srv_clean = server_name.trim();
    let has_real_server = !srv_clean.is_empty() && srv_clean != "Discord Server" && srv_clean != "Discord Voice";
    let pipe_needle = if has_real_server { Some(format!(" | {srv_clean}")) } else { None };
    let slash_needle = if has_real_server { Some(format!(" / {srv_clean}")) } else { None };
    let prefix_pipe = if has_real_server && srv_clean.len() >= 6 {
        Some(format!(" | {}", &srv_clean[..6]))
    } else {
        None
    };

    for pid in pids {
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, false, pid) else {
                continue;
            };

            let mut addr = 0usize;
            let mut mbi = MEMORY_BASIC_INFORMATION::default();

            while VirtualQueryEx(h, Some(addr as *const _), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) != 0 {
                // Check committed read/write memory pages (Discord V8 DOM heap)
                if mbi.State == MEM_COMMIT && (mbi.Protect.0 & 0x04 != 0) && mbi.RegionSize > 0 && mbi.RegionSize <= 10 * 1024 * 1024 {
                    let mut buf = vec![0u8; mbi.RegionSize];
                    let mut bytes_read = 0usize;
                    if ReadProcessMemory(h, mbi.BaseAddress, buf.as_mut_ptr() as *mut _, mbi.RegionSize, &mut bytes_read).as_bool() && bytes_read > 0 {
                        let slice = &buf[..bytes_read];

                        // 1. Scan for " (voice channel)"
                        let mut idx = 0;
                        while let Some(pos) = slice[idx..].windows(vc_needle.len()).position(|w| w == vc_needle) {
                            let match_idx = idx + pos;
                            let mut start = match_idx;
                            while start > 0 {
                                let b = slice[start - 1];
                                let valid = b.is_ascii_alphanumeric()
                                    || b == b' '
                                    || b == b'-'
                                    || b == b'_'
                                    || b == b'#'
                                    || b == b'\''
                                    || b == b'('
                                    || b == b')';
                                if !valid || (match_idx - start >= 35) {
                                    break;
                                }
                                start -= 1;
                            }
                            if start < match_idx {
                                if let Ok(s) = std::str::from_utf8(&slice[start..match_idx]) {
                                    let cand = s.trim().trim_start_matches(|ch: char| !ch.is_alphanumeric() && ch != '#');
                                    if cand.len() >= 2 && cand.len() <= 32 && !cand.contains("http") && !cand.contains("class") {
                                        let mut weight = 15;
                                        // Check context immediately following "(voice channel)"
                                        let end_check = (match_idx + vc_needle.len() + 160).min(slice.len());
                                        let after = &slice[match_idx + vc_needle.len()..end_check];
                                        if after.windows(13).any(|w| w == b"call duration") {
                                            weight += 3000;
                                        }
                                        if after.windows(5).any(|w| w == b"users") || after.windows(4).any(|w| w == b" of ") {
                                            weight += 800;
                                        }
                                        *scores.entry(cand.to_string()).or_insert(0) += weight;
                                    }
                                }
                            }
                            idx = match_idx + vc_needle.len();
                        }

                        // 2. Scan for pipe / slash needles
                        let check_needles: &[Option<&str>] = &[pipe_needle.as_deref(), slash_needle.as_deref(), prefix_pipe.as_deref()];
                        for needle_str in check_needles.iter().copied().flatten() {
                            let n_bytes = needle_str.as_bytes();
                            let mut n_idx = 0;
                            while let Some(pos) = slice[n_idx..].windows(n_bytes.len()).position(|w| w == n_bytes) {
                                let match_idx = n_idx + pos;
                                let mut start = match_idx;
                                while start > 0 {
                                    let b = slice[start - 1];
                                    let valid = b.is_ascii_alphanumeric()
                                        || b == b' '
                                        || b == b'-'
                                        || b == b'_'
                                        || b == b'#'
                                        || b == b'\'';
                                    if !valid || (match_idx - start >= 35) {
                                        break;
                                    }
                                    start -= 1;
                                }
                                if start < match_idx {
                                    if let Ok(s) = std::str::from_utf8(&slice[start..match_idx]) {
                                        let cand = s.trim().trim_start_matches(|ch: char| !ch.is_alphanumeric() && ch != '#');
                                        if cand.len() >= 2 && cand.len() <= 32 && !cand.contains("http") && !cand.contains("class") {
                                            *scores.entry(cand.to_string()).or_insert(0) += 60;
                                        }
                                    }
                                }
                                n_idx = match_idx + n_bytes.len();
                            }
                        }
                    }
                }
                addr = mbi.BaseAddress as usize + mbi.RegionSize;
                if addr >= 0x7FFF_FFFF_0000 {
                    break;
                }
            }

            let _ = CloseHandle(h);
        }
    }

    scores
        .into_iter()
        .max_by(|(c1, s1), (c2, s2)| s1.cmp(s2).then_with(|| c2.len().cmp(&c1.len())))
        .map(|(c, _)| c)
}

struct DiscordLogScanner {
    last_offset: u64,
    in_voice: bool,
    active_guild_id: String,
    active_channel_id: String,
    is_muted: bool,
    is_deafened: bool,
    last_resolved_channel_id: String,
    last_scan_instant: Option<std::time::Instant>,
    last_heartbeat_instant: Option<std::time::Instant>,
}

impl DiscordLogScanner {
    fn new() -> Self {
        Self {
            last_offset: 0,
            in_voice: false,
            active_guild_id: String::new(),
            active_channel_id: String::new(),
            is_muted: false,
            is_deafened: false,
            last_resolved_channel_id: String::new(),
            last_scan_instant: None,
            last_heartbeat_instant: None,
        }
    }

    fn scan(&mut self, log_path: &PathBuf) -> DiscordVoiceState {
        // 1. Process existence check: if Discord is not running, reset state
        let pids = get_discord_pids();
        if pids.is_empty() {
            self.in_voice = false;
            self.last_heartbeat_instant = None;
            return DiscordVoiceState::default();
        }

        let mut file = match File::open(log_path) {
            Ok(f) => f,
            Err(_) => return DiscordVoiceState::default(),
        };

        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len == 0 {
            return DiscordVoiceState::default();
        }

        // On initial startup or file recreation: scan up to the last 64 KB
        let start_pos = if self.last_offset == 0 || len < self.last_offset {
            len.saturating_sub(65536)
        } else {
            self.last_offset
        };

        if start_pos < len {
            if file.seek(SeekFrom::Start(start_pos)).is_ok() {
                let bytes_to_read = (len - start_pos) as usize;
                let mut buf = vec![0u8; bytes_to_read];
                if file.read_exact(&mut buf).is_ok() {
                    let text = String::from_utf8_lossy(&buf);
                    for line in text.lines() {
                        self.process_line(line);
                    }
                }
            }
            self.last_offset = len;
        }

        // 2. Heartbeat freshness check: WebRTC voice sessions send heartbeat every 15s.
        // If > 25 seconds elapsed since last heartbeat, connection is closed.
        if self.in_voice {
            let now = std::time::Instant::now();
            if self.last_heartbeat_instant.map_or(true, |t| now.duration_since(t) > Duration::from_secs(25)) {
                self.in_voice = false;
            }
        }

        if self.in_voice {
            if !self.active_guild_id.is_empty() || !self.active_channel_id.is_empty() {
                resolve_discord_names_from_windows(&self.active_guild_id, &self.active_channel_id);
            }

            let server = get_server_name(&self.active_guild_id).unwrap_or_else(|| {
                if !self.active_guild_id.is_empty() {
                    "Discord Server".into()
                } else {
                    "Discord Voice".into()
                }
            });

            // Fast channel resolution: use cached name immediately with 0ms delay.
            // Only spawn memory scan in a background thread if name is unknown/generic.
            let channel_cached = get_channel_name(&self.active_channel_id);
            let channel_is_generic = channel_cached.as_deref().map_or(true, |c| c == "Voice Channel" || c == "Voice Connected");

            if channel_is_generic && !self.active_channel_id.is_empty() {
                let now = std::time::Instant::now();
                let cooldown_passed = self.last_scan_instant.is_none_or(|t| now.duration_since(t).as_millis() >= 2000);
                if cooldown_passed {
                    self.last_scan_instant = Some(now);
                    let target_ch_id = self.active_channel_id.clone();
                    let srv_name = server.clone();
                    std::thread::Builder::new()
                        .name("discord-mem-scan".into())
                        .spawn(move || {
                            if let Some(real_name) = scan_discord_voice_channel_from_memory(&srv_name) {
                                set_channel_name(target_ch_id, real_name);
                            }
                        })
                        .ok();
                }
            }

            let channel = get_channel_name(&self.active_channel_id).unwrap_or_else(|| {
                if !self.active_channel_id.is_empty() {
                    "Voice Channel".into()
                } else {
                    "Voice Connected".into()
                }
            });

            DiscordVoiceState {
                in_voice: true,
                server,
                channel,
                guild_id: self.active_guild_id.clone(),
                channel_id: self.active_channel_id.clone(),
                is_speaking: false,
                is_muted: self.is_muted,
                is_deafened: self.is_deafened,
                mic_peak: 0.0,
            }
        } else {
            DiscordVoiceState::default()
        }
    }

    fn process_line(&mut self, line: &str) {
        // RTC Connection Events:
        if line.contains("Connecting to RTC server")
            || line.contains("Connected to RTC server.")
            || line.contains("RTC_CONNECTING => RTC_CONNECTED")
            || line.contains("RTC media connection state change: RTC_CONNECTING => CONNECTED")
            || line.contains("[RTCControlSocket(default)] Heartbeat ACK received")
        {
            self.in_voice = true;
            self.last_heartbeat_instant = Some(std::time::Instant::now());
        }

        // Disconnect Events (only for the main voice connection 'default', NEVER for stream):
        if line.contains("default)]") && (
            line.contains("Destroy RTCConnection")
            || line.contains("Disconnected from RTC server")
            || line.contains("[VOICE_DISCONNECT]")
            || line.contains("RTC_CONNECTED => RTC_DISCONNECTED")
            || line.contains("RTC_DISCONNECTED => RTC_DISCONNECTED")
            || line.contains("RTC_DISCONNECTED => DISCONNECTED")
            || line.contains("DISCONNECTED => DISCONNECTED")
        ) {
            self.in_voice = false;
            self.last_heartbeat_instant = None;
        }

        // Parse RTCConnection guild and channel IDs on connect or channel switch:
        // e.g.: [RTCConnection(881625323463147640, default)] Connecting to RTC server ..., channel: 1283511866588659742(2)
        // or:   [RTCConnection(881625323463147640, default)] Updating channel: 1283511866588659742(2)
        if line.contains("[RTCConnection(") && line.contains("default)]") {
            if let Some(start) = line.find("[RTCConnection(") {
                let after = &line[start + "[RTCConnection(".len()..];
                if let Some(comma) = after.find(',') {
                    let g_id = after[..comma].trim();
                    if !g_id.is_empty() && g_id.chars().all(|c| c.is_ascii_digit()) {
                        self.active_guild_id = g_id.to_string();
                    }
                }
            }
            if let Some(ch_idx) = line.find("channel: ") {
                let after_ch = &line[ch_idx + "channel: ".len()..];
                let ch_id: String = after_ch.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !ch_id.is_empty() {
                    self.active_channel_id = ch_id;
                }
            } else if let Some(up_idx) = line.find("Updating channel: ") {
                let after_ch = &line[up_idx + "Updating channel: ".len()..];
                let ch_id: String = after_ch.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !ch_id.is_empty() {
                    self.active_channel_id = ch_id;
                }
            }
        }

        // Mute & Deafen States
        if line.contains("[VOICE_MUTE]") || line.contains("self_mute: true") {
            self.is_muted = true;
        } else if line.contains("self_mute: false") {
            self.is_muted = false;
        }

        if line.contains("self_deaf: true") {
            self.is_deafened = true;
        } else if line.contains("self_deaf: false") {
            self.is_deafened = false;
        }
    }
}

fn worker(sink: EventSink, rx: Receiver<()>) {
    let mut last_state = DiscordVoiceState::default();
    let mut scanner = DiscordLogScanner::new();

    loop {
        // Poll every 60ms for instant voice in/out reactivity (<100ms)
        match rx.recv_timeout(Duration::from_millis(60)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        let mut state = if let Some(path) = discord_log_path() {
            scanner.scan(&path)
        } else {
            DiscordVoiceState::default()
        };

        if state.in_voice {
            // Real-time microphone peak detection using Windows Core Audio API
            let peak = sample_wasapi_mic_peak();
            CURRENT_MIC_PEAK.store(peak.to_bits(), Ordering::Relaxed);
            state.mic_peak = peak;
            let norm_peak = ((peak - 0.035).max(0.0) / 0.25).min(1.0);
            state.is_speaking = norm_peak > 0.05 && !state.is_muted;
        } else {
            CURRENT_MIC_PEAK.store(0, Ordering::Relaxed);
        }

        let meaningful_change = state.in_voice != last_state.in_voice
            || state.is_muted != last_state.is_muted
            || state.is_deafened != last_state.is_deafened
            || state.is_speaking != last_state.is_speaking
            || state.server != last_state.server
            || state.channel != last_state.channel
            || (state.mic_peak - last_state.mic_peak).abs() > 0.08;

        if meaningful_change {
            sink.send(AppEvent::Discord(state.clone()));
            last_state = state;
        }
    }
}
