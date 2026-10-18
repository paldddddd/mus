use anyhow::{Context, Result};
use std::mem::size_of;
use windows::Win32::System::Threading::{
    GetCurrentProcess, ProcessPowerThrottling, SetPriorityClass, SetProcessInformation,
    BELOW_NORMAL_PRIORITY_CLASS,
};

#[repr(C)]
struct PROCESS_POWER_THROTTLING_STATE {
    version: u32,
    control_mask: u32,
    state_mask: u32,
}

const PROCESS_POWER_THROTTLING_CURRENT_VERSION: u32 = 1;
const PROCESS_POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;

/// Configure process priority to Below Normal and enable Windows EcoQoS (Efficiency Mode).
pub fn apply_eco_profile() -> Result<()> {
    unsafe {
        let current_process = GetCurrentProcess();

        // 1. Set priority to Below Normal
        SetPriorityClass(current_process, BELOW_NORMAL_PRIORITY_CLASS)
            .ok()
            .context("SetPriorityClass(BELOW_NORMAL) failed")?;

        // 2. Enable EcoQoS (Power Throttling Execution Speed)
        let mut throttling = PROCESS_POWER_THROTTLING_STATE {
            version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            control_mask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            state_mask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        };

        // SetProcessInformation is supported on Windows 10 1709+
        let _ = SetProcessInformation(
            current_process,
            ProcessPowerThrottling,
            &mut throttling as *mut _ as *const _,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        tracing::info!("Applied process profile: Below Normal priority + EcoQoS (Efficiency Mode)");
    }

    Ok(())
}
