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
                if let Err(e) = run_loopback_capture(running_clone, paused_clone, bands_clone) {
                    warn!("WASAPI loopback visualizer thread ended: {e}");
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

            while let Ok(size) = capture_client.GetNextPacketSize() {
                if size == 0 {
                    break;
                }

                let mut data_ptr: *mut u8 = std::ptr::null_mut();
                let mut num_frames: u32 = 0;
                let mut flags: u32 = 0;

                if capture_client
                    .GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)
                    .is_ok()
                {
                    if (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data_ptr.is_null() {
                        // Silent packet
                        for _ in 0..num_frames {
                            sample_ring[sample_idx] = 0.0;
                            sample_idx = (sample_idx + 1) % FFT_SIZE;
                        }
                    } else if is_float {
                        let float_slice = std::slice::from_raw_parts(data_ptr as *const f32, (num_frames as usize) * channels);
                        for frame in float_slice.chunks_exact(channels) {
                            // Downmix to mono
                            let mono: f32 = frame.iter().sum::<f32>() / (channels as f32);
                            sample_ring[sample_idx] = mono;
                            sample_idx = (sample_idx + 1) % FFT_SIZE;
                        }
                    } else {
                        // 16-bit PCM integer
                        let int16_slice = std::slice::from_raw_parts(data_ptr as *const i16, (num_frames as usize) * channels);
                        for frame in int16_slice.chunks_exact(channels) {
                            let mono: f32 = frame.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / (channels as f32);
                            sample_ring[sample_idx] = mono;
                            sample_idx = (sample_idx + 1) % FFT_SIZE;
                        }
                    }

                    let _ = capture_client.ReleaseBuffer(num_frames);
                }
            }

            // Compute FFT
            for i in 0..FFT_SIZE {
                let s = sample_ring[(sample_idx + i) % FFT_SIZE];
                fft_buffer[i] = Complex::new(s * hann_window[i], 0.0);
            }

            fft.process(&mut fft_buffer);

            // Compute magnitude for frequency bins (first half of FFT)
            let half_n = FFT_SIZE / 2;
            let mut magnitudes = vec![0.0f32; half_n];
            for i in 0..half_n {
                let mag = (fft_buffer[i].re.powi(2) + fft_buffer[i].im.powi(2)).sqrt() / (FFT_SIZE as f32);
                magnitudes[i] = mag;
            }

            // Map magnitudes to frequency bands (logarithmic grouping)
            let num_bands = DEFAULT_BARS;
            let mut new_bands = vec![0.0f32; num_bands];
            for (b, val) in new_bands.iter_mut().enumerate() {
                // Logarithmic frequency index from bin 1 (avoid DC) to half_n
                let start_ratio = (b as f32 / num_bands as f32).powf(2.0);
                let end_ratio = ((b + 1) as f32 / num_bands as f32).powf(2.0);

                let start_bin = ((1.0 + start_ratio * (half_n - 2) as f32) as usize).clamp(1, half_n - 1);
                let end_bin = ((1.0 + end_ratio * (half_n - 1) as f32) as usize).clamp(start_bin + 1, half_n);

                let slice = &magnitudes[start_bin..end_bin];
                let band_sum: f32 = slice.iter().sum();
                let avg = if !slice.is_empty() { band_sum / (slice.len() as f32) } else { 0.0 };

                // Scale up with high-frequency equalization boost and log compression
                let freq_boost = 1.0 + (b as f32 / num_bands as f32) * 3.5;
                *val = (avg * 35.0 * freq_boost).min(1.0);
            }

            // Apply Attack/Decay dynamics smoothing
            for b in 0..num_bands {
                if new_bands[b] > smoothed_bands[b] {
                    // Attack (fast rise)
                    smoothed_bands[b] += (new_bands[b] - smoothed_bands[b]) * 0.75;
                } else {
                    // Decay (smooth fall)
                    smoothed_bands[b] *= 0.85;
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
    if source.is_empty() {
        return vec![0.0; target_len];
    }
    let mut result = Vec::with_capacity(target_len);
    for i in 0..target_len {
        let src_idx = (i as f32 / target_len as f32 * source.len() as f32) as usize;
        result.push(source[src_idx.min(source.len() - 1)]);
    }
    result
}
