//! Master volume change notifications (Core Audio) and hardware volume key hook.
//!
//! A dedicated MTA thread owns the `IMMDeviceEnumerator` and the
//! `IAudioEndpointVolume` of the default render endpoint (console role).
//! Volume changes arrive on an audio-engine thread through
//! [`VolumeCallback::OnNotify`], which only takes a short lock, drops
//! duplicates and stale endpoints, and posts `AppEvent::Volume`.
//! Default-device changes arrive through [`DeviceClient`], which only wakes
//! the worker; the worker re-binds to the new endpoint (Core Audio forbids
//! (un)registering from inside its own callbacks) and reports its state.
//!
//! A low-level keyboard hook intercepts VK_VOLUME_UP, VK_VOLUME_DOWN, and
//! VK_VOLUME_MUTE, blocks the native Windows OSD flyout, and routes the adjustments
//! directly through Core Audio, triggering the Dynamic Island volume HUD.
//!
//! Idle cost: the worker sleeps in a blocking `recv`; nothing polls.

use crate::events::{AppEvent, EventSink};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::core::{implement, Result, PCWSTR};
use windows::Win32::Foundation::{BOOL, E_FAIL, LPARAM, LRESULT, WPARAM};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl};
use windows::Win32::Media::Audio::{
    eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
    MMDeviceEnumerator, AUDCLNT_E_DEVICE_INVALIDATED, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// How long `Drop` waits for the worker to unregister before detaching it.
const STOP_TIMEOUT: Duration = Duration::from_millis(800);

static HOOK_TX: Mutex<Option<Sender<Msg>>> = Mutex::new(None);
static HOOK_HANDLE: Mutex<Option<usize>> = Mutex::new(None);

unsafe extern "system" fn ll_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = kbd.vkCode;
        if vk == VK_VOLUME_UP.0 as u32 || vk == VK_VOLUME_DOWN.0 as u32 || vk == VK_VOLUME_MUTE.0 as u32 {
            let is_down = wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize;
            if is_down {
                if let Ok(guard) = HOOK_TX.lock() {
                    if let Some(tx) = guard.as_ref() {
                        if vk == VK_VOLUME_UP.0 as u32 {
                            let _ = tx.send(Msg::StepUp);
                        } else if vk == VK_VOLUME_DOWN.0 as u32 {
                            let _ = tx.send(Msg::StepDown);
                        } else if vk == VK_VOLUME_MUTE.0 as u32 {
                            let _ = tx.send(Msg::ToggleMute);
                        }
                    }
                }
            }
            // Return 1: suppress keystroke so Windows Shell does not spawn default volume flyout
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Master-volume watcher and hardware HUD replacement hook.
pub struct AudioService {
    tx: Sender<Msg>,
    done: Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl AudioService {
    pub fn start(sink: EventSink) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let client_tx = tx.clone();

        if let Ok(mut guard) = HOOK_TX.lock() {
            *guard = Some(client_tx.clone());
        }
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), None, 0) };
        if let Ok(h) = hook {
            if let Ok(mut guard) = HOOK_HANDLE.lock() {
                *guard = Some(h.0 as usize);
            }
            crate::pinfo!("audio: low-level volume hook active (Windows OSD suppressed)");
        } else if let Err(e) = hook {
            crate::pwarn!("audio: failed to install volume hook ({e})");
        }

        let thread = std::thread::Builder::new()
            .name("poni-audio".into())
            .spawn(move || {
                worker(sink, rx, client_tx);
                let _ = done_tx.send(());
            })
            .map_err(|e| windows::core::Error::new(E_FAIL, format!("audio thread: {e}")))?;
        Ok(Self { tx, done, thread: Some(thread) })
    }
}

impl Drop for AudioService {
    fn drop(&mut self) {
        if let Ok(mut guard) = HOOK_HANDLE.lock() {
            if let Some(raw) = guard.take() {
                let _ = unsafe { UnhookWindowsHookEx(HHOOK(raw as *mut _)) };
                crate::pinfo!("audio: volume hook unhooked (Windows OSD restored)");
            }
        }
        if let Ok(mut guard) = HOOK_TX.lock() {
            *guard = None;
        }

        let _ = self.tx.send(Msg::Shutdown);
        let Some(thread) = self.thread.take() else { return };
        match self.done.recv_timeout(STOP_TIMEOUT) {
            // Finished (or died): joining is immediate.
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
            }
            Err(RecvTimeoutError::Timeout) => crate::pwarn!("audio: worker did not stop in time; detaching it"),
        }
    }
}

enum Msg {
    /// The default render endpoint (console or multimedia role) changed.
    DefaultChanged,
    StepUp,
    StepDown,
    ToggleMute,
    Shutdown,
}

// ---------------------------------------------------------------------------
// Publishing: dedupe + stale-endpoint filter, shared with the COM callbacks.

struct GateState {
    /// Bumped on every re-bind; callbacks of older endpoints are ignored.
    generation: u32,
    /// Last reported (level in 1/10000 steps, muted).
    last: Option<(u32, bool)>,
}

struct Gate {
    sink: EventSink,
    state: Mutex<GateState>,
}

impl Gate {
    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Start a new generation (before binding a new endpoint).
    fn next_generation(&self) -> u32 {
        let mut s = self.lock();
        s.generation = s.generation.wrapping_add(1);
        s.generation
    }

    /// Report `level`/`muted` unless it comes from a stale endpoint or
    /// repeats the last report. Cheap: one lock, no allocation on the
    /// duplicate path. Holding the lock while sending keeps reports ordered
    /// with respect to re-binds.
    fn publish(&self, generation: u32, level: f32, muted: bool) {
        let level = if level.is_finite() { level.clamp(0.0, 1.0) } else { 0.0 };
        let key = ((level * 10_000.0).round() as u32, muted);
        let mut s = self.lock();
        if s.generation != generation || s.last == Some(key) {
            return;
        }
        s.last = Some(key);
        self.sink.send(AppEvent::Volume { level, muted });
    }
}

// ---------------------------------------------------------------------------
// COM callbacks.

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    gate: Arc<Gate>,
    generation: u32,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        if let Some(data) = unsafe { pnotify.as_ref() } {
            self.gate.publish(self.generation, data.fMasterVolume, data.bMuted.as_bool());
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceClient {
    tx: Sender<Msg>,
}

impl IMMNotificationClient_Impl for DeviceClient_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> Result<()> {
        if flow == eRender && (role == eConsole || role == eMultimedia) {
            let _ = self.tx.send(Msg::DefaultChanged);
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Worker thread.

struct ComApartment {
    initialized: bool,
}

impl ComApartment {
    fn enter_mta() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_err() {
            crate::pwarn!("audio: CoInitializeEx failed ({hr:?})");
        }
        Self { initialized: hr.is_ok() }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

struct Binding {
    id: String,
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl Binding {
    fn unbind(self) {
        match unsafe { self.volume.UnregisterControlChangeNotify(&self.callback) } {
            Err(e) if e.code() == AUDCLNT_E_DEVICE_INVALIDATED => {}
            Err(e) => crate::pwarn!("audio: UnregisterControlChangeNotify failed ({e})"),
            Ok(()) => {}
        }
    }
}

fn worker(sink: EventSink, rx: Receiver<Msg>, client_tx: Sender<Msg>) {
    let com = ComApartment::enter_mta();
    if !com.initialized {
        return;
    }

    let enumerator: IMMDeviceEnumerator = match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
        Ok(e) => e,
        Err(e) => {
            crate::pwarn!("audio: MMDeviceEnumerator unavailable ({e}); volume pop-ups disabled");
            return;
        }
    };

    let client: IMMNotificationClient = DeviceClient { tx: client_tx }.into();
    let client_registered = match unsafe { enumerator.RegisterEndpointNotificationCallback(&client) } {
        Ok(()) => true,
        Err(e) => {
            crate::pwarn!("audio: default-device notifications unavailable ({e})");
            false
        }
    };

    let gate = Arc::new(Gate { sink, state: Mutex::new(GateState { generation: 0, last: None }) });
    let mut binding = rebind(&enumerator, &gate, None);

    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Shutdown => break,
            Msg::StepUp => {
                if let Some(b) = &binding {
                    let _ = unsafe { b.volume.VolumeStepUp(std::ptr::null()) };
                }
            }
            Msg::StepDown => {
                if let Some(b) = &binding {
                    let _ = unsafe { b.volume.VolumeStepDown(std::ptr::null()) };
                }
            }
            Msg::ToggleMute => {
                if let Some(b) = &binding {
                    if let Ok(m) = unsafe { b.volume.GetMute() } {
                        let _ = unsafe { b.volume.SetMute(BOOL::from(!m.as_bool()), std::ptr::null()) };
                    }
                }
            }
            Msg::DefaultChanged => {
                let mut shutdown = false;
                while let Ok(m) = rx.try_recv() {
                    if matches!(m, Msg::Shutdown) {
                        shutdown = true;
                        break;
                    }
                }
                if shutdown {
                    break;
                }
                binding = rebind(&enumerator, &gate, binding);
            }
        }
    }

    if let Some(b) = binding {
        b.unbind();
    }
    if client_registered {
        if let Err(e) = unsafe { enumerator.UnregisterEndpointNotificationCallback(&client) } {
            crate::pwarn!("audio: UnregisterEndpointNotificationCallback failed ({e})");
        }
    }
}

fn rebind(enumerator: &IMMDeviceEnumerator, gate: &Arc<Gate>, current: Option<Binding>) -> Option<Binding> {
    let device = match unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
        Ok(d) => d,
        Err(e) => {
            crate::pinfo!("audio: no default output device ({e})");
            gate.next_generation();
            if let Some(b) = current {
                b.unbind();
            }
            return None;
        }
    };
    let id = device_id(&device);
    if let Some(b) = &current {
        if !id.is_empty() && b.id == id && unsafe { b.volume.GetMasterVolumeLevelScalar() }.is_ok() {
            return current;
        }
    }

    let generation = gate.next_generation();
    if let Some(b) = current {
        b.unbind();
    }
    match bind(&device, id, gate, generation) {
        Ok(b) => {
            match unsafe { (b.volume.GetMasterVolumeLevelScalar(), b.volume.GetMute()) } {
                (Ok(level), Ok(muted)) => gate.publish(generation, level, muted.as_bool()),
                (Err(e), _) | (_, Err(e)) => crate::pwarn!("audio: reading the volume failed ({e})"),
            }
            Some(b)
        }
        Err(e) => {
            crate::pwarn!("audio: binding the default output device failed ({e})");
            None
        }
    }
}

fn bind(device: &IMMDevice, id: String, gate: &Arc<Gate>, generation: u32) -> Result<Binding> {
    let volume: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None)? };
    let callback: IAudioEndpointVolumeCallback = VolumeCallback { gate: Arc::clone(gate), generation }.into();
    unsafe { volume.RegisterControlChangeNotify(&callback)? };
    Ok(Binding { id, volume, callback })
}

fn device_id(device: &IMMDevice) -> String {
    match unsafe { device.GetId() } {
        Ok(p) if !p.is_null() => {
            let id = unsafe { p.to_string() }.unwrap_or_default();
            unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
            id
        }
        _ => String::new(),
    }
}
