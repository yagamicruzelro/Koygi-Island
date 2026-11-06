//! Real-time System Audio Loopback Spectrum Analyzer (8 Bands).
//!
//! Captures system output audio (speakers/headphones where Spotify plays)
//! via WASAPI Loopback, processes PCM audio through 8 frequency bands:
//! 1. Sub-Bass (< 90 Hz): Sub kicks, 808 rumble
//! 2. Bass (90 - 200 Hz): Kick punch, bass guitar
//! 3. Low-Mid (200 - 450 Hz): Warmth, guitar body, snare bottom
//! 4. Mid (450 - 900 Hz): Lead guitar, keyboard, vocal fullness
//! 5. Upper-Mid (900 - 1800 Hz): Vocal clarity, snare crack
//! 6. Presence (1800 - 3500 Hz): Attack, consonant sounds
//! 7. Treble (3500 - 7000 Hz): Hi-hats, tambourine, sizzle
//! 8. Brilliance (> 7000 Hz): Air, cymbal shimmer
//!
//! Applies non-linear dynamic expansion (x^0.68) so that even at low
//! Spotify volume (10-30%), the equalizer dances with rich, fluid energy.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::core::Result;
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    eConsole, eRender, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
    IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

pub const NUM_BANDS: usize = 8;

static BANDS: [AtomicU32; NUM_BANDS] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];

/// Returns the current 8 frequency band levels [0.0..1.0].
pub fn current_bands() -> [f32; NUM_BANDS] {
    let mut out = [0.0f32; NUM_BANDS];
    for i in 0..NUM_BANDS {
        out[i] = f32::from_bits(BANDS[i].load(Ordering::Relaxed));
    }
    out
}

pub struct AudioMeterService {
    tx: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl AudioMeterService {
    pub fn start() -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("poni-audio-meter".into())
            .spawn(move || {
                let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                worker(rx);
                unsafe { CoUninitialize() };
            })
            .map_err(|e| windows::core::Error::new(E_FAIL, format!("audio meter thread: {e}")))?;
        Ok(Self { tx, thread: Some(thread) })
    }
}

impl Drop for AudioMeterService {
    fn drop(&mut self) {
        let _ = self.tx.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn worker(rx: Receiver<()>) {
    let mut smooth_levels = [0.0f32; NUM_BANDS];
    let mut y = [0.0f32; 7];
    let mut rolling_peaks = [0.06f32; 3];

    struct LoopbackStream {
        client: IAudioClient,
        capture: IAudioCaptureClient,
        channels: usize,
        alphas: [f32; 7],
    }

    fn init_stream() -> Option<LoopbackStream> {
        unsafe {
            let enumerator = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let client = device.Activate::<IAudioClient>(CLSCTX_ALL, None).ok()?;
            let pwfx = client.GetMixFormat().ok()?;
            let wfx = &*pwfx;
            let sample_rate = (wfx.nSamplesPerSec as f32).max(22050.0);
            let channels = wfx.nChannels as usize;

            let hr = client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                2000000,
                0,
                pwfx,
                None,
            );
            if hr.is_err() {
                return None;
            }
            let capture = client.GetService::<IAudioCaptureClient>().ok()?;
            let _ = client.Start();

            const FREQS: [f32; 7] = [105.0, 230.0, 500.0, 1100.0, 2400.0, 4600.0, 6500.0];
            let mut alphas = [0.0f32; 7];
            for i in 0..7 {
                alphas[i] = (2.0 * std::f32::consts::PI * FREQS[i] / sample_rate).clamp(0.005, 0.99);
            }
            Some(LoopbackStream { client, capture, channels, alphas })
        }
    }

    let mut stream = init_stream();

    loop {
        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        if stream.is_none() {
            stream = init_stream();
        }

        let mut raw_bands = [0.0f32; NUM_BANDS];
        let mut loopback_ok = false;

        if let Some(s) = &stream {
            unsafe {
                let mut p_data = std::ptr::null_mut();
                let mut num_frames = 0u32;
                let mut flags = 0u32;
                let mut band_sums = [0.0f32; NUM_BANDS];
                let mut count = 0usize;

                while s.capture.GetBuffer(&mut p_data, &mut num_frames, &mut flags, None, None).is_ok() {
                    if num_frames == 0 {
                        break;
                    }

                    if (flags & 1) == 0 && !p_data.is_null() && s.channels > 0 {
                        let samples = std::slice::from_raw_parts(p_data as *const f32, (num_frames as usize) * s.channels);
                        for chunk in samples.chunks_exact(s.channels) {
                            let x = (chunk[0] + if s.channels > 1 { chunk[1] } else { chunk[0] }) * 0.5;

                            for i in 0..7 {
                                y[i] += s.alphas[i] * (x - y[i]);
                            }

                            let b0 = y[0];
                            let b1 = y[1] - y[0];
                            let b2 = y[2] - y[1];
                            let b3 = y[3] - y[2];
                            let b4 = y[4] - y[3];
                            let b5 = y[5] - y[4];
                            let b6 = y[6] - y[5];
                            let b7 = x - y[6];

                            band_sums[0] += b0 * b0;
                            band_sums[1] += b1 * b1;
                            band_sums[2] += b2 * b2;
                            band_sums[3] += b3 * b3;
                            band_sums[4] += b4 * b4;
                            band_sums[5] += b5 * b5;
                            band_sums[6] += b6 * b6;
                            band_sums[7] += b7 * b7;
                            count += 1;
                        }
                    }
                    let _ = s.capture.ReleaseBuffer(num_frames);
                }

                if count > 0 {
                    let weights: [f32; NUM_BANDS] = [3.2, 3.4, 3.0, 3.2, 4.0, 5.5, 8.0, 13.5];
                    let mut weighted = [0.0f32; NUM_BANDS];
                    for i in 0..NUM_BANDS {
                        let rms = (band_sums[i] / count as f32).sqrt();
                        weighted[i] = rms * weights[i];
                    }

                    // 3-Zone Decoupled AGC (Bass [0..2], Mid/Vocal [2..5], Treble [5..8]):
                    // Prevents heavy bass punchlines from ducking/freezing vocals and treble.
                    // Each acoustic zone maintains its own dynamic headroom so all frequencies
                    // dance concurrently with fluid, independent musical expression.
                    let zone_max = [
                        weighted[0].max(weighted[1]),
                        weighted[2].max(weighted[3]).max(weighted[4]),
                        weighted[5].max(weighted[6]).max(weighted[7]),
                    ];
                    let floors = [0.045f32, 0.040f32, 0.035f32];

                    for z in 0..3 {
                        if zone_max[z] > rolling_peaks[z] {
                            rolling_peaks[z] = rolling_peaks[z] * 0.35 + zone_max[z] * 0.65;
                        } else {
                            rolling_peaks[z] = (rolling_peaks[z] * 0.955).max(zone_max[z]).max(floors[z]);
                        }
                    }

                    // Subtle 6% beat coupling from the bass punch to mid/high bars for cohesive pulse
                    let bass_punch = ((weighted[0].max(weighted[1]) / rolling_peaks[0].max(floors[0])) - 0.45).max(0.0);

                    for i in 0..NUM_BANDS {
                        let z = if i < 2 { 0 } else if i < 5 { 1 } else { 2 };
                        let scale = rolling_peaks[z].max(floors[z]);
                        let norm = (weighted[i] / scale).min(1.4);
                        let coupled = norm + if i >= 2 { bass_punch * 0.06 } else { 0.0 };

                        // Dynamic expansion: powf(1.28) provides deep valleys and tall peaks
                        let expanded = (coupled * 0.96).powf(1.28);
                        raw_bands[i] = expanded.clamp(0.05, 0.98);
                    }
                    loopback_ok = true;
                }
            }
        }

        if !loopback_ok {
            unsafe {
                if let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    if let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                        if let Ok(meter) = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) {
                            if let Ok(peak) = meter.GetPeakValue() {
                                let norm_peak = (peak / (peak + 0.3)).clamp(0.05, 0.95);
                                for i in 0..NUM_BANDS {
                                    raw_bands[i] = norm_peak;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Instant attack & snappy rhythmic release for explosive beat punch
        for i in 0..NUM_BANDS {
            let target = raw_bands[i].clamp(0.0, 1.0);
            if target > smooth_levels[i] {
                // Instant punch: snap up to beat in 1 frame
                smooth_levels[i] = smooth_levels[i] * 0.06 + target * 0.94;
            } else {
                // Fast release so kicks drop quickly between 16th notes/double kicks
                let decay = if i < 2 {
                    0.54
                } else if i < 5 {
                    0.64
                } else {
                    0.58
                };
                smooth_levels[i] = smooth_levels[i] * decay + target * (1.0 - decay);
            }
            BANDS[i].store(smooth_levels[i].to_bits(), Ordering::Relaxed);
        }
    }
}
