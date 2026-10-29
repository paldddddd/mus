use anyhow::Result;
use std::mem::size_of;
use windows::Win32::System::Threading::{
    GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation,
};

#[repr(C)]
struct PROCESS_POWER_THROTTLING_STATE {
    version: u32,
    control_mask: u32,
    state_mask: u32,
}

const PROCESS_POWER_THROTTLING_CURRENT_VERSION: u32 = 1;
const PROCESS_POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;

/// Configure process profile without throttling real-time timer/render responsiveness.
pub fn apply_eco_profile() -> Result<()> {
    unsafe {
        let current_process = GetCurrentProcess();

        // Explicitly disable execution speed throttling (EcoQoS) so 60 FPS animations,
        // mouse parallax, and audio visualizer are never throttled to 15-20 Hz by Windows.
        let mut throttling = PROCESS_POWER_THROTTLING_STATE {
            version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            control_mask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            state_mask: 0,
        };

        let _ = SetProcessInformation(
            current_process,
            ProcessPowerThrottling,
            &mut throttling as *mut _ as *const _,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        tracing::info!("Applied responsive process profile (EcoQoS throttling disabled)");
    }

    Ok(())
}
