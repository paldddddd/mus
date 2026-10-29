use anyhow::{Context, Result};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tracing::{info, warn};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

const FFT_SIZE: usize = 1024;
const DEFAULT_BARS: usize = 32;

pub struct Visualizer {
    running: Arc<AtomicBool>,
    bands: Arc<Mutex<Vec<f32>>>,
    paused: Arc<AtomicBool>,
}

impl Visualizer {
    pub fn new() -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let paused = Arc::new(AtomicBool::new(false));
        let bands = Arc::new(Mutex::new(vec![0.0f32; DEFAULT_BARS]));

        let running_clone = running.clone();
        let paused_clone = paused.clone();
        let bands_clone = bands.clone();

        thread::Builder::new()
            .name("audio-visualizer".to_string())
            .spawn(move || {
                // Reconnect loop: if audio device drops, changes, or WASAPI fails, recover automatically
                while running_clone.load(Ordering::SeqCst) {
                    if let Err(e) = run_loopback_capture(running_clone.clone(), paused_clone.clone(), bands_clone.clone()) {
                        warn!("WASAPI loopback visualizer error: {e:#}; reconnecting in 1s...");
                        if let Ok(mut g) = bands_clone.lock() {
                            g.fill(0.0);
                        }
                        for _ in 0..10 {
                            if !running_clone.load(Ordering::SeqCst) {
                                return;
                            }
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            })
            .expect("Failed to spawn audio visualizer thread");

        Self {
            running,
            bands,
            paused,
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    pub fn get_spectrum(&self, num_bars: usize) -> Vec<f32> {
        let current = self.bands.lock().unwrap();
        if current.len() == num_bars {
            current.clone()
        } else {
            // Resample to requested number of bars
            resample_bands(&current, num_bars)
        }
    }
}

impl Drop for Visualizer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Default for Visualizer {
    fn default() -> Self {
        Self::new()
    }
}

fn run_loopback_capture(
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    bands: Arc<Mutex<Vec<f32>>>,
) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .context("Failed to create MMDeviceEnumerator")?;

        let device: IMMDevice = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .context("Failed to get default audio render endpoint")?;

        let audio_client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .context("Failed to activate IAudioClient")?;

        let wave_format_ptr = audio_client
            .GetMixFormat()
            .context("Failed to get mix format")?;
        let wave_format = *wave_format_ptr;
        let channels = wave_format.nChannels as usize;
        let sample_rate = wave_format.nSamplesPerSec;

        let is_float = wave_format.wFormatTag == 3 // WAVE_FORMAT_IEEE_FLOAT
            || (wave_format.wFormatTag == 0xFFFE // WAVE_FORMAT_EXTENSIBLE
                && wave_format.wBitsPerSample == 32);

        // Period 10ms (100,000 in 100ns units)
        let buffer_duration: i64 = 100_000 * 2;
        audio_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                buffer_duration,
                0,
                wave_format_ptr,
                None,
            )
            .context("Failed to initialize IAudioClient for loopback")?;

        let capture_client: IAudioCaptureClient = audio_client
            .GetService()
            .context("Failed to get IAudioCaptureClient")?;

        audio_client.Start().context("Failed to start IAudioClient")?;
        info!("WASAPI loopback audio capture started ({} Hz, {} channels, float={})", sample_rate, channels, is_float);

        let mut sample_ring = vec![0.0f32; FFT_SIZE];
        let mut sample_idx = 0;

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let mut fft_buffer: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); FFT_SIZE];
        let mut smoothed_bands = vec![0.0f32; DEFAULT_BARS];
        let mut dynamic_peak: f32 = 0.015;

        let half_n = FFT_SIZE / 2;
        let mut magnitudes = vec![0.0f32; half_n];
        let mut raw_energies = vec![0.0f32; DEFAULT_BARS];
        let mut new_bands = vec![0.0f32; DEFAULT_BARS];

        // Precompute Hann window
        let hann_window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos()))
            .collect();

        while running.load(Ordering::SeqCst) {
            if paused.load(Ordering::SeqCst) {
                // Smoothly decay to zero when paused
                for b in &mut smoothed_bands {
                    *b *= 0.85;
                }
                if let Ok(mut g) = bands.lock() {
                    g.copy_from_slice(&smoothed_bands);
                }
                thread::sleep(Duration::from_millis(50));
                continue;
            }

            let mut got_audio_frames = false;
            loop {
                match capture_client.GetNextPacketSize() {
                    Ok(0) => break,
                    Ok(_) => {
                        let mut data_ptr: *mut u8 = std::ptr::null_mut();
                        let mut num_frames: u32 = 0;
                        let mut flags: u32 = 0;

                        if let Err(err) = capture_client.GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None) {
                            warn!("WASAPI GetBuffer failed: {err:?}");
                            return Err(err.into());
                        }

                        got_audio_frames = true;
                        if (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data_ptr.is_null() {
                            // Silent packet
                            for _ in 0..num_frames {
                                sample_ring[sample_idx] = 0.0;
                                sample_idx = (sample_idx + 1) % FFT_SIZE;
                            }
                        } else if is_float {
                            let float_slice = std::slice::from_raw_parts(data_ptr as *const f32, (num_frames as usize) * channels);
                            for frame in float_slice.chunks_exact(channels) {
                                // Downmix to mono with NaN/Inf protection
                                let mono: f32 = frame.iter().sum::<f32>() / (channels as f32);
                                let safe_mono = if mono.is_finite() { mono.clamp(-3.0, 3.0) } else { 0.0 };
                                sample_ring[sample_idx] = safe_mono;
                                sample_idx = (sample_idx + 1) % FFT_SIZE;
                            }
                        } else {
                            // 16-bit PCM integer
                            let int16_slice = std::slice::from_raw_parts(data_ptr as *const i16, (num_frames as usize) * channels);
                            for frame in int16_slice.chunks_exact(channels) {
                                let mono: f32 = frame.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / (channels as f32);
                                let safe_mono = if mono.is_finite() { mono.clamp(-3.0, 3.0) } else { 0.0 };
                                sample_ring[sample_idx] = safe_mono;
                                sample_idx = (sample_idx + 1) % FFT_SIZE;
                            }
                        }

                        let _ = capture_client.ReleaseBuffer(num_frames);
                    }
                    Err(err) => {
                        warn!("WASAPI GetNextPacketSize error: {err:?}; triggering loopback reinitialization");
                        return Err(err.into());
                    }
                }
            }

            if !got_audio_frames {
                // Smoothly decay ring buffer when no new audio packets arrive (e.g. paused stream)
                for s in &mut sample_ring {
                    *s *= 0.80;
                }
            }

            // Compute FFT
            for i in 0..FFT_SIZE {
                let s = sample_ring[(sample_idx + i) % FFT_SIZE];
                fft_buffer[i] = Complex::new(s * hann_window[i], 0.0);
            }

            fft.process(&mut fft_buffer);

            // Compute magnitude for frequency bins (first half of FFT, *2.0 for Hann window coherent gain)
            for i in 0..half_n {
                let mag = (fft_buffer[i].re.powi(2) + fft_buffer[i].im.powi(2)).sqrt() * 2.0 / (FFT_SIZE as f32);
                magnitudes[i] = if mag.is_finite() { mag.clamp(0.0, 50.0) } else { 0.0 };
            }

            // Map magnitudes to frequency bands using musical logarithmic frequency scale
            let num_bands = DEFAULT_BARS;
            raw_energies.fill(0.0);
            let f_min = 45.0f32;
            let f_max = 16000.0f32;
            let sr = (sample_rate as f32).max(8000.0);

            for (b, val) in raw_energies.iter_mut().enumerate() {
                let start_freq = f_min * (f_max / f_min).powf(b as f32 / num_bands as f32);
                let end_freq = f_min * (f_max / f_min).powf((b + 1) as f32 / num_bands as f32);

                let start_bin = ((start_freq / sr) * FFT_SIZE as f32).floor() as usize;
                let end_bin = ((end_freq / sr) * FFT_SIZE as f32).ceil() as usize;
                let start_bin = start_bin.clamp(1, half_n - 1);
                let end_bin = end_bin.clamp(start_bin + 1, half_n);

                let slice = &magnitudes[start_bin..end_bin];
                let (rms, peak) = if !slice.is_empty() {
                    let sum_sq: f32 = slice.iter().map(|&x| x * x).sum();
                    let peak_v: f32 = slice.iter().copied().fold(0.0f32, f32::max);
                    ((sum_sq / slice.len() as f32).sqrt(), peak_v)
                } else {
                    (0.0, 0.0)
                };
                let energy = peak.max(rms * 1.3);

                // Progressive pink-noise acoustic compensation so all musical registers bounce in harmony
                let tilt = 1.8 + (b as f32 / num_bands as f32).powf(0.85) * 3.5;
                let raw = energy * tilt;
                *val = if raw.is_finite() { raw.clamp(0.0, 50.0) } else { 0.0 };
            }

            // Adaptive Automatic Gain Control (AGC): tracks playback volume level dynamically with NaN/Inf guards
            let frame_peak = raw_energies.iter().copied().fold(0.0f32, f32::max);
            if !dynamic_peak.is_finite() || dynamic_peak > 25.0 || dynamic_peak < 0.001 {
                dynamic_peak = 0.015;
            }
            if frame_peak.is_finite() && frame_peak > dynamic_peak {
                dynamic_peak = dynamic_peak * 0.45 + frame_peak * 0.55;
            } else {
                dynamic_peak = (dynamic_peak * 0.993).max(0.002);
            }

            new_bands.fill(0.0);
            for (b, val) in new_bands.iter_mut().enumerate() {
                let raw = raw_energies[b];
                if !raw.is_finite() || raw <= 0.0001 {
                    *val = 0.0;
                } else {
                    let norm = (raw / dynamic_peak.max(0.002)).clamp(0.0, 1.0);
                    // Gamma curve for punchy bass and lively dynamics
                    *val = norm.powf(0.78);
                }
            }

            // Apply Attack/Decay dynamics smoothing
            for b in 0..num_bands {
                if !smoothed_bands[b].is_finite() {
                    smoothed_bands[b] = 0.0;
                }
                if new_bands[b] > smoothed_bands[b] {
                    // Instant punchy attack
                    smoothed_bands[b] += (new_bands[b] - smoothed_bands[b]) * 0.88;
                } else {
                    // Snappy organic fall
                    smoothed_bands[b] = (smoothed_bands[b] * 0.82 - 0.005).max(0.0);
                }
                smoothed_bands[b] = smoothed_bands[b].clamp(0.0, 1.0);
            }

            // Publish smoothed bands
            if let Ok(mut g) = bands.lock() {
                g.copy_from_slice(&smoothed_bands);
            }

            thread::sleep(Duration::from_millis(16)); // ~60 FPS
        }

        let _ = audio_client.Stop();
    }

    Ok(())
}

fn resample_bands(source: &[f32], target_len: usize) -> Vec<f32> {
    if source.is_empty() || target_len == 0 {
        return vec![0.0; target_len];
    }
    let mut result = Vec::with_capacity(target_len);
    for i in 0..target_len {
        let start = (i * source.len()) / target_len;
        let end = (((i + 1) * source.len()) / target_len).max(start + 1).min(source.len());
        let peak = source[start..end].iter().copied().fold(0.0f32, f32::max);
        result.push(peak);
    }
    result
}
