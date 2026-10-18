use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperAudioConfig {
    pub volume: u32, // 0 - 100
    pub muted: bool,
    pub crossfade_ms: u64, // e.g. 300 ms
}

impl Default for WallpaperAudioConfig {
    fn default() -> Self {
        Self {
            volume: 50,
            muted: false,
            crossfade_ms: 300,
        }
    }
}

pub struct WallpaperAudio {
    config: Arc<Mutex<WallpaperAudioConfig>>,
    current_volume: Arc<Mutex<f32>>,
    fading: Arc<AtomicBool>,
}

impl WallpaperAudio {
    pub fn new(config: WallpaperAudioConfig) -> Self {
        let initial_vol = if config.muted {
            0.0
        } else {
            config.volume as f32 / 100.0
        };

        Self {
            config: Arc::new(Mutex::new(config)),
            current_volume: Arc::new(Mutex::new(initial_vol)),
            fading: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_volume(&self, volume: u32) {
        let mut cfg = self.config.lock().unwrap();
        cfg.volume = volume.min(100);
        if !cfg.muted {
            *self.current_volume.lock().unwrap() = cfg.volume as f32 / 100.0;
        }
    }

    pub fn set_muted(&self, muted: bool) {
        let mut cfg = self.config.lock().unwrap();
        cfg.muted = muted;
        if muted {
            *self.current_volume.lock().unwrap() = 0.0;
        } else {
            *self.current_volume.lock().unwrap() = cfg.volume as f32 / 100.0;
        }
    }

    pub fn is_muted(&self) -> bool {
        self.config.lock().unwrap().muted
    }

    pub fn get_effective_volume(&self) -> f32 {
        *self.current_volume.lock().unwrap()
    }

    pub fn fade_out(&self) {
        let crossfade_ms = self.config.lock().unwrap().crossfade_ms;
        let current_vol_arc = self.current_volume.clone();
        let fading_arc = self.fading.clone();

        thread::spawn(move || {
            fading_arc.store(true, Ordering::SeqCst);
            let steps = 15;
            let step_dur = Duration::from_millis(crossfade_ms / steps);

            for _ in 0..steps {
                if !fading_arc.load(Ordering::SeqCst) {
                    break;
                }
                {
                    let mut vol = current_vol_arc.lock().unwrap();
                    *vol = (*vol * 0.8).max(0.0);
                }
                thread::sleep(step_dur);
            }
            *current_vol_arc.lock().unwrap() = 0.0;
            fading_arc.store(false, Ordering::SeqCst);
        });
    }

    pub fn fade_in(&self) {
        let (target_vol, crossfade_ms) = {
            let cfg = self.config.lock().unwrap();
            if cfg.muted {
                (0.0, cfg.crossfade_ms)
            } else {
                (cfg.volume as f32 / 100.0, cfg.crossfade_ms)
            }
        };

        if target_vol <= 0.0 {
            return;
        }

        let current_vol_arc = self.current_volume.clone();
        let fading_arc = self.fading.clone();

        thread::spawn(move || {
            fading_arc.store(true, Ordering::SeqCst);
            let steps = 15;
            let step_dur = Duration::from_millis(crossfade_ms / steps);
            let increment = target_vol / (steps as f32);

            *current_vol_arc.lock().unwrap() = 0.0;
            for _ in 0..steps {
                if !fading_arc.load(Ordering::SeqCst) {
                    break;
                }
                {
                    let mut vol = current_vol_arc.lock().unwrap();
                    *vol = (*vol + increment).min(target_vol);
                }
                thread::sleep(step_dur);
            }
            *current_vol_arc.lock().unwrap() = target_vol;
            fading_arc.store(false, Ordering::SeqCst);
        });
    }
}

impl Default for WallpaperAudio {
    fn default() -> Self {
        Self::new(WallpaperAudioConfig::default())
    }
}
